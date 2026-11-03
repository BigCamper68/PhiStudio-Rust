use phistudio_plugins::*;
use phistudio_sdk::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

struct Gate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
struct Peer {
    gate: Option<Arc<Gate>>,
    storage: Value,
    count: Value,
}
impl Plugin for Peer {
    fn call(&mut self, r: &Request) -> Result<Response, String> {
        Ok(match r {
            Request::Activate {
                settings, storage, ..
            } => {
                self.count = settings["count"].clone();
                self.storage = storage.clone();
                Response {
                    contributions: ["rename", "block", "inspect", "store"]
                        .into_iter()
                        .map(|id| Contribution::Command {
                            id: id.into(),
                            title: id.into(),
                            shortcut: None,
                            snapshot: false,
                        })
                        .collect(),
                    ..Default::default()
                }
            }
            Request::Command { id, context, .. } if id == "rename" || id == "block" => {
                if id == "block"
                    && let Some(gate) = &self.gate
                {
                    gate.entered.send(()).map_err(|e| e.to_string())?;
                    gate.release
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .map_err(|e| e.to_string())?;
                }
                Response {
                    effects: vec![Effect::Edit {
                        transaction: Transaction {
                            version: context.version,
                            label: "Worker rename".into(),
                            operations: vec![PatchOperation::Replace {
                                path: "/META/name".into(),
                                value: json!("Worker result"),
                            }],
                        },
                    }],
                    ..Default::default()
                }
            }
            Request::Command { id, .. } if id == "store" => Response {
                effects: vec![Effect::Store {
                    value: json!({"kept":42}),
                }],
                ..Default::default()
            },
            Request::Command { id, .. } if id == "inspect" => Response {
                result: json!({"count":self.count,"storage":self.storage}),
                ..Default::default()
            },
            Request::SettingsChanged { value, .. } => {
                self.count = value.clone();
                Response::default()
            }
            _ => Response::default(),
        })
    }
}
fn builtin(gate: Option<Arc<Gate>>) -> Builtin {
    let manifest:Manifest = serde_json::from_value(json!({"api":1,"id":"test.worker","name":"Worker test","version":"1.0.0","backend":{"kind":"builtin"},
        "capabilities":["document_edit","storage"],"settings":{"count":{"kind":"number","title":"Count","default":1,"min":0,"max":10}}})).unwrap();
    Builtin {
        manifest,
        enabled: true,
        create: Arc::new(move || {
            Box::new(Peer {
                gate: gate.clone(),
                storage: Value::Null,
                count: Value::Null,
            })
        }),
    }
}
fn wait(runtime: &Runtime, id: u64) -> Result<Output, String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(completion) = runtime.poll()? {
            assert_eq!(completion.id, id);
            return completion.result;
        }
        assert!(
            Instant::now() < deadline,
            "Worker did not finish request {id}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn command(id: &str) -> Action {
    Action::Command {
        plugin: "test.worker".into(),
        id: id.into(),
        arguments: Value::Null,
        context: EditorContext::default(),
    }
}

#[test]
fn disabling_revokes_queued_and_in_progress_edits_before_the_worker_can_stop() {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let gate = Arc::new(Gate {
        entered: entered_tx,
        release: Mutex::new(release_rx),
    });
    let mut runtime = Runtime::spawn(None, vec![builtin(Some(gate))]).unwrap();
    wait(&runtime, 0).unwrap();
    let first = runtime.submit(command("rename")).unwrap();
    let queued = wait(&runtime, first).unwrap().effects.remove(0);
    let blocked = runtime.submit(command("block")).unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let disabled = runtime
        .submit(Action::Disable("test.worker".into()))
        .unwrap();
    assert!(!runtime.is_current(&queued.origin));
    let mut editor = phistudio_core::Editor::new(
        phistudio_core::ChartDocument::empty("Original", 120.).unwrap(),
    );
    let Effect::Edit { transaction } = queued.effect else {
        panic!()
    };
    assert!(
        runtime
            .apply_transaction(
                &queued.origin,
                &queued.required,
                0,
                &mut editor,
                transaction
            )
            .is_err()
    );
    release_tx.send(()).unwrap();
    assert!(wait(&runtime, blocked).unwrap().effects.is_empty());
    wait(&runtime, disabled).unwrap();
    assert_eq!(editor.document().name(), "Original");
    assert_eq!(
        runtime.registry().unwrap().plugins[0].status,
        Status::Disabled
    );
    runtime.shutdown().unwrap();
}

