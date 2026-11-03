use phistudio_plugins::*;
use phistudio_sdk::*;
use serde_json::{Value, json};

fn data(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{b:02x}")).collect()
}
fn module(activate: &Response, action: &Response) -> Vec<u8> {
    let a = serde_json::to_vec(activate).unwrap();
    let b = serde_json::to_vec(action).unwrap();
    wat::parse_str(format!(
        r#"(module
        (memory (export "memory") 2)
        (global $called (mut i32) (i32.const 0))
        (data (i32.const 1024) "{}") (data (i32.const 8192) "{}")
        (func (export "phistudio_api_version") (result i32) i32.const 1)
        (func (export "phistudio_alloc") (param i32) (result i32) i32.const 32768)
        (func (export "phistudio_free") (param i32 i32))
        (func (export "phistudio_call") (param i32 i32) (result i64)
            global.get $called i32.eqz
            if (result i64)
                i32.const 1 global.set $called i64.const {}
            else i64.const {} end)
    )"#,
        data(&a),
        data(&b),
        ((a.len() as u64) << 32) | 1024,
        ((b.len() as u64) << 32) | 8192
    ))
    .unwrap()
}

#[test]
fn real_wasm_package_edits_chart_and_history_survives_module_removal() {
    let manifest: Manifest = serde_json::from_value(
        json!({"api":1,"id":"example.wasm","name":"Wasm example","version":"1.0.0",
        "backend":{"kind":"wasm","entry":"plugin.wasm"},"capabilities":["document_edit"]}),
    )
    .unwrap();
    let action = Response {
        effects: vec![Effect::Edit {
            transaction: Transaction {
                version: DocumentVersion {
                    session: 42,
                    revision: 0,
                },
                label: "Wasm edit".into(),
                operations: vec![PatchOperation::Replace {
                    path: "/META/name".into(),
                    value: json!("Wasm result"),
                }],
            },
        }],
        ..Default::default()
    };
    let activation = Response {
        contributions: vec![Contribution::Command {
            id: "run".into(),
            title: "Run".into(),
            shortcut: None,
            snapshot: false,
        }],
        ..Default::default()
    };
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join(MANIFEST_NAME),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.path().join("plugin.wasm"),
        module(&activation, &action),
    )
    .unwrap();
    let package = Package::open(root.path()).unwrap();
    let instance = package.load_wasm(WasmLimits::default()).unwrap();
    let mut host = Host::default();
    host.install(manifest.clone(), Box::new(instance), manifest.capabilities)
        .unwrap();
    host.enable(&manifest.id).unwrap();
    let context = EditorContext {
        version: DocumentVersion {
            session: 42,
            revision: 0,
        },
        ..Default::default()
    };
    let result = host
        .command(&manifest.id, "run", Value::Null, context)
        .unwrap();
    let mut editor = phistudio_core::Editor::new(
        phistudio_core::ChartDocument::empty("Original", 120.).unwrap(),
    );
    for owned in result.effects {
        let Effect::Edit { transaction } = owned.effect else {
            panic!()
        };
        host.apply_transaction(&owned.origin, 42, &mut editor, transaction)
            .unwrap();
    }
    assert_eq!(editor.document().name(), "Wasm result");
    host.uninstall(&manifest.id);
    assert!(editor.undo());
    assert_eq!(editor.document().name(), "Original");
    assert!(editor.redo());
    assert_eq!(editor.document().name(), "Wasm result");
}

fn raw(body: &str, start: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
        (memory (export "memory") 1)
        (func (export "phistudio_api_version") (result i32) i32.const 1)
        (func (export "phistudio_alloc") (param i32) (result i32) i32.const 8192)
        (func (export "phistudio_free") (param i32 i32))
        (func (export "phistudio_call") (param i32 i32) (result i64) {body}) {start})"#
    ))
    .unwrap()
}

#[test]
fn runaway_callbacks_start_functions_and_memory_growth_are_bounded() {
    let limits = WasmLimits {
        fuel_per_call: 1000,
        memory_bytes: 65536,
        ..Default::default()
    };
    let mut looping =
        WasmPlugin::load(&raw("(loop $forever br $forever) i64.const 0", ""), limits).unwrap();
    assert!(looping.call(&Request::Deactivate).is_err());
    assert!(
        looping
            .call(&Request::Deactivate)
            .unwrap_err()
            .contains("reload")
    );
    assert!(
        WasmPlugin::load(
            &raw(
                "i64.const 0",
                "(func $init (loop $forever br $forever)) (start $init)"
            ),
            limits
        )
        .is_err()
    );
    let mut growing =
        WasmPlugin::load(&raw("i32.const 1 memory.grow drop i64.const 0", ""), limits).unwrap();
    assert!(growing.call(&Request::Deactivate).is_err());
}

#[test]
fn invalid_output_ranges_oversized_responses_and_os_imports_are_rejected() {
    for output in [
        (2u64 << 32) | u64::from(u32::MAX),
        ((MAX_MESSAGE_BYTES as u64 + 1) << 32) | 1024,
        (2u64 << 32) | 8192,
        (2u64 << 32) | 1024,
        0,
    ] {
        let mut plugin = WasmPlugin::load(
            &raw(&format!("i64.const {output}"), ""),
            WasmLimits::default(),
        )
        .unwrap();
        assert!(plugin.call(&Request::Deactivate).is_err());
    }
    let imports =
        wat::parse_str(r#"(module (import "wasi_snapshot_preview1" "fd_read" (func)))"#).unwrap();
    assert!(
        WasmPlugin::load(&imports, WasmLimits::default())
            .err()
            .unwrap()
            .contains("import")
    );
}

#[test]
fn incompatible_abi_and_excess_initial_memory_fail_before_activation() {
    let wrong_version = wat::parse_str(
        r#"(module
        (func (export "phistudio_api_version") (result i32) i32.const 2))"#,
    )
    .unwrap();
    assert!(
        WasmPlugin::load(&wrong_version, WasmLimits::default())
            .err()
            .unwrap()
            .contains("API 2")
    );
    let limits = WasmLimits {
        memory_bytes: 1,
        ..Default::default()
    };
    assert!(WasmPlugin::load(&raw("i64.const 0", ""), limits).is_err());
}
