use phistudio_core::*;
use std::{fs, path::Path};
fn media(root: &Path) -> NewProjectMedia {
    let audio = root.join("original audio.OGG");
    let art = root.join("original art.PNG");
    fs::write(&audio, b"original audio contents").unwrap();
    fs::write(&art, b"original illustration contents").unwrap();
    NewProjectMedia {
        audio,
        audio_name: "original audio.OGG".into(),
        illustration: art,
        illustration_name: "original art.PNG".into(),
    }
}
#[test]
fn creation_copies_required_media_and_publishes_a_complete_project() {
    let root = tempfile::tempdir().unwrap();
    let input = tempfile::tempdir().unwrap();
    let source = media(input.path());
    let document = ChartDocument::empty("New chart", 173.).unwrap();
    let location = create_project_with_media(root.path(), &document, &source).unwrap();
    let created = load_chart(&location.chart).unwrap();
    assert_eq!(created.name(), "New chart");
    assert_eq!(created.tempo().bpm_at(0.), 173.);
    let assets = location.assets.as_ref().unwrap();
    assert_eq!(
        fs::read(assets.join("audio.ogg")).unwrap(),
        fs::read(&source.audio).unwrap()
    );
    assert_eq!(
        fs::read(assets.join("illustration.png")).unwrap(),
        fs::read(&source.illustration).unwrap()
    );
    assert_eq!(created.as_json()["META"]["song"], "audio.ogg");
    assert_eq!(created.as_json()["META"]["background"], "illustration.png");
    let options = load_project_options_with_assets(&location.chart, Some(assets)).unwrap();
    assert_eq!(options.music.as_deref(), Some("audio.ogg"));
    assert_eq!(options.illustration.as_deref(), Some("illustration.png"));
    assert_eq!(list_projects(root.path()).unwrap().projects.len(), 1);
    assert_eq!(
        fs::read_dir(root.path().join("staging")).unwrap().count(),
        0
    );
}
#[test]
fn missing_or_empty_media_never_publish_a_partial_chart() {
    let root = tempfile::tempdir().unwrap();
    let input = tempfile::tempdir().unwrap();
    let source = media(input.path());
    let document = ChartDocument::empty("Must be complete", 120.).unwrap();
    fs::remove_file(&source.illustration).unwrap();
    assert!(create_project_with_media(root.path(), &document, &source).is_err());
    fs::write(&source.illustration, b"").unwrap();
    assert!(create_project_with_media(root.path(), &document, &source).is_err());
    assert!(list_projects(root.path()).unwrap().projects.is_empty());
    assert_eq!(fs::read(&source.audio).unwrap(), b"original audio contents");
}
#[test]
fn deleting_and_restoring_retains_saved_chart_assets_and_private_library_record() {
    let root = tempfile::tempdir().unwrap();
    let input = tempfile::tempdir().unwrap();
    let source = media(input.path());
    let location = create_project_with_media(
        root.path(),
        &ChartDocument::empty("Keep this", 140.).unwrap(),
        &source,
    )
    .unwrap();
    remember_project(root.path(), location.clone()).unwrap();
    let original = fs::read(&location.chart).unwrap();
    let workspace = location.assets.as_ref().unwrap();
    let trash = trash_project(root.path(), workspace, "Keep this").unwrap();
    assert!(!workspace.exists());
    assert!(list_projects(root.path()).unwrap().projects.is_empty());
    assert!(last_project(root.path()).unwrap().is_none());
    assert_eq!(
        fs::read(trash.entry.join("project/chart.json")).unwrap(),
        original
    );
    assert_eq!(fs::read(&source.audio).unwrap(), b"original audio contents");
    assert_eq!(restore_project(root.path(), &trash).unwrap(), *workspace);
    assert_eq!(fs::read(&location.chart).unwrap(), original);
    assert_eq!(
        fs::read(workspace.join("illustration.png")).unwrap(),
        b"original illustration contents"
    );
    assert_eq!(
        list_projects(root.path()).unwrap().projects[0].name,
        "Keep this"
    );
}
#[test]
fn deletion_and_undo_reject_external_paths_and_never_replace_an_existing_project() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    assert!(trash_project(root.path(), outside.path(), "Outside").is_err());
    let location =
        create_project(root.path(), &ChartDocument::empty("Saved", 120.).unwrap()).unwrap();
    let workspace = location.assets.unwrap();
    let trash = trash_project(root.path(), &workspace, "Saved").unwrap();
    fs::create_dir(&workspace).unwrap();
    fs::write(workspace.join("keep"), b"new content").unwrap();
    assert!(restore_project(root.path(), &trash).is_err());
    assert_eq!(fs::read(workspace.join("keep")).unwrap(), b"new content");
    assert!(trash.entry.join("project/chart.json").is_file());
    let forged = TrashedProject {
        original: workspace,
        entry: outside.path().to_owned(),
        name: "Outside".into(),
    };
    assert!(restore_project(root.path(), &forged).is_err());
}

#[test]
fn a_long_display_title_does_not_make_undo_unreadable() {
    let root = tempfile::tempdir().unwrap();
    let title = "Chart".repeat(5000);
    let location =
        create_project(root.path(), &ChartDocument::empty(&title, 120.).unwrap()).unwrap();
    let trash = trash_project(root.path(), location.assets.as_deref().unwrap(), &title).unwrap();
    restore_project(root.path(), &trash).unwrap();
    assert_eq!(load_chart(&location.chart).unwrap().name(), title);
}
