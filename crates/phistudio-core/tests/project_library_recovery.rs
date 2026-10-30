use phistudio_core::{
    ChartDocument, Command, Editor, ProjectLocation, RecoverySnapshot, clear_recovery,
    create_project, list_projects, load_chart, load_recovery, open_project, remember_project,
    save_chart, save_recovery,
};

#[test]
fn existing_imports_are_discovered_and_a_selected_chart_keeps_its_resource_root() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("projects/old-import");
    let assets = workspace.join("wrapped");
    std::fs::create_dir_all(&assets).unwrap();
    let chart = assets.join("hard.json");
    save_chart(&ChartDocument::empty("Hard", 150.).unwrap(), &chart).unwrap();
    save_chart(
        &ChartDocument::empty("Easy", 100.).unwrap(),
        &assets.join("easy.json"),
    )
    .unwrap();
    std::fs::write(assets.join("music.ogg"), b"untouched audio").unwrap();
    let library = list_projects(root.path()).unwrap();
    assert_eq!(library.projects.len(), 1);
    assert!(library.issues.is_empty());
    let discovered = open_project(root.path(), &workspace).unwrap();
    assert_eq!(discovered.charts.len(), 2);
    assert_eq!(discovered.suggested, None);
    remember_project(
        root.path(),
        ProjectLocation {
            chart: chart.clone(),
            assets: Some(assets.clone()),
        },
    )
    .unwrap();
    let reopened = open_project(root.path(), &workspace).unwrap();
    assert_eq!(
        reopened.charts[reopened.suggested.unwrap()].path,
        chart.canonicalize().unwrap()
    );
    assert_eq!(reopened.assets, assets.canonicalize().unwrap());
    let library = list_projects(root.path()).unwrap();
    assert_eq!(library.projects[0].name, "Hard");
    assert!(library.projects[0].last_opened_ms > 0);
    assert_eq!(
        std::fs::read(assets.join("music.ogg")).unwrap(),
        b"untouched audio"
    );
}

#[test]
fn damaged_records_can_be_rediscovered_but_external_workspaces_and_links_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let location =
        create_project(root.path(), &ChartDocument::empty("Private", 120.).unwrap()).unwrap();
    remember_project(root.path(), location.clone()).unwrap();
    let workspace = location.assets.unwrap();
    let record = std::fs::read_dir(root.path().join("library"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    metadata["chart"] = serde_json::json!("../../outside.json");
    std::fs::write(record, serde_json::to_vec(&metadata).unwrap()).unwrap();
    let library = list_projects(root.path()).unwrap();
    assert_eq!(library.projects.len(), 1);
    assert_eq!(library.issues.len(), 1);
    assert!(open_project(root.path(), &workspace).is_ok());
    assert!(open_project(root.path(), outside.path()).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path(), workspace.join("outside-link")).unwrap();
        assert!(open_project(root.path(), &workspace).is_err());
    }
}

#[test]
fn recovery_preserves_edits_resources_and_dirty_state_without_overwriting_the_saved_chart() {
    let root = tempfile::tempdir().unwrap();
    let mut value = ChartDocument::empty("Saved", 120.)
        .unwrap()
        .as_json()
        .clone();
    value["futureExtension"] = serde_json::json!({"keep": [null, true, 17]});
    let saved = ChartDocument::from_value(value).unwrap();
    let location = create_project(root.path(), &saved).unwrap();
    let mut editor = Editor::new(saved.clone());
    editor
        .execute(Command::RenameChart("Recovered edit".into()))
        .unwrap();
    save_recovery(
        root.path(),
        &RecoverySnapshot {
            document: editor.document().clone(),
            location: Some(location.clone()),
            beat: 12.25,
            line: 0,
        },
    )
    .unwrap();
    assert_eq!(
        load_chart(&location.chart).unwrap().as_json(),
        saved.as_json()
    );
    let snapshot = load_recovery(root.path()).unwrap().unwrap();
    assert_eq!(snapshot.document.as_json(), editor.document().as_json());
    assert_eq!(snapshot.location, Some(location.clone()));
    assert_eq!(snapshot.beat, 12.25);
    let mut recovered = Editor::recovered(snapshot.document);
    assert!(recovered.is_dirty());
    assert!(!recovered.can_undo());
    recovered
        .execute(Command::RenameChart("Another edit".into()))
        .unwrap();
    recovered.undo();
    assert!(recovered.is_dirty());
    save_chart(recovered.document(), &location.chart).unwrap();
    recovered.mark_saved();
    assert!(!recovered.is_dirty());
    assert!(load_recovery(root.path()).unwrap().is_none());
    clear_recovery(root.path()).unwrap();
    assert!(!root.path().join("recovery/session.json").exists());
}

