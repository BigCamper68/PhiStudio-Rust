use phistudio_core::{
    ChartDocument, Command, Editor, ProjectLocation, last_project, remember_project, save_chart,
};

#[test]
fn restarting_reopens_the_saved_working_copy_with_its_resources() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("projects/example");
    std::fs::create_dir_all(&project).unwrap();
    let chart = project.join("chart.json");
    let mut editor = Editor::new(ChartDocument::empty("Imported", 120.).unwrap());
    save_chart(editor.document(), &chart).unwrap();
    remember_project(
        root.path(),
        ProjectLocation {
            chart: chart.clone(),
            assets: Some(project.clone()),
        },
    )
    .unwrap();
    editor
        .execute(Command::RenameChart("Saved edit".into()))
        .unwrap();
    save_chart(editor.document(), &chart).unwrap();
    let (restored, location) = last_project(root.path()).unwrap().unwrap();
    assert_eq!(restored.as_json(), editor.document().as_json());
    assert_eq!(location.chart, chart.canonicalize().unwrap());
    assert_eq!(location.assets, Some(project.canonicalize().unwrap()));
}

#[test]
fn a_corrupt_or_external_session_cannot_redirect_project_loading() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let chart = outside.path().join("chart.json");
    save_chart(&ChartDocument::empty("Outside", 120.).unwrap(), &chart).unwrap();
    assert!(last_project(root.path()).unwrap().is_none());
    assert!(
        remember_project(
            root.path(),
            ProjectLocation {
                chart: chart.clone(),
                assets: None
            }
        )
        .is_err()
    );
    std::fs::write(
        root.path().join("last-project.json"),
        serde_json::to_vec(&ProjectLocation {
            chart,
            assets: None,
        })
        .unwrap(),
    )
    .unwrap();
    assert!(last_project(root.path()).is_err());
    std::fs::write(root.path().join("last-project.json"), "broken").unwrap();
    assert!(last_project(root.path()).is_err());
}
