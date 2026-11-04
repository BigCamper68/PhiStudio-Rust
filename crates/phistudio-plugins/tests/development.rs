use phistudio_plugins::*;
use phistudio_sdk::*;
use serde_json::json;

fn package(root: &std::path::Path) {
    let manifest = json!({"api":1,"id":"test.development","name":"Development","version":"1.0.0","backend":{"kind":"wasm","entry":"plugin.wasm"}});
    std::fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let bytes = wat::parse_str(
        r#"(module (memory (export "memory") 1) (data (i32.const 1024) "{}")
        (func (export "phistudio_api_version") (result i32) i32.const 1)
        (func (export "phistudio_alloc") (param i32) (result i32) i32.const 8192)
        (func (export "phistudio_free") (param i32 i32))
        (func (export "phistudio_call") (param i32 i32) (result i64) i64.const 8589935616))"#,
    )
    .unwrap();
    std::fs::write(root.join("plugin.wasm"), bytes).unwrap();
}
#[test]
fn cli_validates_packages_archives_and_templates_without_replacing_files() {
    let source = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    package(source.path());
    let archive = output.path().join("plugin.zip");
    pack_external_package(source.path(), &archive).unwrap();
    let info = inspect_external_package(&archive).unwrap();
    assert_eq!(info["manifest"]["id"], "test.development");
    assert_eq!(info["activated"], false);
    assert!(pack_external_package(source.path(), &archive).is_err());
    let cli = env!("CARGO_BIN_EXE_phistudio-plugin");
    assert!(
        std::process::Command::new(cli)
            .arg("validate")
            .arg(&archive)
            .status()
            .unwrap()
            .success()
    );
    let template = output.path().join("example");
    let sdk = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../phistudio-sdk");
    create_plugin_template(&template, "example.cli-template", &sdk).unwrap();
    assert!(create_plugin_template(&template, "example.cli-template", &sdk).is_err());
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(template.join("manifest.json")).unwrap()).unwrap();
    manifest.validate().unwrap();
    assert!(
        std::fs::read_to_string(template.join("src/lib.rs"))
            .unwrap()
            .contains("export_plugin!(|| Example)")
    );
    std::fs::write(source.path().join("plugin.wasm"), b"invalid wasm").unwrap();
    assert!(inspect_external_package(source.path()).is_err());
    assert!(
        !std::process::Command::new(cli)
            .arg("validate")
            .arg(source.path())
            .status()
            .unwrap()
            .success()
    );
}

struct Metrics;
impl Plugin for Metrics {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![Contribution::Command {
                    id: "call".into(),
                    title: "Call".into(),
                    shortcut: None,
                    snapshot: false,
                }],
                ..Default::default()
            },
            _ => Response::default(),
        })
    }
}
#[test]
fn diagnostics_retain_only_bounded_callback_metadata_and_wasm_cost() {
    let mut host = Host::default();
    let manifest:Manifest=serde_json::from_value(json!({"api":1,"id":"test.metrics","name":"Metrics","version":"1.0.0","backend":{"kind":"builtin"}})).unwrap();
    host.install(manifest, Box::new(Metrics), Default::default())
        .unwrap();
    host.enable("test.metrics").unwrap();
    for _ in 0..100 {
        host.command(
            "test.metrics",
            "call",
            json!({"secret":"do-not-log-this"}),
            EditorContext::default(),
        )
        .unwrap();
    }
    let info = host.snapshot().pop().unwrap();
    assert_eq!(info.diagnostics.calls, 101);
    assert_eq!(info.diagnostics.recent.len(), 64);
    assert!(
        !serde_json::to_string(&info.diagnostics)
            .unwrap()
            .contains("do-not-log-this")
    );
    let source = tempfile::tempdir().unwrap();
    package(source.path());
    let package = Package::open(source.path()).unwrap();
    let mut wasm = package.load_wasm(WasmLimits::default()).unwrap();
    wasm.call(&Request::Deactivate).unwrap();
    let cost = wasm.execution_cost();
    assert!(cost.fuel.unwrap() > 0);
    assert_eq!(cost.memory_bytes, Some(65536));
}