#[test]
fn untitled_recovery_is_supported_and_a_failed_checkpoint_keeps_the_previous_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let document = ChartDocument::empty("Untitled edit", 120.).unwrap();
    let mut snapshot = RecoverySnapshot {
        document,
        location: None,
        beat: 3.5,
        line: 0,
    };
    save_recovery(root.path(), &snapshot).unwrap();
    snapshot.beat = f64::NAN;
    assert!(save_recovery(root.path(), &snapshot).is_err());
    assert_eq!(load_recovery(root.path()).unwrap().unwrap().beat, 3.5);
    snapshot.beat = 0.;
    let outside = tempfile::tempdir().unwrap();
    let chart = outside.path().join("chart.json");
    save_chart(&snapshot.document, &chart).unwrap();
    snapshot.location = Some(ProjectLocation {
        chart,
        assets: None,
    });
    assert!(save_recovery(root.path(), &snapshot).is_err());
    assert_eq!(
        load_recovery(root.path()).unwrap().unwrap().document.name(),
        "Untitled edit"
    );
    let path = root.path().join("recovery/session.json");
    let mut envelope: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    envelope["location"] = serde_json::to_value(snapshot.location).unwrap();
    std::fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    assert!(load_recovery(root.path()).is_err());
}

#[test]
fn library_covers_resolve_manifest_assets_for_existing_imports_and_cached_records() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("projects/wrapped-import");
    let assets = workspace.join("wrapped");
    std::fs::create_dir_all(assets.join("art")).unwrap();
    let chart = assets.join("hard.json");
    save_chart(&ChartDocument::empty("Cover test", 150.).unwrap(), &chart).unwrap();
    std::fs::write(assets.join("art/Background.PNG"), b"preserved illustration").unwrap();
    std::fs::write(assets.join("art/unrelated.png"), b"other image").unwrap();
    std::fs::write(
        assets.join("info.yml"),
        b"chart: hard.json\nillustration: art/Background.PNG\n",
    )
    .unwrap();
    let expected = Some(assets.join("art/Background.PNG").canonicalize().unwrap());
    assert_eq!(
        list_projects(root.path()).unwrap().projects[0].cover,
        expected
    );
    remember_project(
        root.path(),
        ProjectLocation {
            chart,
            assets: Some(assets.clone()),
        },
    )
    .unwrap();
    assert_eq!(
        list_projects(root.path()).unwrap().projects[0].cover,
        expected
    );
    let record = std::fs::read_dir(root.path().join("library"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    // Records from earlier APKs have no cover field; they must discover it too.
    metadata.as_object_mut().unwrap().remove("cover");
    std::fs::write(&record, serde_json::to_vec(&metadata).unwrap()).unwrap();
    assert_eq!(
        list_projects(root.path()).unwrap().projects[0].cover,
        expected
    );
    metadata["cover"] = serde_json::json!("../../outside.png");
    std::fs::write(&record, serde_json::to_vec(&metadata).unwrap()).unwrap();
    assert_eq!(
        list_projects(root.path()).unwrap().projects[0].cover,
        expected
    );
    std::fs::remove_file(assets.join("art/Background.PNG")).unwrap();
    std::fs::remove_file(assets.join("art/unrelated.png")).unwrap();
    assert!(
        list_projects(root.path()).unwrap().projects[0]
            .cover
            .is_none()
    );
}
