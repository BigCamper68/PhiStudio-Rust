use phistudio_core::*;
use phistudio_sdk::*;
use serde_json::{Value, json};
use std::{fs, io::Read, path::Path};

fn entry(policy: PluginDataPolicy) -> PluginData {
    PluginData::new(
        2,
        policy,
        json!({"keep":[null,{"source":"Exact custom data"}]}),
    )
}
fn document() -> ChartDocument {
    let mut value = ChartDocument::empty("Plugin data", 123.)
        .unwrap()
        .as_json()
        .clone();
    value["futureField"] = json!({"nested":[null,42,true]});
    ChartDocument::from_value(value).unwrap()
}
#[test]
fn plugin_data_and_note_edits_share_one_atomic_sparse_history_entry() {
    let mut editor = Editor::new(document());
    let original = editor.document().as_json().clone();
    let original_text = editor.document().to_json().unwrap();
    let operations = vec![
        PatchOperation::Add {
            path: "/judgeLineList/0/notes/-".into(),
            value: serde_json::to_value(Note::default()).unwrap(),
        },
        PatchOperation::PluginData {
            plugin: "example.owner".into(),
            entry: Some(entry(PluginDataPolicy::EditorOnly)),
        },
    ];
    assert!(
        editor
            .apply_patch(0, "Note and project data".into(), operations)
            .unwrap()
    );
    let changed = editor.document().as_json().clone();
    assert_eq!(editor.history_rows(), vec![("Note and project data", true)]);
    assert_eq!(changed[PLUGIN_DATA_KEY]["example.owner"]["version"], 2);
    assert_eq!(
        changed[PLUGIN_DATA_KEY]["example.owner"]["data"]["keep"][0],
        Value::Null
    );
    assert_eq!(changed["futureField"], original["futureField"]);
    assert_eq!(editor.document().note_count(), 1);
    editor.undo();
    assert_eq!(editor.document().as_json(), &original);
    assert_eq!(editor.document().to_json().unwrap(), original_text);
    editor.redo();
    assert_eq!(editor.document().as_json(), &changed);
    editor
        .apply_patch(
            editor.revision(),
            "Remove data".into(),
            vec![PatchOperation::PluginData {
                plugin: "example.owner".into(),
                entry: None,
            }],
        )
        .unwrap();
    assert!(editor.document().as_json().get(PLUGIN_DATA_KEY).is_none());
    editor.undo();
    assert_eq!(editor.document().as_json(), &changed);
    let revision = editor.revision();
    for operation in [
        PatchOperation::PluginData {
            plugin: "../invalid".into(),
            entry: Some(entry(PluginDataPolicy::Runtime)),
        },
        PatchOperation::PluginData {
            plugin: "example.owner".into(),
            entry: Some(PluginData::new(0, PluginDataPolicy::Runtime, Value::Null)),
        },
    ] {
        assert!(
            editor
                .apply_patch(
                    revision,
                    "Must roll back".into(),
                    vec![
                        PatchOperation::Replace {
                            path: "/META/name".into(),
                            value: json!("Wrong")
                        },
                        operation
                    ]
                )
                .is_err()
        );
        assert_eq!(editor.revision(), revision);
        assert_eq!(editor.document().as_json(), &changed);
    }
    let mut collision = original.clone();
    collision[PLUGIN_DATA_KEY] = json!("legacy opaque namespace");
    let mut editor = Editor::new(ChartDocument::from_value(collision.clone()).unwrap());
    assert!(
        editor
            .apply_patch(
                0,
                "Preserve legacy data".into(),
                vec![PatchOperation::PluginData {
                    plugin: "example.owner".into(),
                    entry: Some(entry(PluginDataPolicy::Runtime))
                }]
            )
            .is_err()
    );
    assert_eq!(editor.document().as_json(), &collision);
}

