use super::*;
use phistudio_plugins::{Builtin, Runtime};
use phistudio_sdk::*;
use std::sync::{Arc, Mutex, mpsc};

struct Gate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
struct Converter {
    bytes: Vec<u8>,
    gate: Option<Arc<Gate>>,
    files: Option<ProviderFiles>,
}
impl Plugin for Converter {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![Contribution::Format {
                    id: "test".into(),
                    title: "Test converter".into(),
                    extensions: vec!["test".into()],
                    bridge: FormatBridge::Chart,
                    decode: true,
                    encode: true,
                }],
                ..Default::default()
            },
            Request::Format { files, .. } => {
                if let Some(gate) = &self.gate {
                    gate.entered.send(()).unwrap();
                    gate.release
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(10))
                        .unwrap();
                    return Err("Converter failed after waiting".into());
                }
                self.files = Some(files.clone());
                Response {
                    effects: vec![Effect::Call {
                        call: ServiceCall {
                            request_id: "write".into(),
                            service: ServiceRef {
                                provider: "host".into(),
                                id: "files".into(),
                                version: "^1".parse().unwrap(),
                            },
                            method: "write".into(),
                            arguments: json!(FileWrite {
                                operation: files.operation,
                                file: files.output,
                                offset: None,
                                data: encode_file_chunk(&self.bytes).unwrap()
                            }),
                        },
                    }],
                    ..Default::default()
                }
            }
            Request::ServiceResult { reply, .. } => {
                assert!(matches!(reply.outcome, ServiceOutcome::Success { .. }));
                let files = self.files.as_ref().unwrap();
                Response {
                    effects: vec![Effect::Complete {
                        operation: files.operation,
                        file: files.output,
                        name: "converted.rpe.json".into(),
                    }],
                    ..Default::default()
                }
            }
            _ => Response::default(),
        })
    }
}
fn setup(
    bytes: Vec<u8>,
    gate: Option<Arc<Gate>>,
) -> (tempfile::TempDir, Workbench, FileManager, egui::Context) {
    let root = tempfile::tempdir().unwrap();
    let manifest = serde_json::from_value(json!({"api":1,"id":"test.provider-ui","name":"Provider UI","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["formats","services","document_read"]})).unwrap();
    let mut app = Workbench::new(ChartDocument::empty("Original", 120.).unwrap(), None);
    app.plugins.runtime = Some(
        Runtime::spawn(
            Some(root.path().into()),
            vec![Builtin {
                manifest,
                enabled: true,
                create: Arc::new(move || {
                    Box::new(Converter {
                        bytes: bytes.clone(),
                        gate: gate.clone(),
                        files: None,
                    })
                }),
            }],
        )
        .unwrap(),
    );
    app.plugins.pending.insert(0, None);
    let mut files = FileManager::new(Some(root.path().into()));
    let ctx = egui::Context::default();
    settle(&mut app, &mut files, &ctx);
    (root, app, files, ctx)
}
fn provider(operation: FormatOperation) -> Provider {
    Provider::Format {
        id: "test".into(),
        operation,
    }
}
fn origin(app: &Workbench) -> Origin {
    app.plugins.registry.plugins[0].origin.clone().unwrap()
}
fn start(app: &mut Workbench, root: &std::path::Path) -> Operation {
    let origin = origin(app);
    app.open_plugin_provider(&origin, provider(FormatOperation::Decode));
    assert!(app.plugins.providers.dialog.is_some());
    let operation = app
        .begin_plugin_provider(
            &origin,
            provider(FormatOperation::Decode),
            PluginDataExport::Preserve,
        )
        .unwrap();
    let source = root.join("source.test");
    std::fs::write(&source, b"source").unwrap();
    app.start_plugin_provider_source(
        operation.clone(),
        MediaFile::new(source, "source.test".into(), false),
    );
    operation
}
fn dirty(app: &mut Workbench) {
    app.editor
        .execute(phistudio_core::Command::AddNote {
            line: 0,
            note: phistudio_core::Note::default(),
        })
        .unwrap();
    assert!(app.editor.is_dirty());
}

#[test]
fn home_allows_only_decode_and_back_cancels_queued_snapshot_without_file_work() {
    let (_root, mut app, mut files, ctx) = setup(vec![], None);
    app.show_projects = true;
    assert!(app.modal_open());
    assert!(app.plugin_provider_available(&provider(FormatOperation::Decode)));
    assert!(!app.plugin_provider_available(&provider(FormatOperation::Encode)));
    app.plugins.open = true;
    let decode_owner = origin(&app);
    app.open_plugin_provider(&decode_owner, provider(FormatOperation::Decode));
    assert!(app.plugin_provider_modal());
    app.back();
    assert!(app.show_projects && app.plugins.open);
    assert!(!app.plugin_provider_modal());
    app.show_projects = false;
    let origin = origin(&app);
    app.open_plugin_provider(&origin, provider(FormatOperation::Encode));
    let operation = app
        .begin_plugin_provider(
            &origin,
            provider(FormatOperation::Encode),
            PluginDataExport::Preserve,
        )
        .unwrap();
    app.files.request = Some(crate::files::FileRequest::ProviderSnapshot(
        operation.clone(),
    ));
    app.back();
    assert!(!operation.is_current());
    assert!(app.files.request.is_none());
    settle(&mut app, &mut files, &ctx);
    assert!(!app.files.busy && !app.close_requested);
    assert!(app.error.is_none());
    // More than the completion-channel capacity must not strand cleanup replies.
    for _ in 0..40 {
        app.open_plugin_provider(&origin, provider(FormatOperation::Encode));
        app.begin_plugin_provider(
            &origin,
            provider(FormatOperation::Encode),
            PluginDataExport::Preserve,
        )
        .unwrap();
        app.back();
        settle(&mut app, &mut files, &ctx);
    }
    assert!(app.error.is_none());
    app.plugins.shutdown().unwrap();
}

#[test]
fn converter_failure_is_reported_while_late_failures_after_back_or_document_change_are_discarded() {
    for cancel in ["none", "back", "document"] {
        let (entered, entry) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let (root, mut app, mut files, ctx) = setup(
            vec![],
            Some(Arc::new(Gate {
                entered,
                release: Mutex::new(gate),
            })),
        );
        let operation = start(&mut app, root.path());
        entry.recv_timeout(Duration::from_secs(10)).unwrap();
        match cancel {
            "back" => app.back(),
            "document" => dirty(&mut app),
            _ => {
                // Worker failure revokes before its completion enters the UI queue.
                // Keep the pending job long enough to display that error.
                operation.cancel();
                frame(&mut app, &mut files, &ctx, vec![]);
                assert!(app.plugin_provider_modal());
            }
        }
        release.send(()).unwrap();
        settle(&mut app, &mut files, &ctx);
        assert!(!app.plugin_provider_modal());
        assert!(!operation.is_current());
        if cancel == "none" {
            assert!(app.error.as_ref().unwrap().contains("Converter failed"));
        } else {
            assert!(app.error.is_none(), "{cancel}: {:?}", app.error);
        }
        assert_eq!(app.editor.document().name(), "Original");
        assert_eq!(app.editor.is_dirty(), cancel == "document");
        app.plugins.shutdown().unwrap();
    }
}

#[test]
fn malformed_provider_output_keeps_unsaved_edits_history_and_provider_activation() {
    let (root, mut app, mut files, ctx) = setup(b"This is not RPE".to_vec(), None);
    dirty(&mut app);
    let original = app.editor.document().as_json().clone();
    let revision = app.editor.revision();
    start(&mut app, root.path());
    settle(&mut app, &mut files, &ctx);
    assert!(app.error.is_some());
    assert_eq!(app.editor.document().as_json(), &original);
    assert_eq!(app.editor.revision(), revision);
    assert!(app.editor.is_dirty() && app.editor.can_undo());
    assert!(app.files.pending_open.is_none());
    assert_eq!(
        app.plugins.registry.plugins[0].status,
        phistudio_plugins::Status::Enabled
    );
    app.plugins.shutdown().unwrap();
}

#[test]
fn converted_import_uses_the_existing_unsaved_changes_dialog_before_replacing_the_document() {
    let converted = ChartDocument::empty("Converted", 140.).unwrap();
    let (root, mut app, mut files, ctx) =
        setup(serde_json::to_vec(converted.as_json()).unwrap(), None);
    dirty(&mut app);
    let original = app.editor.document().as_json().clone();
    start(&mut app, root.path());
    settle(&mut app, &mut files, &ctx);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(app.editor.is_dirty() && app.editor.can_undo());
    let pending = app.files.pending_open.as_ref().unwrap();
    assert!(pending.is_file());
    assert_eq!(
        phistudio_core::load_chart(pending).unwrap().name(),
        "Converted"
    );
    assert!(app.files.cancel_pending_open());
    assert_eq!(app.editor.document().as_json(), &original);
    app.plugins.shutdown().unwrap();
}
