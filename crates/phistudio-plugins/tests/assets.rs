use phistudio_plugins::*;
use phistudio_sdk::*;
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

type Replies = Arc<Mutex<Vec<ServiceOutcome>>>;
struct Gate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
struct Reader {
    replies: Replies,
    gate: Option<Arc<Gate>>,
}
impl Plugin for Reader {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        match request {
            Request::Activate { .. } => Ok(Response {
                contributions: vec![
                    Contribution::Command {
                        id: "request".into(),
                        title: "Read assets".into(),
                        shortcut: None,
                        snapshot: false,
                    },
                    Contribution::Service {
                        id: "read".into(),
                        version: Version::new(1, 0, 0),
                    },
                ],
                ..Default::default()
            }),
            Request::Command { arguments, .. } | Request::Service { arguments, .. } => {
                if arguments["blocked"] == true
                    && let Some(gate) = &self.gate
                {
                    gate.entered.send(()).unwrap();
                    gate.release
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                }
                Ok(Response {
                    effects: vec![Effect::Call {
                        call: ServiceCall {
                            request_id: "asset-request".into(),
                            service: ServiceRef {
                                provider: "host".into(),
                                id: "assets".into(),
                                version: "^1".parse().unwrap(),
                            },
                            method: arguments["method"].as_str().unwrap().into(),
                            arguments: arguments["args"].clone(),
                        },
                    }],
                    ..Default::default()
                })
            }
            Request::ServiceResult { reply, .. } => {
                self.replies.lock().unwrap().push(reply.outcome.clone());
                Ok(Response::default())
            }
            _ => Ok(Response::default()),
        }
    }
}
fn builtin(id: &str, project: bool, replies: &Replies, gate: Option<Arc<Gate>>) -> Builtin {
    let mut capabilities = vec![Capability::Services];
    if project {
        capabilities.push(Capability::ProjectRead);
    }
    let manifest = serde_json::from_value(json!({"api":1,"id":id,"name":"Asset reader","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":capabilities})).unwrap();
    let replies = replies.clone();
    Builtin {
        manifest,
        enabled: true,
        create: Arc::new(move || {
            Box::new(Reader {
                replies: replies.clone(),
                gate: gate.clone(),
            })
        }),
    }
}
fn wait(runtime: &Runtime, id: u64) -> Output {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(completion) = runtime.poll().unwrap() {
            assert_eq!(completion.id, id);
            return completion.result.unwrap();
        }
        assert!(Instant::now() < deadline, "Asset worker did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn command(
    plugin: &str,
    version: DocumentVersion,
    method: &str,
    args: Value,
    blocked: bool,
) -> Action {
    Action::Command {
        plugin: plugin.into(),
        id: "request".into(),
        arguments: json!({"method":method,"args":args,"blocked":blocked}),
        context: EditorContext {
            version,
            ..Default::default()
        },
    }
}
fn call(
    runtime: &mut Runtime,
    replies: &Replies,
    plugin: &str,
    version: DocumentVersion,
    method: &str,
    args: Value,
) -> ServiceOutcome {
    let id = runtime
        .submit(command(plugin, version, method, args, false))
        .unwrap();
    let output = wait(runtime, id);
    assert!(
        output.calls.is_empty(),
        "Asset IO must stay off the UI thread"
    );
    assert!(output.effects.is_empty());
    replies
        .lock()
        .unwrap()
        .pop()
        .expect("Missing service reply")
}
fn success(outcome: ServiceOutcome) -> Value {
    match outcome {
        ServiceOutcome::Success { value } => value,
        error => panic!("{error:?}"),
    }
}
fn failure(outcome: ServiceOutcome) {
    assert!(
        matches!(outcome, ServiceOutcome::Error { .. }),
        "{outcome:?}"
    );
}
fn png(path: &Path) {
    image::save_buffer_with_format(
        path,
        &[255, 0, 0, 255, 0, 255, 0, 128],
        2,
        1,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .unwrap();
}
fn version() -> DocumentVersion {
    DocumentVersion {
        session: 7,
        revision: 3,
    }
}

#[test]
fn project_snapshots_stream_bytes_decode_images_and_revoke_by_owner_and_asset_revision() {
    let root = tempfile::tempdir().unwrap();
    png(&root.path().join("cover.png"));
    let original = std::fs::read(root.path().join("cover.png")).unwrap();
    let replies = Replies::default();
    let mut runtime = Runtime::spawn(
        None,
        vec![
            builtin("test.reader", true, &replies, None),
            builtin("test.other", true, &replies, None),
        ],
    )
    .unwrap();
    wait(&runtime, 0);
    let mut version = version();
    runtime.set_project(version, 4, Some(root.path()));
    let file = success(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "open",
        json!({"source":"project","path":"cover.png"}),
    ))["id"]
        .as_u64()
        .unwrap();
    let origin = runtime
        .registry()
        .unwrap()
        .plugins
        .iter()
        .find(|p| p.manifest.id == "test.reader")
        .unwrap()
        .origin
        .clone()
        .unwrap();
    let image = success(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "image",
        json!({"file":file}),
    ));
    assert_eq!(image, json!({"file":file,"width":2,"height":1}));
    assert_eq!(
        runtime.image(&origin, file).unwrap().rgba(),
        &[255, 0, 0, 255, 0, 255, 0, 128]
    );
    std::fs::write(root.path().join("cover.png"), b"replacement").unwrap();
    let bytes: FileChunk = serde_json::from_value(success(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "read",
        json!({"file":file,"offset":0,"limit":MAX_FILE_CHUNK_BYTES}),
    )))
    .unwrap();
    assert_eq!(decode_file_chunk(&bytes.data).unwrap(), original);
    failure(call(
        &mut runtime,
        &replies,
        "test.other",
        version,
        "read",
        json!({"file":file,"offset":0,"limit":1}),
    ));
    let other = runtime
        .registry()
        .unwrap()
        .plugins
        .iter()
        .find(|p| p.manifest.id == "test.other")
        .unwrap()
        .origin
        .clone()
        .unwrap();
    assert!(runtime.image(&other, file).is_none());
    let previous = version;
    version.revision += 1;
    runtime.set_project(version, 4, Some(root.path()));
    assert!(
        runtime.image(&origin, file).is_some(),
        "Chart edits keep resource snapshots"
    );
    failure(call(
        &mut runtime,
        &replies,
        "test.reader",
        previous,
        "read",
        json!({"file":file,"offset":0,"limit":1}),
    ));
    success(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "read",
        json!({"file":file,"offset":0,"limit":1}),
    ));
    runtime.set_project(version, 5, Some(root.path()));
    assert!(
        runtime.image(&origin, file).is_none(),
        "Native resource edits revoke immediately"
    );
    failure(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "read",
        json!({"file":file,"offset":0,"limit":1}),
    ));
    png(&root.path().join("cover.png"));
    let second = success(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "open",
        json!({"source":"project","path":"cover.png"}),
    ))["id"]
        .as_u64()
        .unwrap();
    assert!(second > file);
    success(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "image",
        json!({"file":second}),
    ));
    runtime.set_project(
        DocumentVersion {
            session: 8,
            revision: 0,
        },
        0,
        Some(root.path()),
    );
    assert!(runtime.image(&origin, second).is_none());
    assert!(
        runtime
            .registry()
            .unwrap()
            .plugins
            .iter()
            .all(|p| p.status == Status::Enabled)
    );
    runtime.shutdown().unwrap();
}