#[test]
fn settings_storage_and_enabled_defaults_survive_reload_and_restart() {
    let root = tempfile::tempdir().unwrap();
    let mut runtime = Runtime::spawn(Some(root.path().into()), vec![builtin(None)]).unwrap();
    wait(&runtime, 0).unwrap();
    let id = runtime
        .submit(Action::Setting {
            plugin: "test.worker".into(),
            key: "count".into(),
            value: json!(7),
        })
        .unwrap();
    wait(&runtime, id).unwrap();
    let id = runtime.submit(command("store")).unwrap();
    wait(&runtime, id).unwrap();
    let origin = runtime.registry().unwrap().plugins[0]
        .origin
        .clone()
        .unwrap();
    let id = runtime.submit(Action::Reload).unwrap();
    wait(&runtime, id).unwrap();
    assert!(!runtime.is_current(&origin));
    assert_eq!(
        runtime.registry().unwrap().plugins[0].status,
        Status::Enabled
    );
    let id = runtime.submit(command("inspect")).unwrap();
    assert_eq!(
        wait(&runtime, id).unwrap().result,
        json!({"count":7,"storage":{"kept":42}})
    );
    runtime.shutdown().unwrap();
    let mut restarted = Runtime::spawn(Some(root.path().into()), vec![builtin(None)]).unwrap();
    wait(&restarted, 0).unwrap();
    let id = restarted.submit(command("inspect")).unwrap();
    assert_eq!(
        wait(&restarted, id).unwrap().result,
        json!({"count":7,"storage":{"kept":42}})
    );
    let id = restarted
        .submit(Action::Disable("test.worker".into()))
        .unwrap();
    wait(&restarted, id).unwrap();
    restarted.shutdown().unwrap();
    let mut disabled = Runtime::spawn(Some(root.path().into()), vec![builtin(None)]).unwrap();
    wait(&disabled, 0).unwrap();
    assert_eq!(
        disabled.registry().unwrap().plugins[0].status,
        Status::Disabled
    );
    disabled.shutdown().unwrap();
}

#[test]
fn a_failed_reload_does_not_revalidate_old_effects_or_overwrite_invalid_settings() {
    let root = tempfile::tempdir().unwrap();
    let mut runtime = Runtime::spawn(Some(root.path().into()), vec![builtin(None)]).unwrap();
    wait(&runtime, 0).unwrap();
    let id = runtime.submit(command("rename")).unwrap();
    let effect = wait(&runtime, id).unwrap().effects.remove(0);
    std::fs::write(root.path().join("plugins-state.json"), b"invalid settings").unwrap();
    let id = runtime.submit(Action::Reload).unwrap();
    assert!(wait(&runtime, id).is_err());
    assert!(!runtime.is_current(&effect.origin));
    let id = runtime
        .submit(Action::Disable("test.worker".into()))
        .unwrap();
    assert!(wait(&runtime, id).is_err());
    assert_eq!(
        std::fs::read(root.path().join("plugins-state.json")).unwrap(),
        b"invalid settings"
    );
    runtime.shutdown().unwrap();
}

#[test]
fn shutdown_invalidates_the_published_registry() {
    let mut runtime = Runtime::spawn(None, vec![builtin(None)]).unwrap();
    wait(&runtime, 0).unwrap();
    let origin = runtime.registry().unwrap().plugins[0]
        .origin
        .clone()
        .unwrap();
    runtime.shutdown().unwrap();
    assert!(!runtime.is_current(&origin));
    assert!(
        runtime.registry().unwrap().plugins[0]
            .contributions
            .is_empty()
    );
}

#[test]
fn queued_installation_owns_its_temporary_picker_snapshot() {
    struct Source(std::path::PathBuf);
    impl PackageSource for Source {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for Source {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    use std::io::Write;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("picker.zip");
    let mut manifest = serde_json::to_value(builtin(None).manifest).unwrap();
    manifest["id"] = json!("test.package");
    manifest["backend"] = json!({"kind":"wasm","entry":"plugin.wasm"});
    let bytes = wat::parse_str(
        r#"(module
        (memory (export "memory") 1)
        (data (i32.const 1024) "{}")
        (func (export "phistudio_api_version") (result i32) i32.const 1)
        (func (export "phistudio_alloc") (param i32) (result i32) i32.const 8192)
        (func (export "phistudio_free") (param i32 i32))
        (func (export "phistudio_call") (param i32 i32) (result i64) i64.const 8589935616))"#,
    )
    .unwrap();
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for (name, bytes) in [
        ("manifest.json", serde_json::to_vec(&manifest).unwrap()),
        ("plugin.wasm", bytes),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut runtime = Runtime::spawn(
        Some(root.path().into()),
        vec![builtin(Some(Arc::new(Gate {
            entered: entered_tx,
            release: Mutex::new(release_rx),
        })))],
    )
    .unwrap();
    wait(&runtime, 0).unwrap();
    let command = runtime.submit(command("block")).unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let install = runtime
        .submit(Action::Install(Box::new(Source(path.clone()))))
        .unwrap();
    assert!(path.exists());
    release_tx.send(()).unwrap();
    wait(&runtime, command).unwrap();
    wait(&runtime, install).unwrap();
    assert!(!path.exists());
    assert!(
        root.path()
            .join("plugins/test.package/manifest.json")
            .exists()
    );
    let setting = runtime
        .submit(Action::Setting {
            plugin: "test.package".into(),
            key: "count".into(),
            value: json!(7),
        })
        .unwrap();
    wait(&runtime, setting).unwrap();
    let remove = runtime
        .submit(Action::Remove("test.package".into()))
        .unwrap();
    wait(&runtime, remove).unwrap();
    let retired = std::fs::read_dir(root.path().join("plugins/.removed"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("test.package");
    let reinstall = runtime.submit(Action::Install(Box::new(retired))).unwrap();
    wait(&runtime, reinstall).unwrap();
    assert_eq!(
        runtime
            .registry()
            .unwrap()
            .plugins
            .iter()
            .find(|p| p.manifest.id == "test.package")
            .unwrap()
            .settings["count"],
        json!(7)
    );
    runtime.shutdown().unwrap();
}
