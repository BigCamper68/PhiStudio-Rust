use super::*;

#[test]
#[ignore = "requires the separately built project-data Wasm example"]
fn compiled_wasm_project_data_persists_through_save_reload_projects_and_independent_undo() {
    let root = tempfile::tempdir().unwrap();
    let mut value = ChartDocument::empty("Project tags", 120.)
        .unwrap()
        .as_json()
        .clone();
    value[PLUGIN_DATA_KEY] = json!({"future.plugin":{"keep":[null,42]}});
    let mut app = Workbench::new(ChartDocument::from_value(value.clone()).unwrap(), None);
    app.workspace.root = Some(root.path().into());
    app.start_plugins(Some(root.path().into()));
    settle(&mut app);
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/plugins/project-data");
    app.install_plugin(MediaFile::new(source, "Project data".into(), false));
    settle(&mut app);
    app.enable_test_plugin("example.project-data".into());
    settle(&mut app);
    click_panel(&mut app, "Store tag in project");
    let changed = app.editor.document().as_json().clone();
    assert_eq!(
        changed[PLUGIN_DATA_KEY]["example.project-data"]["data"]["tag"],
        "My tag"
    );
    assert_eq!(
        changed[PLUGIN_DATA_KEY]["example.project-data"]["export"],
        "editor_only"
    );
    assert_eq!(
        changed[PLUGIN_DATA_KEY]["future.plugin"],
        value[PLUGIN_DATA_KEY]["future.plugin"]
    );
    assert_eq!(app.editor.history_rows(), vec![("Store project tag", true)]);
    app.plugin_action(Action::Disable("example.project-data".into()));
    settle(&mut app);
    assert!(app.editor.undo());
    assert_eq!(app.editor.document().as_json(), &value);
    assert!(app.editor.redo());
    assert_eq!(app.editor.document().as_json(), &changed);
    let saved = root.path().join("saved.rpe.json");
    app.save(saved.clone());
    settle(&mut app);
    assert_eq!(
        phistudio_core::load_chart(&saved).unwrap().as_json(),
        &changed
    );
    app.enable_test_plugin("example.project-data".into());
    settle(&mut app);
    let panel = &app.plugins.panels[&("example.project-data".into(), "tag".into())];
    assert!(
        panel
            .widgets
            .iter()
            .any(|w| matches!(w, Widget::Label { text } if text == "Project tag loaded"))
    );
    app.plugin_action(Action::Reload);
    settle(&mut app);
    assert_eq!(app.editor.document().as_json(), &changed);
    app.replace_document(
        ChartDocument::empty("Another project", 120.).unwrap(),
        None,
        None,
    );
    settle(&mut app);
    assert!(
        app.editor
            .document()
            .as_json()
            .get(PLUGIN_DATA_KEY)
            .is_none()
    );
    let panel = &app.plugins.panels[&("example.project-data".into(), "tag".into())];
    assert!(
        panel
            .widgets
            .iter()
            .any(|w| matches!(w, Widget::Label { text } if text == "No project tag stored"))
    );
    app.replace_document(
        phistudio_core::load_chart(&saved).unwrap(),
        Some(saved),
        None,
    );
    settle(&mut app);
    click_panel(&mut app, "Remove project tag");
    assert!(
        app.editor.document().as_json()[PLUGIN_DATA_KEY]
            .get("example.project-data")
            .is_none()
    );
    assert_eq!(
        app.editor.document().as_json()[PLUGIN_DATA_KEY]["future.plugin"],
        value[PLUGIN_DATA_KEY]["future.plugin"]
    );
    app.plugin_action(Action::Disable("example.project-data".into()));
    settle(&mut app);
    assert!(app.editor.undo());
    assert_eq!(app.editor.document().as_json(), &changed);
    app.plugins.shutdown().unwrap();
}