#[test]
fn listings_permissions_bounds_paths_links_and_handle_lifetimes_are_enforced() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("nested")).unwrap();
    for index in 0..300 {
        std::fs::write(
            root.path().join("nested").join(format!("{index:03}.bin")),
            [index as u8],
        )
        .unwrap();
    }
    std::fs::File::create(root.path().join("huge.bin"))
        .unwrap()
        .set_len(MAX_ASSET_BYTES + 1)
        .unwrap();
    std::fs::write(root.path().join("invalid.png"), b"invalid").unwrap();
    let mut dimensions = include_bytes!("../../../examples/plugins/asset-view/mark.bmp").to_vec();
    dimensions[18..22].copy_from_slice(&8192i32.to_le_bytes());
    dimensions[22..26].copy_from_slice(&8192i32.to_le_bytes());
    std::fs::write(root.path().join("oversized.bmp"), dimensions).unwrap();
    let replies = Replies::default();
    let mut runtime = Runtime::spawn(
        None,
        vec![
            builtin("test.reader", true, &replies, None),
            builtin("test.denied", false, &replies, None),
        ],
    )
    .unwrap();
    wait(&runtime, 0);
    let version = version();
    runtime.set_project(version, 0, Some(root.path()));
    failure(call(
        &mut runtime,
        &replies,
        "test.denied",
        version,
        "list",
        json!({"source":"project","offset":0,"limit":1}),
    ));
    let page = success(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "list",
        json!({"source":"project","directory":"nested","offset":0,"limit":256}),
    ));
    assert_eq!(page["total"], 300);
    assert_eq!(page["next_offset"], 256);
    assert_eq!(page["entries"][0]["path"], "nested/000.bin");
    assert!(serde_json::to_vec(&page).unwrap().len() <= MAX_FILE_CHUNK_BYTES);
    let page = success(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "list",
        json!({"source":"project","directory":"nested","offset":256,"limit":256}),
    ));
    assert_eq!(page["entries"].as_array().unwrap().len(), 44);
    assert_eq!(page["next_offset"], Value::Null);
    for path in [
        "../outside",
        "/outside",
        "nested/../huge.bin",
        "nested\\000.bin",
        "C:/outside",
        "huge.bin",
        "nested",
        "control\n",
    ] {
        failure(call(
            &mut runtime,
            &replies,
            "test.reader",
            version,
            "open",
            json!({"source":"project","path":path}),
        ));
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("nested/000.bin", root.path().join("alias.bin")).unwrap();
        failure(call(
            &mut runtime,
            &replies,
            "test.reader",
            version,
            "open",
            json!({"source":"project","path":"alias.bin"}),
        ));
    }
    let bad = success(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "open",
        json!({"source":"project","path":"invalid.png"}),
    ))["id"]
        .as_u64()
        .unwrap();
    failure(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "image",
        json!({"file":bad}),
    ));
    success(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "close",
        json!({"file":bad}),
    ));
    let oversized = success(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "open",
        json!({"source":"project","path":"oversized.bmp"}),
    ))["id"]
        .as_u64()
        .unwrap();
    failure(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "image",
        json!({"file":oversized}),
    ));
    success(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "close",
        json!({"file":oversized}),
    ));
    let mut handles = vec![];
    for _ in 0..MAX_PLUGIN_ASSET_HANDLES {
        handles.push(
            success(call(
                &mut runtime,
                &replies,
                "test.reader",
                version,
                "open",
                json!({"source":"project","path":"nested/000.bin"}),
            ))["id"]
                .as_u64()
                .unwrap(),
        );
    }
    failure(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "open",
        json!({"source":"project","path":"nested/000.bin"}),
    ));
    for args in [
        json!({"file":handles[0],"offset":0,"limit":0}),
        json!({"file":handles[0],"offset":0,"limit":MAX_FILE_CHUNK_BYTES+1}),
        json!({"file":handles[0],"offset":2,"limit":1}),
    ] {
        failure(call(
            &mut runtime,
            &replies,
            "test.reader",
            version,
            "read",
            args,
        ));
    }
    success(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "close",
        json!({"file":handles[0]}),
    ));
    failure(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "read",
        json!({"file":handles[0],"offset":0,"limit":1}),
    ));
    success(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "open",
        json!({"source":"project","path":"nested/001.bin"}),
    ));
    let id = runtime.submit(Action::Reload).unwrap();
    wait(&runtime, id);
    failure(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "read",
        json!({"file":handles[1],"offset":0,"limit":1}),
    ));
    assert!(
        runtime
            .registry()
            .unwrap()
            .plugins
            .iter()
            .all(|p| p.status == Status::Enabled)
    );
    runtime.shutdown().unwrap();
}

