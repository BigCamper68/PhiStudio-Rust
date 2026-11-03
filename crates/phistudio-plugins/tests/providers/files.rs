use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct FileHook(Arc<AtomicUsize>);
impl Plugin for FileHook {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![Contribution::Hook {
                    point: SERVICE_BEFORE_HOOK.into(),
                    priority: 0,
                }],
                ..Default::default()
            },
            Request::Hook { payload, .. } => {
                let call: ServiceIntercept = serde_json::from_value(payload.clone()).unwrap();
                assert_eq!(call.call.service.id, "files");
                self.0.fetch_add(1, Ordering::SeqCst);
                Response {
                    hook: Some(HookDecision::Continue),
                    ..Default::default()
                }
            }
            _ => Response::default(),
        })
    }
}

#[test]
fn file_service_interceptors_remain_participants_until_artifact_publication() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    std::fs::write(&source, b"source").unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    let manifest = serde_json::from_value(json!({"api":1,"id":"test.transparent-hook","name":"File hook","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["hooks"]})).unwrap();
    let mut runtime = Runtime::spawn(
        None,
        vec![
            copier(None, false, false),
            Builtin {
                manifest,
                enabled: true,
                create: Arc::new(move || Box::new(FileHook(calls.clone()))),
            },
        ],
    )
    .unwrap();
    wait(&runtime, 0).unwrap();
    let scope = operation(&mut runtime, false);
    let id = start(&mut runtime, &scope, &source);
    let mut output = wait(&runtime, id).unwrap();
    let artifact = output.artifact.take().unwrap();
    assert!(count.load(Ordering::SeqCst) >= 2);
    assert!(runtime.is_output_current(&output));
    let stop = runtime
        .submit(Action::Disable("test.transparent-hook".into()))
        .unwrap();
    assert!(runtime.is_current(scope.origin()));
    assert!(!scope.is_current());
    assert!(!runtime.is_output_current(&output));
    let destination = root.path().join("destination");
    std::fs::write(&destination, b"Keep destination").unwrap();
    assert!(artifact.write_to(&destination).is_err());
    assert_eq!(std::fs::read(destination).unwrap(), b"Keep destination");
    wait(&runtime, stop).unwrap();
    runtime.shutdown().unwrap();
}

struct FileChecks {
    files: Option<ProviderFiles>,
    steps: Vec<(&'static str, Value, Result<Value, &'static str>)>,
    step: usize,
}
impl FileChecks {
    fn next(&self) -> Response {
        let files = self.files.as_ref().unwrap();
        Response {
            effects: vec![
                if let Some((method, arguments, _)) = self.steps.get(self.step) {
                    Effect::Call {
                        call: ServiceCall {
                            request_id: format!("step{}", self.step),
                            service: ServiceRef {
                                provider: "host".into(),
                                id: "files".into(),
                                version: "^1".parse().unwrap(),
                            },
                            method: (*method).into(),
                            arguments: arguments.clone(),
                        },
                    }
                } else {
                    Effect::Complete {
                        operation: files.operation,
                        file: files.output,
                        name: "checked.copy".into(),
                    }
                },
            ],
            ..Default::default()
        }
    }
}
impl Plugin for FileChecks {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        match request {
            Request::Activate { .. } => Ok(Response {
                contributions: vec![Contribution::Format {
                    id: "copy".into(),
                    title: "Checked format".into(),
                    extensions: vec!["copy".into()],
                    bridge: FormatBridge::Chart,
                    decode: true,
                    encode: false,
                }],
                ..Default::default()
            }),
            Request::Format { files, .. } => {
                let op = files.operation;
                let read = |file, offset, limit| json!({"operation":op,"file":file,"offset":offset,"limit":limit});
                let write = |file, offset, bytes: &[u8]| json!({"operation":op,"file":file,"offset":offset,"data":encode_file_chunk(bytes).unwrap()});
                let resize = |size| json!({"operation":op,"file":2,"size":size});
                self.steps = vec![
                    (
                        "read",
                        read(1, 0, 2),
                        Ok(json!(FileChunk {
                            offset: 0,
                            size: 6,
                            next_offset: Some(2),
                            data: encode_file_chunk(b"so").unwrap()
                        })),
                    ),
                    (
                        "read",
                        read(1, 6, 2),
                        Ok(json!(FileChunk {
                            offset: 6,
                            size: 6,
                            next_offset: None,
                            data: String::new()
                        })),
                    ),
                    ("read", read(1, 7, 1), Err("past the end")),
                    ("read", read(1, 0, 0), Err("Invalid file read")),
                    (
                        "read",
                        read(1, 0, MAX_FILE_CHUNK_BYTES + 1),
                        Err("Invalid file read"),
                    ),
                    ("read", read(999, 0, 1), Err("does not belong")),
                    (
                        "read",
                        json!({"operation":op + 1,"file":1,"offset":0,"limit":1}),
                        Err("operation ID"),
                    ),
                    ("write", write(1, None::<u64>, b"bad"), Err("writable")),
                    ("write", write(2, Some(1), b"bad"), Err("past the end")),
                    (
                        "write",
                        json!({"operation":op,"file":2,"data":"!"}),
                        Err("Invalid"),
                    ),
                    (
                        "write",
                        json!({"operation":op,"file":2,"data":"AAAA".repeat(MAX_FILE_CHUNK_BYTES / 3 + 2)}),
                        Err("limit"),
                    ),
                    ("resize", resize(MAX_PROVIDER_FILE_BYTES + 1), Err("resize")),
                    (
                        "resize",
                        json!({"operation":op,"file":1,"size":0}),
                        Err("resize"),
                    ),
                    ("missing", json!({}), Err("Unknown")),
                    ("write", write(2, None, b"abcdef"), Ok(json!({"size":6}))),
                    ("write", write(2, Some(2), b"ZZ"), Ok(json!({"size":6}))),
                    ("resize", resize(8), Ok(json!({"size":8}))),
                    (
                        "read",
                        read(2, 0, 8),
                        Ok(json!(FileChunk {
                            offset: 0,
                            size: 8,
                            next_offset: None,
                            data: encode_file_chunk(b"abZZef\0\0").unwrap()
                        })),
                    ),
                    ("resize", resize(3), Ok(json!({"size":3}))),
                    ("write", write(2, None, b"!"), Ok(json!({"size":4}))),
                ];
                self.files = Some(files.clone());
                Ok(self.next())
            }
            Request::ServiceResult { reply, .. } => {
                let (_, _, expected) = &self.steps[self.step];
                match (&reply.outcome, expected) {
                    (ServiceOutcome::Success { value }, Ok(expected)) => {
                        assert_eq!(value, expected)
                    }
                    (ServiceOutcome::Error { code, message }, Err(part)) => {
                        assert_eq!(code, "file_error");
                        assert!(message.contains(part), "Step {}: {message}", self.step);
                    }
                    _ => panic!(
                        "Unexpected file result at {}: {:?}",
                        self.step, reply.outcome
                    ),
                }
                self.step += 1;
                Ok(self.next())
            }
            _ => Ok(Response::default()),
        }
    }
}

#[test]
fn file_services_enforce_operation_bounds_and_support_random_access_and_truncation() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    std::fs::write(&source, b"source").unwrap();
    let manifest = serde_json::from_value(json!({"api":1,"id":"test.files","name":"Files","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["formats","services"]})).unwrap();
    let mut runtime = Runtime::spawn(
        Some(root.path().into()),
        vec![Builtin {
            manifest,
            enabled: true,
            create: Arc::new(|| {
                Box::new(FileChecks {
                    files: None,
                    steps: vec![],
                    step: 0,
                })
            }),
        }],
    )
    .unwrap();
    wait(&runtime, 0).unwrap();
    let scope = operation(&mut runtime, false);
    let id = start(&mut runtime, &scope, &source);
    let output = wait(&runtime, id).unwrap();
    let artifact = output.artifact.unwrap();
    assert_eq!(std::fs::read(artifact.path()).unwrap(), b"abZ!");
    assert_eq!(std::fs::read(source).unwrap(), b"source");
    assert_eq!(
        runtime.registry().unwrap().plugins[0].status,
        Status::Enabled
    );
    runtime.shutdown().unwrap();
}

#[test]
fn provider_inputs_reject_directories_and_oversized_files_and_clear_private_work() {
    let root = tempfile::tempdir().unwrap();
    let large = root.path().join("large");
    std::fs::File::create(&large)
        .unwrap()
        .set_len(MAX_PROVIDER_FILE_BYTES + 1)
        .unwrap();
    for source in [root.path(), large.as_path()] {
        let mut runtime = spawn(Some(root.path().into()), None, false, false);
        let scope = operation(&mut runtime, false);
        let id = start(&mut runtime, &scope, source);
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
}

#[test]
fn operations_cannot_cross_runtimes_or_change_the_registered_resource_kind() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    std::fs::write(&source, b"source").unwrap();
    let mut first = spawn(None, None, false, false);
    let mut second = spawn(None, None, false, false);
    let scope = operation(&mut first, false);
    assert!(second.cancel_provider(&scope).is_err());
    assert!(
        second
            .submit(Action::CancelProvider(scope.clone()))
            .is_err()
    );
    assert!(scope.is_current());
    assert!(
        second
            .submit(Action::Provider {
                operation: scope.clone(),
                source: Box::new(source.clone()),
                name: "source.copy".into(),
                context: context()
            })
            .is_err()
    );
    let origin = scope.origin().clone();
    assert!(
        first
            .begin_provider(
                &origin,
                Provider::Resource {
                    id: "copy".into(),
                    resource: ResourceKind::Shader
                },
                context().version
            )
            .is_err()
    );
    let id = start(&mut first, &scope, &source);
    assert!(wait(&first, id).unwrap().artifact.is_some());
    first.shutdown().unwrap();
    second.shutdown().unwrap();
}
