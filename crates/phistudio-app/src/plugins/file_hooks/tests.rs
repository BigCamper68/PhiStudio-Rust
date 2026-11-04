use super::*;
use crate::files::FileManager;
use phistudio_core::{ChartDocument, Command, Note};
use phistudio_plugins::{Builtin, Runtime, Status};
use phistudio_sdk::*;
use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

#[derive(Clone)]
enum Decision {
    Continue,
    Cancel(FileAction),
    Rename(String),
    Forge,
    Edit,
}
struct Gate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
struct Control {
    decision: Mutex<Decision>,
    seen: Mutex<Vec<FileIntercept>>,
    gate: Option<Arc<Gate>>,
}
struct Guard(Arc<Control>);
impl Plugin for Guard {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![Contribution::Hook {
                    point: FILE_BEFORE_HOOK.into(),
                    priority: 0,
                }],
                ..Default::default()
            },
            Request::Hook {
                point,
                payload,
                context,
            } => {
                assert_eq!(point, FILE_BEFORE_HOOK);
                assert!(context.chart.is_none() && context.selection.is_null());
                let mut data: FileIntercept = serde_json::from_value(payload.clone()).unwrap();
                self.0.seen.lock().unwrap().push(data.clone());
                if let Some(gate) = &self.0.gate {
                    gate.entered.send(()).unwrap();
                    gate.release
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(10))
                        .unwrap();
                }
                let hook = match self.0.decision.lock().unwrap().clone() {
                    Decision::Cancel(action) if action == data.action => HookDecision::Cancel {
                        reason: "Blocked file action".into(),
                    },
                    Decision::Rename(name) => {
                        data.name = Some(name);
                        HookDecision::Replace {
                            payload: json!(data),
                        }
                    }
                    Decision::Forge => {
                        data.version.revision += 1;
                        HookDecision::Replace {
                            payload: json!(data),
                        }
                    }
                    Decision::Edit => {
                        return Ok(Response {
                            effects: vec![Effect::Edit {
                                transaction: Transaction {
                                    version: context.version,
                                    label: "Forbidden hook edit".into(),
                                    operations: vec![PatchOperation::Replace {
                                        path: "/META/name".into(),
                                        value: json!("Forbidden"),
                                    }],
                                },
                            }],
                            ..Default::default()
                        });
                    }
                    _ => HookDecision::Continue,
                };
                Response {
                    hook: Some(hook),
                    ..Default::default()
                }
            }
            _ => Response::default(),
        })
    }
}
struct Scene {
    app: Workbench,
    files: FileManager,
    ctx: egui::Context,
    control: Arc<Control>,
    original: PathBuf,
    root: tempfile::TempDir,
}
impl Scene {
    fn new(decision: Decision, gate: Option<Arc<Gate>>) -> Self {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let original = project.join("original.rpe.json");
        let mut value = ChartDocument::empty("Original", 120.)
            .unwrap()
            .as_json()
            .clone();
        value["futureData"] = json!({"keep":[null,{"text":"Unknown source"}]});
        let document = ChartDocument::from_value(value).unwrap();
        phistudio_core::save_chart(&document, &original).unwrap();
        let control = Arc::new(Control {
            decision: Mutex::new(decision),
            seen: Mutex::new(vec![]),
            gate,
        });
        let instance = control.clone();
        let manifest=serde_json::from_value(json!({"api":1,"id":"test.file-ui","name":"File UI","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["hooks","document_edit"]})).unwrap();
        let mut app = Workbench::new(document, Some(original.clone()));
        app.plugins.runtime = Some(
            Runtime::spawn(
                Some(root.path().into()),
                vec![Builtin {
                    manifest,
                    enabled: true,
                    create: Arc::new(move || Box::new(Guard(instance.clone()))),
                }],
            )
            .unwrap(),
        );
        app.plugins.pending.insert(0, None);
        let files = FileManager::new(Some(root.path().into()));
        let mut scene = Self {
            app,
            files,
            ctx: egui::Context::default(),
            control,
            original,
            root,
        };
        scene.settle();
        scene
    }
    fn frame(&mut self) {
        let _ = self.render(vec![]);
    }
    fn render(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.files.update(&mut self.app);
        self.ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 700.),
                )),
                events,
                ..Default::default()
            },
            |ctx| self.app.ui(ctx, false),
        )
    }
    fn settle(&mut self) {
        let end = Instant::now() + Duration::from_secs(15);
        loop {
            self.frame();
            if !self.app.plugins.working()
                && !self.app.files.busy
                && self.app.files.request.is_none()
                && !self.app.plugin_file_hook_modal()
            {
                break;
            }
            assert!(
                Instant::now() < end,
                "File hook workflow stalled: {:?}",
                self.app.error
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn dirty(&mut self) {
        self.app
            .editor
            .execute(Command::AddNote {
                line: 0,
                note: Note::default(),
            })
            .unwrap();
    }
    fn decide(&self, decision: Decision) {
        *self.control.decision.lock().unwrap() = decision;
    }
    fn end(&mut self) {
        self.files.checkpoint(&self.app, false).unwrap();
        self.app.plugins.shutdown().unwrap();
    }
}

fn click(scene: &mut Scene, label: &str) {
    let views: Vec<_> = scene
        .app
        .plugins
        .panels
        .iter()
        .map(|((_, id), p)| (p.origin.clone(), id.clone()))
        .collect();
    for (origin, id) in views {
        scene.app.open_plugin_panel(&origin, &id);
    }
    scene.frame();
    let output = scene.render(vec![]);
    let pos = output
        .shapes
        .iter()
        .rev()
        .find_map(|s| match &s.1 {
            egui::Shape::Text(t) if t.galley.job.text == label => {
                let p = t.pos + t.galley.rect.center().to_vec2();
                s.0.contains(p).then_some(p)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing file lab control: {label}"));
    for pressed in [true, false] {
        let _ = scene.render(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    scene.settle();
}

#[test]
#[ignore = "requires the separately built file-lab Wasm example"]
fn compiled_wasm_file_lab_guards_real_save_open_and_export_workflows_through_its_checkboxes() {
    let mut scene = Scene::new(Decision::Continue, None);
    scene.app.plugins.shutdown().unwrap();
    scene.app.plugins = State::default();
    scene.app.start_plugins(Some(scene.root.path().into()));
    scene.settle();
    let package =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/file-lab");
    scene
        .app
        .install_plugin(MediaFile::new(package, "File lab".into(), false));
    scene.settle();
    scene.app.enable_test_plugin("example.file-lab".into());
    scene.settle();
    assert!(scene.app.error.is_none(), "{:?}", scene.app.error);
    scene.dirty();
    let disk = std::fs::read(&scene.original).unwrap();
    click(&mut scene, "Block chart saves");
    scene.app.save(scene.original.clone());
    scene.settle();
    assert_eq!(std::fs::read(&scene.original).unwrap(), disk);
    assert!(scene.app.editor.is_dirty());
    assert!(scene.app.status.contains("blocked by File workflow lab"));
    click(&mut scene, "Block chart saves");
    scene.app.save(scene.original.clone());
    scene.settle();
    assert!(!scene.app.editor.is_dirty());
    let target = scene.original.parent().unwrap().join("next.rpe.json");
    phistudio_core::save_chart(&ChartDocument::empty("Next", 140.).unwrap(), &target).unwrap();
    click(&mut scene, "Block chart opens");
    scene.app.open(target.clone());
    scene.settle();
    assert_eq!(scene.app.editor.document().name(), "Original");
    click(&mut scene, "Block chart opens");
    scene.app.open(target);
    scene.settle();
    assert_eq!(scene.app.editor.document().name(), "Next");
    click(&mut scene, "Suggest a reviewed export name");
    scene.app.files.request = Some(FileRequest::ExportCurrent);
    scene.settle();
    assert_eq!(scene.app.files.export_path, "reviewed-project.pez");
    assert!(scene.app.files.export_package.is_some());
    assert!(scene.app.error.is_none(), "{:?}", scene.app.error);
    assert!(!scene.app.editor.is_dirty());
    scene.end();
}

#[test]
fn save_hooks_cancel_before_writing_and_continue_without_losing_source_data_or_undo() {
    let mut scene = Scene::new(Decision::Cancel(FileAction::Save), None);
    scene.dirty();
    let expected = scene.app.editor.document().as_json().clone();
    let disk = std::fs::read(&scene.original).unwrap();
    scene.app.save(scene.original.clone());
    assert!(scene.app.plugin_file_hook_modal());
    scene.settle();
    assert!(scene.app.error.is_none(), "{:?}", scene.app.error);
    assert_eq!(std::fs::read(&scene.original).unwrap(), disk);
    assert!(scene.app.editor.is_dirty() && scene.app.editor.can_undo());
    assert_eq!(scene.app.editor.document().as_json(), &expected);
    scene.decide(Decision::Continue);
    scene.app.save(scene.original.clone());
    scene.settle();
    assert!(scene.app.error.is_none(), "{:?}", scene.app.error);
    assert!(!scene.app.editor.is_dirty() && scene.app.editor.can_undo());
    assert_eq!(
        phistudio_core::load_chart(&scene.original)
            .unwrap()
            .as_json()["futureData"],
        expected["futureData"]
    );
    assert_eq!(
        scene
            .control
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.action == FileAction::Save)
            .count(),
        2
    );
    scene.end();
}

#[test]
fn cancelled_discard_and_open_keeps_the_current_document_undo_and_recovery() {
    let mut scene = Scene::new(Decision::Cancel(FileAction::Open), None);
    let target = scene.root.path().join("next.json");
    phistudio_core::save_chart(&ChartDocument::empty("Next", 140.).unwrap(), &target).unwrap();
    scene.dirty();
    scene.files.checkpoint(&scene.app, false).unwrap();
    assert!(
        phistudio_core::load_recovery(scene.root.path())
            .unwrap()
            .is_some()
    );
    let expected = scene.app.editor.document().as_json().clone();
    scene.app.open(target.clone());
    scene.app.files.request = Some(FileRequest::DiscardAndOpen(target.clone()));
    scene.settle();
    assert_eq!(scene.app.editor.document().as_json(), &expected);
    assert!(scene.app.editor.is_dirty() && scene.app.editor.can_undo());
    assert_eq!(scene.app.files.pending_open.as_ref(), Some(&target));
    assert!(
        phistudio_core::load_recovery(scene.root.path())
            .unwrap()
            .is_some()
    );
    scene.decide(Decision::Continue);
    scene.app.files.request = Some(FileRequest::DiscardAndOpen(target));
    scene.settle();
    assert!(scene.app.error.is_none(), "{:?}", scene.app.error);
    assert_eq!(scene.app.editor.document().name(), "Next");
    assert!(!scene.app.editor.is_dirty() && !scene.app.editor.can_undo());
    assert_eq!(scene.control.seen.lock().unwrap().len(), 2);
    scene.end();
}

#[test]
fn back_document_changes_and_unload_revoke_a_waiting_save_before_any_write() {
    for scenario in ["back", "revision", "project", "disable"] {
        let (entered, entry) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let mut scene = Scene::new(
            Decision::Continue,
            Some(Arc::new(Gate {
                entered,
                release: Mutex::new(gate),
            })),
        );
        scene.dirty();
        let disk = std::fs::read(&scene.original).unwrap();
        scene.app.save(scene.original.clone());
        entry.recv_timeout(Duration::from_secs(10)).unwrap();
        match scenario {
            "back" => scene.app.back(),
            "revision" => scene.dirty(),
            "project" => scene.app.replace_document(
                ChartDocument::empty("Other project", 150.).unwrap(),
                None,
                None,
            ),
            "disable" => scene
                .app
                .plugin_action(Action::Disable("test.file-ui".into())),
            _ => unreachable!(),
        }
        scene.frame();
        release.send(()).unwrap();
        scene.settle();
        assert!(
            scene.app.error.is_none(),
            "{scenario}: {:?}",
            scene.app.error
        );
        assert_eq!(std::fs::read(&scene.original).unwrap(), disk);
        assert!(!scene.app.close_requested && !scene.app.plugin_file_hook_modal());
        assert_eq!(scene.app.editor.is_dirty(), scenario != "project");
        scene.end();
    }
}

#[test]
fn export_hooks_cancel_or_rename_the_suggestion_without_changing_package_data() {
    let mut scene = Scene::new(Decision::Cancel(FileAction::Export), None);
    scene.dirty();
    let original = scene.app.editor.document().as_json().clone();
    scene.app.files.request = Some(FileRequest::ExportCurrent);
    scene.settle();
    assert!(scene.app.files.export_package.is_none());
    assert!(scene.app.editor.is_dirty());
    scene.decide(Decision::Rename("reviewed-project.pez".into()));
    scene.app.files.request = Some(FileRequest::ExportCurrent);
    scene.settle();
    assert!(scene.app.error.is_none(), "{:?}", scene.app.error);
    assert_eq!(scene.app.files.export_path, "reviewed-project.pez");
    let package = scene.app.files.export_package.as_ref().unwrap();
    let imported = phistudio_core::import_project(
        package.path(),
        &scene.root.path().join("roundtrip"),
        "Review",
        true,
    )
    .unwrap();
    let chart =
        phistudio_core::load_chart(&imported.charts[imported.suggested.unwrap()].path).unwrap();
    assert_eq!(chart.as_json()["futureData"], original["futureData"]);
    assert_eq!(scene.app.editor.document().as_json(), &original);
    assert!(scene.app.editor.is_dirty());
    scene.end();
}

#[test]
fn stale_approved_project_saves_are_rechecked_when_the_file_manager_dispatches_them() {
    let mut scene = Scene::new(Decision::Continue, None);
    scene.app.file = None;
    scene.dirty();
    scene.app.files.request = Some(FileRequest::SaveWorkspace);
    scene.files.update(&mut scene.app);
    let end = Instant::now() + Duration::from_secs(10);
    while scene.app.plugins.file_hooks.pending.is_some() {
        scene.app.poll_plugins();
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(matches!(
        scene.app.files.request,
        Some(FileRequest::AfterHook { .. })
    ));
    scene.app.back();
    assert!(scene.app.files.request.is_none());
    assert!(!scene.app.close_requested);
    scene.app.files.request = Some(FileRequest::SaveWorkspace);
    scene.files.update(&mut scene.app);
    while scene.app.plugins.file_hooks.pending.is_some() {
        scene.app.poll_plugins();
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    scene.dirty();
    scene.settle();
    assert!(scene.app.file.is_none() && scene.app.editor.is_dirty());
    scene.app.files.request = Some(FileRequest::SaveWorkspace);
    scene.settle();
    assert!(scene.app.error.is_none(), "{:?}", scene.app.error);
    assert!(
        scene
            .app
            .file
            .as_ref()
            .unwrap()
            .starts_with(scene.root.path().join("projects"))
    );
    assert!(!scene.app.editor.is_dirty());
    scene.end();
}

#[test]
fn cancelled_imports_release_only_owned_picker_sources_before_any_archive_decoding() {
    for owned in [false, true] {
        let mut scene = Scene::new(Decision::Cancel(FileAction::Open), None);
        let source = scene.root.path().join("picked-source");
        std::fs::write(&source, b"Do not decode or copy this file").unwrap();
        scene.app.files.request = Some(FileRequest::ImportSource(MediaFile::new(
            source.clone(),
            "picked.pez".into(),
            owned,
        )));
        scene.files.update(&mut scene.app);
        assert!(source.exists());
        scene.settle();
        assert!(scene.app.error.is_none(), "{:?}", scene.app.error);
        assert_eq!(source.exists(), !owned);
        assert_eq!(scene.app.editor.document().name(), "Original");
        scene.end();
    }
}

#[test]
fn malformed_replacements_and_reentrant_edits_fail_without_mutating_the_editor_or_file() {
    for decision in [
        Decision::Rename("../outside.json".into()),
        Decision::Forge,
        Decision::Edit,
    ] {
        let mut scene = Scene::new(decision, None);
        scene.dirty();
        let expected = scene.app.editor.document().as_json().clone();
        let disk = std::fs::read(&scene.original).unwrap();
        scene.app.save(scene.original.clone());
        scene.settle();
        assert!(scene.app.error.is_some());
        assert_eq!(scene.app.editor.document().as_json(), &expected);
        assert_eq!(std::fs::read(&scene.original).unwrap(), disk);
        assert!(scene.app.editor.is_dirty());
        assert!(matches!(
            scene.app.plugins.registry.plugins[0].status,
            Status::Failed(_)
        ));
        scene.end();
    }
}
