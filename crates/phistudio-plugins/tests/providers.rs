use phistudio_plugins::*;
use phistudio_sdk::*;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

struct Gate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
struct Copier {
    files: Option<ProviderFiles>,
    next: Option<u64>,
    gate: Option<Arc<Gate>>,
    bad_file: bool,
    edit: bool,
}
impl Copier {
    fn request(&self, method: &str, arguments: Value) -> Response {
        Response {
            effects: vec![Effect::Call {
                call: ServiceCall {
                    request_id: method.into(),
                    service: ServiceRef {
                        provider: "host".into(),
                        id: "files".into(),
                        version: "^1".parse().unwrap(),
                    },
                    method: method.into(),
                    arguments,
                },
            }],
            ..Default::default()
        }
    }
    fn read(&self, offset: u64) -> Response {
        let files = self.files.as_ref().unwrap();
        self.request(
            "read",
            json!(FileRead {
                operation: files.operation,
                file: if self.bad_file { 999 } else { files.input.id },
                offset,
                limit: MAX_FILE_CHUNK_BYTES
            }),
        )
    }
}
impl Plugin for Copier {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![
                    Contribution::Format {
                        id: "copy".into(),
                        title: "Copy format".into(),
                        extensions: vec!["copy".into()],
                        bridge: FormatBridge::Chart,
                        decode: true,
                        encode: true,
                    },
                    Contribution::Resource {
                        id: "copy".into(),
                        extensions: vec!["raw".into()],
                        title: "Copy resource".into(),
                        resource: ResourceKind::Data,
                    },
                ],
                ..Default::default()
            },
            Request::Format { files, context, .. } | Request::Resource { files, context, .. } => {
                assert!(context.chart.is_none());
                assert!(context.selection.is_null());
                self.files = Some(files.clone());
                if let Some(gate) = &self.gate {
                    gate.entered.send(()).unwrap();
                    gate.release
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(10))
                        .unwrap();
                }
                if self.edit {
                    Response {
                        effects: vec![Effect::Edit {
                            transaction: Transaction {
                                version: context.version,
                                label: "Forbidden provider edit".into(),
                                operations: vec![PatchOperation::Replace {
                                    path: "/META/name".into(),
                                    value: json!("Forbidden"),
                                }],
                            },
                        }],
                        ..Default::default()
                    }
                } else {
                    self.read(0)
                }
            }
            Request::ServiceResult { reply, .. } => {
                let files = self.files.as_ref().unwrap();
                let value = match &reply.outcome {
                    ServiceOutcome::Success { value } => value,
                    ServiceOutcome::Error { message, .. } => {
                        return Ok(Response {
                            effects: vec![Effect::Fail {
                                operation: files.operation,
                                message: message.clone(),
                            }],
                            ..Default::default()
                        });
                    }
                };
                if reply.request_id == "read" {
                    let chunk: FileChunk = serde_json::from_value(value.clone()).unwrap();
                    self.next = chunk.next_offset;
                    self.request(
                        "write",
                        json!(FileWrite {
                            operation: files.operation,
                            file: files.output,
                            offset: None,
                            data: chunk.data
                        }),
                    )
                } else if let Some(offset) = self.next {
                    self.read(offset)
                } else {
                    Response {
                        effects: vec![Effect::Complete {
                            operation: files.operation,
                            file: files.output,
                            name: "output.copy".into(),
                        }],
                        ..Default::default()
                    }
                }
            }
            _ => Response::default(),
        })
    }
}
fn spawn(
    root: Option<std::path::PathBuf>,
    gate: Option<Arc<Gate>>,
    bad_file: bool,
    edit: bool,
) -> Runtime {
    let runtime = Runtime::spawn(root, vec![copier(gate, bad_file, edit)]).unwrap();
    wait(&runtime, 0).unwrap();
    runtime
}
fn copier(gate: Option<Arc<Gate>>, bad_file: bool, edit: bool) -> Builtin {
    let manifest:Manifest = serde_json::from_value(json!({"api":1,"id":"test.provider","name":"Provider","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["formats","resources","services","document_read","document_edit"]})).unwrap();
    Builtin {
        manifest,
        enabled: true,
        create: Arc::new(move || {
            Box::new(Copier {
                files: None,
                next: None,
                gate: gate.clone(),
                bad_file,
                edit,
            })
        }),
    }
}
fn wait(runtime: &Runtime, id: u64) -> Result<Output, String> {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(c) = runtime.poll()? {
            assert_eq!(c.id, id);
            return c.result;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn context() -> EditorContext {
    EditorContext {
        version: DocumentVersion {
            session: 9,
            revision: 0,
        },
        chart: Some(json!({"not":"a provider snapshot"})),
        selection: json!([1, 2, 3]),
        ..Default::default()
    }
}
fn operation(runtime: &mut Runtime, resource: bool) -> Operation {
    let origin = runtime.registry().unwrap().plugins[0]
        .origin
        .clone()
        .unwrap();
    runtime
        .begin_provider(
            &origin,
            if resource {
                Provider::Resource {
                    id: "copy".into(),
                    resource: ResourceKind::Data,
                }
            } else {
                Provider::Format {
                    id: "copy".into(),
                    operation: FormatOperation::Decode,
                }
            },
            context().version,
        )
        .unwrap()
}
fn start(runtime: &mut Runtime, operation: &Operation, path: &std::path::Path) -> u64 {
    runtime
        .submit(Action::Provider {
            operation: operation.clone(),
            source: Box::new(path.to_owned()),
            name: "source.copy".into(),
            context: context(),
        })
        .unwrap()
}

#[test]
fn binary_format_and_resource_providers_stream_beyond_one_message_and_own_their_artifacts() {
    for resource in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let bytes: Vec<u8> = (0..(MAX_MESSAGE_BYTES + 1234))
            .map(|i| (i % 251) as u8)
            .collect();
        std::fs::write(&source, &bytes).unwrap();
        let mut runtime = spawn(Some(root.path().into()), None, false, false);
        let scope = operation(&mut runtime, resource);
        let id = start(&mut runtime, &scope, &source);
        let mut output = wait(&runtime, id).unwrap();
        assert!(output.calls.is_empty());
        assert!(runtime.is_output_current(&output));
        assert!(!scope.is_running());
        let artifact = output.artifact.take().unwrap();
        let path = artifact.path().to_owned();
        assert_eq!(artifact.name(), "output.copy");
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let destination = root.path().join("published.copy");
        std::fs::write(&destination, b"Previous file").unwrap();
        artifact.write_to(&destination).unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), bytes);
        assert!(
            runtime
                .submit(Action::Provider {
                    operation: scope.clone(),
                    source: Box::new(source.clone()),
                    name: "source.copy".into(),
                    context: context()
                })
                .is_err()
        );
        let stop = runtime
            .submit(Action::Disable("test.provider".into()))
            .unwrap();
        assert!(!artifact.operation().is_current());
        assert!(!runtime.is_output_current(&output));
        std::fs::write(&destination, b"Keep this file").unwrap();
        assert!(artifact.write_to(&destination).is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), b"Keep this file");
        wait(&runtime, stop).unwrap();
        drop(artifact);
        assert!(!path.exists());
        runtime.shutdown().unwrap();
    }
}

#[path = "providers/continuations.rs"]
mod continuations;
#[path = "providers/files.rs"]
mod files;
#[path = "providers/packages.rs"]
mod packages;

#[test]
fn source_snapshots_are_immutable_and_cancelled_work_never_publishes_an_artifact() {
    for cancel in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        std::fs::write(&source, b"Original source").unwrap();
        let (entered, entry) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let mut runtime = spawn(
            Some(root.path().into()),
            Some(Arc::new(Gate {
                entered,
                release: Mutex::new(gate),
            })),
            false,
            false,
        );
        let scope = operation(&mut runtime, false);
        let id = start(&mut runtime, &scope, &source);
        entry.recv_timeout(Duration::from_secs(10)).unwrap();
        std::fs::write(&source, b"Changed outside the editor").unwrap();
        if cancel {
            runtime.cancel_provider(&scope).unwrap();
        }
        release.send(()).unwrap();
        let output = wait(&runtime, id).unwrap();
        if cancel {
            assert!(output.artifact.is_none());
            assert!(!scope.is_current());
            // cancel_provider submitted a separate cleanup request.
            let end = Instant::now() + Duration::from_secs(10);
            loop {
                if runtime.poll().unwrap().is_some() {
                    break;
                }
                assert!(Instant::now() < end);
                std::thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(
                std::fs::read_dir(root.path().join("plugins-work"))
                    .unwrap()
                    .count(),
                0
            );
        } else {
            assert_eq!(
                std::fs::read(output.artifact.unwrap().path()).unwrap(),
                b"Original source"
            );
        }
        runtime.shutdown().unwrap();
    }
}

#[test]
fn invalid_file_handles_fail_the_operation_without_failing_the_provider() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    std::fs::write(&source, b"Source").unwrap();
    let mut runtime = spawn(Some(root.path().into()), None, true, false);
    let scope = operation(&mut runtime, false);
    let id = start(&mut runtime, &scope, &source);
    let output = wait(&runtime, id).unwrap();
    assert!(output.artifact.is_none());
    assert!(output.cancelled.unwrap().contains("does not belong"));
    assert_eq!(
        runtime.registry().unwrap().plugins[0].status,
        Status::Enabled
    );
    assert!(!scope.is_running());
    assert_eq!(
        std::fs::read_dir(root.path().join("plugins-work"))
            .unwrap()
            .count(),
        0
    );
    runtime.shutdown().unwrap();
}

#[test]
fn providers_cannot_apply_chart_edits_and_failures_clear_their_private_files() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    std::fs::write(&source, b"Source").unwrap();
    let mut runtime = spawn(Some(root.path().into()), None, false, true);
    let scope = operation(&mut runtime, false);
    let id = start(&mut runtime, &scope, &source);
    assert!(wait(&runtime, id).is_err());
    assert!(!scope.is_current());
    assert_eq!(
        std::fs::read_dir(root.path().join("plugins-work"))
            .unwrap()
            .count(),
        0
    );
    runtime.shutdown().unwrap();
}
