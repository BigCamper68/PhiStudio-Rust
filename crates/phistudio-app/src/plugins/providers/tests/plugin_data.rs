use super::*;
use phistudio_sdk::{
    PLUGIN_DATA_KEY, PatchOperation, PluginData, PluginDataExport, PluginDataPolicy,
};
use std::io::Read;

fn unpack(path: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
    let mut source = std::io::Cursor::new(std::fs::read(path).unwrap());
    let mut magic = [0; 20];
    source.read_exact(&mut magic).unwrap();
    assert_eq!(&magic, b"PhiStudio project 1\n");
    let mut count = [0; 4];
    source.read_exact(&mut count).unwrap();
    let mut result = BTreeMap::new();
    for _ in 0..u32::from_le_bytes(count) {
        let mut name_size = [0; 4];
        source.read_exact(&mut name_size).unwrap();
        let mut data_size = [0; 8];
        source.read_exact(&mut data_size).unwrap();
        let mut name = vec![0; u32::from_le_bytes(name_size) as usize];
        source.read_exact(&mut name).unwrap();
        let mut data = vec![0; u64::from_le_bytes(data_size) as usize];
        source.read_exact(&mut data).unwrap();
        result.insert(String::from_utf8(name).unwrap(), data);
    }
    result
}
#[test]
#[ignore = "requires the separately built chart-envelope and project-pack Wasm examples"]
fn compiled_wasm_provider_export_policies_filter_copies_through_real_controls_and_preserve_sources()
{
    for example in ["chart-envelope", "project-pack"] {
        let root = tempfile::tempdir().unwrap();
        let assets = root.path().join("assets");
        std::fs::create_dir_all(assets.join("sub")).unwrap();
        let mut value = ChartDocument::empty("Export policy", 120.)
            .unwrap()
            .as_json()
            .clone();
        value[PLUGIN_DATA_KEY] = json!({
            "test.editor": PluginData::new(1, PluginDataPolicy::EditorOnly, json!({"exact":[null,42]})),
            "test.runtime": PluginData::new(2, PluginDataPolicy::Runtime, json!({"keep":true})),
            "test.legacy": {"keep":[true,null]},
        });
        value["futureRoot"] = json!({"keep":42});
        let document = ChartDocument::from_value(value.clone()).unwrap();
        let saved = assets.join("original.rpe.json");
        phistudio_core::save_chart(&document, &saved).unwrap();
        let second = assets.join("sub/second.json");
        phistudio_core::save_chart(&document, &second).unwrap();
        let resource = br#"{ "phiStudioPlugins": "resource JSON", "keep": null }"#;
        std::fs::write(assets.join("resource.json"), resource).unwrap();
        let mut app = Workbench::new(document, Some(saved.clone()));
        let mut files = FileManager::new(Some(root.path().into()));
        let ctx = egui::Context::default();
        app.start_plugins(Some(root.path().into()));
        settle(&mut app, &mut files, &ctx);
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/plugins")
            .join(example);
        app.install_plugin(MediaFile::new(source, example.into(), false));
        settle(&mut app, &mut files, &ctx);
        app.enable_test_plugin(format!("example.{example}"));
        settle(&mut app, &mut files, &ctx);
        app.plugins.open = true;
        for (policy, label) in [
            (PluginDataExport::Preserve, "Preserve all plugin data"),
            (PluginDataExport::Runtime, "Runtime data only"),
            (PluginDataExport::Remove, "Remove all plugin data"),
        ] {
            click(&mut app, &mut files, &ctx, "Export with plugin");
            let target = root.path().join(format!("{example}-{policy:?}.out"));
            app.plugins.providers.dialog.as_mut().unwrap().destination =
                target.to_string_lossy().into_owned();
            if policy != PluginDataExport::Preserve {
                click(&mut app, &mut files, &ctx, "Preserve all plugin data");
                click(&mut app, &mut files, &ctx, label);
            }
            assert_eq!(
                app.plugins.providers.dialog.as_ref().unwrap().plugin_data,
                policy
            );
            click(&mut app, &mut files, &ctx, "Export with plugin");
            settle(&mut app, &mut files, &ctx);
            assert!(app.error.is_none(), "{example} {policy:?}: {:?}", app.error);
            let charts = if example == "chart-envelope" {
                let bytes = std::fs::read(&target).unwrap();
                vec![
                    serde_json::from_slice::<Value>(&bytes[b"PhiStudio chart 1\n".len()..])
                        .unwrap(),
                ]
            } else {
                let entries = unpack(&target);
                assert_eq!(entries["resource.json"], resource);
                ["original.rpe.json", "sub/second.json"]
                    .map(|name| serde_json::from_slice::<Value>(&entries[name]).unwrap())
                    .to_vec()
            };
            for chart in charts {
                match policy {
                    PluginDataExport::Preserve => {
                        assert_eq!(chart[PLUGIN_DATA_KEY], value[PLUGIN_DATA_KEY])
                    }
                    PluginDataExport::Runtime => {
                        assert!(chart[PLUGIN_DATA_KEY].get("test.editor").is_none());
                        assert_eq!(
                            chart[PLUGIN_DATA_KEY]["test.runtime"],
                            value[PLUGIN_DATA_KEY]["test.runtime"]
                        );
                        assert_eq!(
                            chart[PLUGIN_DATA_KEY]["test.legacy"],
                            value[PLUGIN_DATA_KEY]["test.legacy"]
                        );
                    }
                    PluginDataExport::Remove => assert!(chart.get(PLUGIN_DATA_KEY).is_none()),
                }
                assert_eq!(chart["futureRoot"], value["futureRoot"]);
            }
            assert_eq!(app.editor.document().as_json(), &value);
            assert_eq!(
                phistudio_core::load_chart(&saved).unwrap().as_json(),
                &value
            );
            assert_eq!(
                phistudio_core::load_chart(&second).unwrap().as_json(),
                &value
            );
            assert!(!app.editor.is_dirty() && !app.editor.can_undo());
        }
        app.editor
            .apply_patch(
                app.editor.revision(),
                "Required extension data".into(),
                vec![PatchOperation::PluginData {
                    plugin: "test.required".into(),
                    entry: Some(PluginData::new(
                        1,
                        PluginDataPolicy::RequiresConversion,
                        Value::Null,
                    )),
                }],
            )
            .unwrap();
        let original = app.editor.document().as_json().clone();
        click(&mut app, &mut files, &ctx, "Export with plugin");
        let rejected = root.path().join("must-not-exist.out");
        app.plugins.providers.dialog.as_mut().unwrap().destination =
            rejected.to_string_lossy().into_owned();
        click(&mut app, &mut files, &ctx, "Preserve all plugin data");
        click(&mut app, &mut files, &ctx, "Runtime data only");
        click(&mut app, &mut files, &ctx, "Export with plugin");
        settle(&mut app, &mut files, &ctx);
        assert!(app.error.as_ref().unwrap().contains("requires conversion"));
        assert!(!rejected.exists());
        assert_eq!(app.editor.document().as_json(), &original);
        assert!(app.editor.is_dirty() && app.editor.can_undo());
        assert_eq!(
            app.plugins.registry.plugins[0].status,
            phistudio_plugins::Status::Enabled
        );
        app.plugins.shutdown().unwrap();
    }
}