fn with_data() -> ChartDocument {
    let mut value = document().as_json().clone();
    value[PLUGIN_DATA_KEY] = json!({
        "test.runtime": entry(PluginDataPolicy::Runtime),
        "test.editor": entry(PluginDataPolicy::EditorOnly),
        "test.legacy": {"keep":true},
        "test.null": null,
        "test.future": {"format":"phistudio.plugin-data/2","version":1,"export":"editor_only","data":{}},
        "test.extra": {"format":PLUGIN_DATA_FORMAT,"version":1,"export":"editor_only","data":{},"future":true},
    });
    ChartDocument::from_value(value).unwrap()
}
#[test]
fn explicit_export_policies_preserve_unknown_data_and_only_modify_copies() {
    let document = with_data();
    let original = document.as_json().clone();
    assert_eq!(
        document
            .with_plugin_data_export(PluginDataExport::Preserve)
            .unwrap()
            .as_json(),
        &original
    );
    let exported = document
        .with_plugin_data_export(PluginDataExport::Runtime)
        .unwrap();
    assert!(
        exported.as_json()[PLUGIN_DATA_KEY]
            .get("test.editor")
            .is_none()
    );
    for key in [
        "test.runtime",
        "test.legacy",
        "test.null",
        "test.future",
        "test.extra",
    ] {
        assert_eq!(
            exported.as_json()[PLUGIN_DATA_KEY].get(key),
            original[PLUGIN_DATA_KEY].get(key),
            "{key}"
        );
    }
    assert!(
        document
            .with_plugin_data_export(PluginDataExport::Remove)
            .unwrap()
            .as_json()
            .get(PLUGIN_DATA_KEY)
            .is_none()
    );
    assert_eq!(document.as_json(), &original);
    let root = tempfile::tempdir().unwrap();
    let chart = root.path().join("saved.rpe.json");
    save_chart(&document, &chart).unwrap();
    assert_eq!(load_chart(&chart).unwrap().as_json(), &original);
    save_recovery(
        root.path(),
        &RecoverySnapshot {
            document: document.clone(),
            location: None,
            beat: 3.,
            line: 0,
        },
    )
    .unwrap();
    assert_eq!(
        load_recovery(root.path())
            .unwrap()
            .unwrap()
            .document
            .as_json(),
        &original
    );
    let mut required = original.clone();
    required[PLUGIN_DATA_KEY]["test.required"] = json!(entry(PluginDataPolicy::RequiresConversion));
    let required = ChartDocument::from_value(required).unwrap();
    assert!(
        required
            .with_plugin_data_export(PluginDataExport::Runtime)
            .unwrap_err()
            .to_string()
            .contains("test.required")
    );
    assert!(
        required
            .with_plugin_data_export(PluginDataExport::Preserve)
            .is_ok()
    );
    assert!(
        required
            .with_plugin_data_export(PluginDataExport::Remove)
            .is_ok()
    );
}

fn zip_value(path: &Path, name: &str) -> Value {
    let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    serde_json::from_reader(archive.by_name(name).unwrap()).unwrap()
}
#[test]
fn project_export_filters_every_rpe_chart_but_preserves_resources_and_saved_files() {
    let root = tempfile::tempdir().unwrap();
    let assets = root.path().join("assets");
    fs::create_dir_all(assets.join("sub")).unwrap();
    let active = with_data();
    let original = active.as_json().clone();
    let chart = assets.join("active.rpe.json");
    save_chart(&active, &chart).unwrap();
    let second = assets.join("sub/second.json");
    save_chart(&active, &second).unwrap();
    let opaque = br#"{ "phiStudioPlugins": {"keep": "opaque resource JSON"}, "unknown": null }"#;
    fs::write(assets.join("resource.json"), opaque).unwrap();
    fs::write(assets.join("broken.json"), b"not a chart").unwrap();
    let options = ProjectOptions::default();
    let preserve = prepare_project_export(&active, &options, Some(&assets), Some(&chart)).unwrap();
    assert_eq!(zip_value(preserve.path(), "sub/second.json"), original);
    for policy in [PluginDataExport::Runtime, PluginDataExport::Remove] {
        let package = prepare_project_export_with_plugin_data(
            &active,
            &options,
            Some(&assets),
            Some(&chart),
            policy,
        )
        .unwrap();
        for name in ["active.rpe.json", "sub/second.json"] {
            let exported = zip_value(package.path(), name);
            assert!(exported[PLUGIN_DATA_KEY].get("test.editor").is_none());
            if policy == PluginDataExport::Runtime {
                assert_eq!(
                    exported[PLUGIN_DATA_KEY]["test.legacy"],
                    original[PLUGIN_DATA_KEY]["test.legacy"]
                );
                assert_eq!(
                    exported[PLUGIN_DATA_KEY]["test.runtime"],
                    original[PLUGIN_DATA_KEY]["test.runtime"]
                );
            } else {
                assert!(exported.get(PLUGIN_DATA_KEY).is_none());
            }
            assert_eq!(exported["futureField"], original["futureField"]);
        }
        let mut archive = zip::ZipArchive::new(fs::File::open(package.path()).unwrap()).unwrap();
        let mut bytes = vec![];
        archive
            .by_name("resource.json")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, opaque);
        let mut bytes = vec![];
        archive
            .by_name("broken.json")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, b"not a chart");
        assert_eq!(load_chart(&chart).unwrap().as_json(), &original);
        assert_eq!(load_chart(&second).unwrap().as_json(), &original);
    }
    let mut required = original.clone();
    required[PLUGIN_DATA_KEY]["test.required"] = json!(entry(PluginDataPolicy::RequiresConversion));
    save_chart(&ChartDocument::from_value(required).unwrap(), &second).unwrap();
    assert!(
        prepare_project_export_with_plugin_data(
            &active,
            &options,
            Some(&assets),
            Some(&chart),
            PluginDataExport::Runtime
        )
        .is_err()
    );
    assert_eq!(active.as_json(), &original);
}
