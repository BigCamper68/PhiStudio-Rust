use phistudio_core::{ChartDocument, Editor, Note};
use phistudio_plugins::{Action, Package, Runtime, WasmLimits};
use phistudio_sdk::*;
use serde_json::json;

/// Build examples/plugins/note-shift first. This measures a compiled Rust guest,
/// including JSON decoding/encoding, rather than a constant WAT response.
#[test]
#[ignore = "requires the separately built external Rust Wasm example"]
fn compiled_rust_guest_edits_large_charts_and_retains_extensions() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/note-shift");
    let package = Package::open(&root).unwrap();
    let mut wasm = package.load_wasm(WasmLimits::default()).unwrap();
    let settings = package
        .manifest
        .settings
        .iter()
        .map(|(k, s)| (k.clone(), s.default_value()))
        .collect();
    wasm.call(&Request::Activate {
        manifest: package.manifest,
        settings,
        storage: serde_json::Value::Null,
    })
    .unwrap();
    for count in [4000, 20_000] {
        let mut source = ChartDocument::empty("Real guest", 120.)
            .unwrap()
            .as_json()
            .clone();
        let mut note = serde_json::to_value(Note::default()).unwrap();
        note["unknown"] = json!({"preserve":[1,null,3]});
        source["judgeLineList"][0]["notes"] = json!(vec![note; count]);
        source["judgeLineList"][0]["numOfNotes"] = json!(count);
        let mut editor = Editor::new(ChartDocument::from_value(source.clone()).unwrap());
        let started = std::time::Instant::now();
        let response = wasm
            .call(&Request::Command {
                id: "shift".into(),
                arguments: json!({"offset":25}),
                context: EditorContext {
                    version: DocumentVersion {
                        session: 1,
                        revision: 0,
                    },
                    chart: Some(source.clone()),
                    ..Default::default()
                },
            })
            .unwrap();
        println!(
            "Compiled guest ({count} notes): {:?}, {} fuel",
            started.elapsed(),
            wasm.fuel_consumed()
        );
        let Effect::Edit { transaction } = response.effects.into_iter().next().unwrap() else {
            panic!()
        };
        assert!(
            editor
                .apply_patch(0, transaction.label, transaction.operations)
                .unwrap()
        );
        for note in editor.document().as_json()["judgeLineList"][0]["notes"]
            .as_array()
            .unwrap()
        {
            assert_eq!(note["positionX"], 25.);
            assert_eq!(note["unknown"], json!({"preserve":[1,null,3]}));
        }
        editor.undo();
        assert_eq!(editor.document().as_json(), &source);
    }
}

#[test]
#[ignore = "requires the separately built note-statistics Wasm example"]
fn compiled_statistics_guest_scans_large_charts_through_the_worker_and_services() {
    use std::time::{Duration, Instant};
    fn wait(runtime: &Runtime, id: u64) -> phistudio_plugins::Output {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(completion) = runtime.poll().unwrap() {
                assert_eq!(completion.id, id);
                return completion.result.unwrap();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let root = tempfile::tempdir().unwrap();
    let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/plugins/note-statistics");
    let mut runtime = Runtime::spawn(Some(root.path().into()), vec![]).unwrap();
    wait(&runtime, 0);
    let id = runtime.submit(Action::Install(Box::new(package))).unwrap();
    wait(&runtime, id);
    let id = runtime
        .submit(Action::Enable("example.note-statistics".into()))
        .unwrap();
    wait(&runtime, id);
    for count in [4000, 20_000] {
        let mut source = ChartDocument::empty("Paged guest", 120.)
            .unwrap()
            .as_json()
            .clone();
        source["judgeLineList"][0]["notes"] =
            json!(vec![serde_json::to_value(Note::default()).unwrap(); count]);
        source["judgeLineList"][0]["numOfNotes"] = json!(count);
        let document = ChartDocument::from_value(source.clone()).unwrap();
        let context = EditorContext {
            version: DocumentVersion {
                session: 1,
                revision: 0,
            },
            ..Default::default()
        };
        let started = Instant::now();
        let mut id = runtime
            .submit(Action::Command {
                plugin: "example.note-statistics".into(),
                id: "count".into(),
                arguments: serde_json::Value::Null,
                context: context.clone(),
            })
            .unwrap();
        let mut queries = 0;
        let mut label = String::new();
        loop {
            let output = wait(&runtime, id);
            for effect in output.effects {
                match effect.effect {
                    Effect::Panel { widgets, .. } => {
                        for widget in widgets {
                            if let Widget::Label { text } = widget {
                                label = text;
                            }
                        }
                    }
                    Effect::Edit { .. } => panic!("Statistics must not edit the chart"),
                    _ => {}
                }
            }
            let mut calls = output.calls;
            if calls.is_empty() {
                break;
            }
            assert_eq!(calls.len(), 1);
            let call = calls.remove(0);
            let outcome = runtime.document_service(&call, context.version, &document);
            id = runtime
                .submit(Action::ServiceReply { call, outcome })
                .unwrap();
            queries += 1;
        }
        eprintln!(
            "Compiled paged guest ({count} notes, {queries} queries): {:?}",
            started.elapsed()
        );
        assert!(label.contains(&format!("{count} notes")), "{label}");
        assert!(queries > 1);
        assert_eq!(document.as_json(), &source);
    }
    runtime.shutdown().unwrap();
}
