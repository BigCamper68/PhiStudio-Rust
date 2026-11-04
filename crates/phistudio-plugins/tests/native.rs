#![cfg(target_os = "linux")]
use phistudio_core::{ChartDocument, Editor};
use phistudio_plugins::*;
use phistudio_sdk::*;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

struct Fixture {
    _root: tempfile::TempDir,
    package: PathBuf,
    log: PathBuf,
}
fn fixture(abi: u32) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let package = root.path().join("package");
    std::fs::create_dir(&package).unwrap();
    let log = root.path().join("lifecycle.log");
    let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("cc")
        .args(["-shared", "-fPIC"])
        .arg(crate_root.join("tests/fixtures/native.c"))
        .arg("-I")
        .arg(crate_root.join("../phistudio-sdk/include"))
        .arg(format!(
            "-DTEST_LOG={}",
            serde_json::to_string(&log.to_string_lossy()).unwrap()
        ))
        .arg(format!("-DTEST_ABI={abi}"))
        .arg("-o")
        .arg(package.join("plugin.so"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(
        package.join("manifest.json"),
        serde_json::to_vec(&json!({
            "api":1,"id":"test.native","name":"Native C fixture","version":"1.0.0",
            "backend":{"kind":"native","entry":"plugin.so"},"capabilities":["document_edit"]
        }))
        .unwrap(),
    )
    .unwrap();
    Fixture {
        _root: root,
        package,
        log,
    }
}
fn wait(runtime: &Runtime, id: u64) -> Result<Output, String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(completion) = runtime.poll()? {
            assert_eq!(completion.id, id);
            return completion.result;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn command(arguments: Value) -> Action {
    Action::Command {
        plugin: "test.native".into(),
        id: "rename".into(),
        arguments,
        context: EditorContext {
            version: DocumentVersion {
                session: 1,
                revision: 0,
            },
            ..Default::default()
        },
    }
}

fn approve(runtime: &mut Runtime) {
    let manifest = runtime.registry().unwrap().plugins[0].manifest.clone();
    let requested = PermissionRequest::from_manifest(&manifest);
    let id = runtime
        .submit(Action::Permissions {
            plugin: manifest.id,
            decision: Some(PermissionDecision {
                requested: requested.clone(),
                granted: requested,
            }),
        })
        .unwrap();
    wait(runtime, id).unwrap();
}

#[test]
fn native_c_abi_is_lazy_frees_outputs_unloads_and_leaves_callback_free_history() {
    let fixture = fixture(1);
    let workspace = tempfile::tempdir().unwrap();
    let mut runtime = Runtime::spawn(Some(workspace.path().into()), vec![]).unwrap();
    wait(&runtime, 0).unwrap();
    let id = runtime
        .submit(Action::Install(Box::new(fixture.package)))
        .unwrap();
    wait(&runtime, id).unwrap();
    assert!(
        !fixture.log.exists(),
        "Installing must not execute native constructors"
    );
    let id = runtime.submit(Action::Reload).unwrap();
    wait(&runtime, id).unwrap();
    assert!(
        !fixture.log.exists(),
        "Discovery must not execute disabled native code"
    );
    let denied = runtime
        .submit(Action::Enable("test.native".into()))
        .unwrap();
    assert!(
        wait(&runtime, denied)
            .err()
            .unwrap()
            .contains("Review permissions")
    );
    assert!(
        !fixture.log.exists(),
        "Unapproved Enable must not execute native constructors"
    );
    approve(&mut runtime);
    let id = runtime
        .submit(Action::Enable("test.native".into()))
        .unwrap();
    wait(&runtime, id).unwrap();
    assert_eq!(
        std::fs::read_to_string(&fixture.log).unwrap(),
        "loaded\ncreated\nactivated\nfreed\n"
    );
    let id = runtime.submit(command(Value::Null)).unwrap();
    let effect = wait(&runtime, id).unwrap().effects.remove(0);
    let Effect::Edit { transaction } = effect.effect else {
        panic!()
    };
    let mut editor = Editor::new(ChartDocument::empty("Original", 120.).unwrap());
    assert!(
        runtime
            .apply_transaction(
                &effect.origin,
                &effect.required,
                1,
                &mut editor,
                transaction
            )
            .unwrap()
    );
    assert_eq!(editor.document().name(), "Native result");
    let id = runtime
        .submit(Action::Disable("test.native".into()))
        .unwrap();
    wait(&runtime, id).unwrap();
    let log = std::fs::read_to_string(&fixture.log).unwrap();
    assert!(
        log.ends_with("deactivated\nfreed\ndestroyed\nunloaded\n"),
        "{log}"
    );
    assert_eq!(log.matches("freed").count(), 3);
    assert!(editor.undo());
    assert_eq!(editor.document().name(), "Original");
    assert!(editor.redo());
    assert_eq!(editor.document().name(), "Native result");
    let id = runtime
        .submit(Action::Remove("test.native".into()))
        .unwrap();
    wait(&runtime, id).unwrap();
    assert!(!workspace.path().join("plugins/test.native").exists());
    runtime.shutdown().unwrap();
}

#[test]
fn incompatible_native_abi_is_rejected_before_creating_state() {
    let fixture = fixture(999);
    let workspace = tempfile::tempdir().unwrap();
    let mut runtime = Runtime::spawn(Some(workspace.path().into()), vec![]).unwrap();
    wait(&runtime, 0).unwrap();
    let id = runtime
        .submit(Action::Install(Box::new(fixture.package)))
        .unwrap();
    wait(&runtime, id).unwrap();
    assert!(!fixture.log.exists());
    approve(&mut runtime);
    let id = runtime
        .submit(Action::Enable("test.native".into()))
        .unwrap();
    assert!(wait(&runtime, id).err().unwrap().contains("incompatible"));
    assert_eq!(
        std::fs::read_to_string(&fixture.log).unwrap(),
        "loaded\nunloaded\n"
    );
    assert!(
        runtime.registry().unwrap().plugins[0]
            .contributions
            .is_empty()
    );
    runtime.shutdown().unwrap();
}

#[test]
fn native_errors_return_buffers_and_destroy_state_before_reporting_failure() {
    let fixture = fixture(1);
    let workspace = tempfile::tempdir().unwrap();
    let mut runtime = Runtime::spawn(Some(workspace.path().into()), vec![]).unwrap();
    wait(&runtime, 0).unwrap();
    let id = runtime
        .submit(Action::Install(Box::new(fixture.package)))
        .unwrap();
    wait(&runtime, id).unwrap();
    approve(&mut runtime);
    let id = runtime
        .submit(Action::Enable("test.native".into()))
        .unwrap();
    wait(&runtime, id).unwrap();
    let id = runtime.submit(command(json!("fail"))).unwrap();
    assert!(
        wait(&runtime, id)
            .err()
            .unwrap()
            .contains("Expected native failure")
    );
    let log = std::fs::read_to_string(&fixture.log).unwrap();
    assert!(
        log.ends_with("failed\nfreed\ndestroyed\nunloaded\n"),
        "{log}"
    );
    assert!(matches!(
        &runtime.registry().unwrap().plugins[0].status,
        Status::Failed(_)
    ));
    assert!(
        runtime.registry().unwrap().plugins[0]
            .contributions
            .is_empty()
    );
    runtime.shutdown().unwrap();
}

#[test]
fn native_module_headers_reject_wrong_architectures_and_nonlibraries() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("invalid.so");
    for prefix in [
        vec![0_u8; 64],
        b"\x7fELF".iter().copied().chain([0; 60]).collect(),
    ] {
        std::fs::write(&path, prefix).unwrap();
        assert!(validate_native_file(&path).is_err());
    }
}

#[test]
fn invalid_native_buffers_do_not_free_host_input_or_read_oversized_memory() {
    for (argument, message, frees) in [
        ("alias", "separate allocation", 1),
        ("oversized-alias", "separate allocation", 1),
        ("oversized", "message limit", 2),
        ("invalid-json", "Invalid native response", 2),
    ] {
        let fixture = fixture(1);
        let workspace = tempfile::tempdir().unwrap();
        let mut runtime = Runtime::spawn(Some(workspace.path().into()), vec![]).unwrap();
        wait(&runtime, 0).unwrap();
        let id = runtime
            .submit(Action::Install(Box::new(fixture.package)))
            .unwrap();
        wait(&runtime, id).unwrap();
        approve(&mut runtime);
        let id = runtime
            .submit(Action::Enable("test.native".into()))
            .unwrap();
        wait(&runtime, id).unwrap();
        let id = runtime.submit(command(json!(argument))).unwrap();
        let error = wait(&runtime, id).err().unwrap();
        assert!(error.contains(message), "{argument}: {error}");
        let log = std::fs::read_to_string(&fixture.log).unwrap();
        assert_eq!(log.matches("freed").count(), frees, "{argument}: {log}");
        assert!(log.ends_with("destroyed\nunloaded\n"));
        runtime.shutdown().unwrap();
    }
}