#[test]
fn queued_queries_are_cancelled_by_disable_and_project_replacement() {
    let root = tempfile::tempdir().unwrap();
    png(&root.path().join("cover.png"));
    let (entered, entered_rx) = mpsc::channel();
    let (release_tx, release) = mpsc::channel();
    let gate = Arc::new(Gate {
        entered,
        release: Mutex::new(release),
    });
    let replies = Replies::default();
    let mut runtime = Runtime::spawn(
        None,
        vec![builtin("test.reader", true, &replies, Some(gate))],
    )
    .unwrap();
    wait(&runtime, 0);
    let version = version();
    runtime.set_project(version, 0, Some(root.path()));
    let blocked = runtime
        .submit(command(
            "test.reader",
            version,
            "open",
            json!({"source":"project","path":"cover.png"}),
            true,
        ))
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    runtime.set_project(
        DocumentVersion {
            session: 8,
            revision: 0,
        },
        0,
        Some(root.path()),
    );
    release_tx.send(()).unwrap();
    wait(&runtime, blocked);
    failure(replies.lock().unwrap().pop().unwrap());
    runtime.set_project(version, 0, Some(root.path()));
    let blocked = runtime
        .submit(command(
            "test.reader",
            version,
            "open",
            json!({"source":"project","path":"cover.png"}),
            true,
        ))
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let disabled = runtime
        .submit(Action::Disable("test.reader".into()))
        .unwrap();
    release_tx.send(()).unwrap();
    wait(&runtime, blocked);
    wait(&runtime, disabled);
    assert!(replies.lock().unwrap().is_empty());
    runtime.shutdown().unwrap();
}

