use phistudio_plugins::*;
use phistudio_sdk::*;
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, Instant},
};

fn wait(runtime: &Runtime, id: u64) -> Result<Output, String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(done) = runtime.poll()? {
            assert_eq!(done.id, id);
            return done.result;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn manifest(id: &str) -> Manifest {
    serde_json::from_value(json!({"api":1,"id":id,"name":id,"version":"1.0.0",
        "backend":{"kind":"wasm","entry":"plugin.wasm"},"capabilities":["ui"]}))
    .unwrap()
}
fn package(root: &Path, manifest: &Manifest) -> std::path::PathBuf {
    let path = root.join(&manifest.id);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("manifest.json"),
        serde_json::to_vec(manifest).unwrap(),
    )
    .unwrap();
    let module = wat::parse_str(
        r#"(module
        (memory (export "memory") 1)
        (data (i32.const 1024) "{}")
        (func (export "phistudio_api_version") (result i32) i32.const 1)
        (func (export "phistudio_alloc") (param i32) (result i32) i32.const 8192)
        (func (export "phistudio_free") (param i32 i32))
        (func (export "phistudio_call") (param i32 i32) (result i64) i64.const 8589935616))"#,
    )
    .unwrap();
    std::fs::write(path.join("plugin.wasm"), module).unwrap();
    path
}
fn approve(runtime: &mut Runtime, m: &Manifest) {
    let requested = PermissionRequest::from_manifest(m);
    let id = runtime
        .submit(Action::Permissions {
            plugin: m.id.clone(),
            decision: Some(PermissionDecision {
                requested: requested.clone(),
                granted: requested,
            }),
        })
        .unwrap();
    wait(runtime, id).unwrap();
}

#[test]
fn external_consent_survives_restart_and_expanded_packages_stop_before_activation() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let m = manifest("test.consent");
    let package = package(source.path(), &m);
    let mut runtime = Runtime::spawn(Some(root.path().into()), vec![]).unwrap();
    wait(&runtime, 0).unwrap();
    let id = runtime.submit(Action::Install(Box::new(package))).unwrap();
    wait(&runtime, id).unwrap();
    assert!(runtime.registry().unwrap().plugins[0].granted.is_empty());
    let id = runtime.submit(Action::Enable(m.id.clone())).unwrap();
    assert!(
        wait(&runtime, id)
            .err()
            .unwrap()
            .contains("Review permissions")
    );
    approve(&mut runtime, &m);
    let id = runtime.submit(Action::Enable(m.id.clone())).unwrap();
    wait(&runtime, id).unwrap();
    let old = runtime.registry().unwrap().plugins[0]
        .origin
        .clone()
        .unwrap();
    let id = runtime
        .submit(Action::Permissions {
            plugin: m.id.clone(),
            decision: None,
        })
        .unwrap();
    assert!(!runtime.is_current(&old));
    wait(&runtime, id).unwrap();
    assert!(runtime.registry().unwrap().plugins[0].granted.is_empty());
    approve(&mut runtime, &m);
    let id = runtime.submit(Action::Enable(m.id.clone())).unwrap();
    wait(&runtime, id).unwrap();
    runtime.shutdown().unwrap();
    let mut restarted = Runtime::spawn(Some(root.path().into()), vec![]).unwrap();
    wait(&restarted, 0).unwrap();
    assert_eq!(
        restarted.registry().unwrap().plugins[0].status,
        Status::Enabled
    );
    restarted.shutdown().unwrap();
    let mut expanded = m.clone();
    expanded.capabilities.insert(Capability::DocumentEdit);
    std::fs::write(
        root.path().join("plugins/test.consent/manifest.json"),
        serde_json::to_vec(&expanded).unwrap(),
    )
    .unwrap();
    let mut expanded_runtime = Runtime::spawn(Some(root.path().into()), vec![]).unwrap();
    wait(&expanded_runtime, 0).unwrap();
    let registry = expanded_runtime.registry().unwrap();
    assert_eq!(registry.plugins[0].status, Status::Disabled);
    assert!(registry.plugins[0].granted.is_empty());
    expanded_runtime.shutdown().unwrap();
}

#[test]
fn dependencies_cannot_be_activated_by_approving_only_the_consumer() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let base = manifest("test.base");
    let mut consumer = manifest("test.consumer");
    consumer
        .dependencies
        .insert(base.id.clone(), "*".parse().unwrap());
    let mut runtime = Runtime::spawn(Some(root.path().into()), vec![]).unwrap();
    wait(&runtime, 0).unwrap();
    for m in [&base, &consumer] {
        let id = runtime
            .submit(Action::Install(Box::new(package(source.path(), m))))
            .unwrap();
        wait(&runtime, id).unwrap();
    }
    approve(&mut runtime, &consumer);
    let id = runtime.submit(Action::Enable(consumer.id.clone())).unwrap();
    assert!(wait(&runtime, id).err().unwrap().contains("test.base"));
    assert!(
        runtime
            .registry()
            .unwrap()
            .plugins
            .iter()
            .all(|p| p.status == Status::Disabled)
    );
    approve(&mut runtime, &base);
    let id = runtime.submit(Action::Enable(consumer.id.clone())).unwrap();
    wait(&runtime, id).unwrap();
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

struct FeaturePeer;
impl Plugin for FeaturePeer {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                effects: vec![Effect::Call {
                    call: ServiceCall {
                        request_id: "features".into(),
                        service: ServiceRef {
                            provider: "host".into(),
                            id: "features".into(),
                            version: "^1".parse().unwrap(),
                        },
                        method: "get".into(),
                        arguments: Value::Null,
                    },
                }],
                ..Default::default()
            },
            Request::ServiceResult { reply, .. } => {
                let ServiceOutcome::Success { value } = &reply.outcome else {
                    panic!("{:?}", reply.outcome)
                };
                let features: HostFeatures = serde_json::from_value(value.clone()).unwrap();
                assert!(features.granted.is_empty());
                assert!(features.services.contains_key("features"));
                Response {
                    effects: vec![Effect::Notify {
                        message: "Features received".into(),
                    }],
                    ..Default::default()
                }
            }
            _ => Response::default(),
        })
    }
}
#[test]
fn feature_discovery_does_not_require_services_or_other_privileges() {
    let mut m = manifest("test.features");
    m.backend = Backend::Builtin;
    m.capabilities.clear();
    let mut runtime = Runtime::spawn(
        None,
        vec![Builtin {
            manifest: m,
            enabled: true,
            create: std::sync::Arc::new(|| Box::new(FeaturePeer)),
        }],
    )
    .unwrap();
    let result = wait(&runtime, 0).unwrap();
    assert!(
        matches!(&result.effects[0].effect, Effect::Notify { message } if message == "Features received")
    );
    runtime.shutdown().unwrap();
}