struct Consumer;
impl Plugin for Consumer {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![Contribution::Command {
                    id: "delegate".into(),
                    title: "Delegate asset reading".into(),
                    shortcut: None,
                    snapshot: false,
                }],
                ..Default::default()
            },
            Request::Command { .. } => Response {
                effects: vec![Effect::Call {
                    call: ServiceCall {
                        request_id: "delegated".into(),
                        service: ServiceRef {
                            provider: "test.reader".into(),
                            id: "read".into(),
                            version: VersionReq::STAR,
                        },
                        method: "open".into(),
                        arguments: json!({"method":"open","args":{"source":"project","path":"file.bin"}}),
                    },
                }],
                ..Default::default()
            },
            _ => Response::default(),
        })
    }
}

#[test]
fn service_created_snapshots_retain_the_original_consumers_lifetime() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("file.bin"), b"delegated data").unwrap();
    let replies = Replies::default();
    let consumer = Builtin {
        manifest: serde_json::from_value(json!({"api":1,"id":"test.consumer","name":"Consumer","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["services"],"dependencies":{"test.reader":"^1"}})).unwrap(),
        enabled: true, create: Arc::new(|| Box::new(Consumer)),
    };
    let mut runtime = Runtime::spawn(
        None,
        vec![builtin("test.reader", true, &replies, None), consumer],
    )
    .unwrap();
    wait(&runtime, 0);
    let version = version();
    runtime.set_project(version, 0, Some(root.path()));
    let id = runtime
        .submit(Action::Command {
            plugin: "test.consumer".into(),
            id: "delegate".into(),
            arguments: Value::Null,
            context: EditorContext {
                version,
                ..Default::default()
            },
        })
        .unwrap();
    wait(&runtime, id);
    let file = success(replies.lock().unwrap().pop().unwrap())["id"]
        .as_u64()
        .unwrap();
    success(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "read",
        json!({"file":file,"offset":0,"limit":16}),
    ));
    let disabled = runtime
        .submit(Action::Disable("test.consumer".into()))
        .unwrap();
    wait(&runtime, disabled);
    assert_eq!(
        runtime
            .registry()
            .unwrap()
            .plugins
            .iter()
            .find(|p| p.manifest.id == "test.reader")
            .unwrap()
            .status,
        Status::Enabled
    );
    failure(call(
        &mut runtime,
        &replies,
        "test.reader",
        version,
        "read",
        json!({"file":file,"offset":0,"limit":16}),
    ));
    runtime.shutdown().unwrap();
}
