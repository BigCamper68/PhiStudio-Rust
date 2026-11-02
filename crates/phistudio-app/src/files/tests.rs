use super::*;
#[test]
fn closing_a_loading_resource_query_leaves_editor_unblocked_and_ignores_late_results() {
    let mut app = Workbench::new(
        phistudio_core::ChartDocument::empty("Query", 120.).unwrap(),
        None,
    );
    let mut files = FileManager::new(None);
    let (sender, result) = mpsc::channel();
    app.workspace.view = Some(crate::workspace_ui::View::Resources);
    app.workspace.loading = true;
    files.query = Some(WorkspaceQuery {
        session: app.session,
        view: app.workspace.view.clone(),
        result,
    });
    files.update(&mut app);
    assert!(app.modal_open() && !app.files.busy);
    app.back();
    files.update(&mut app);
    assert!(!app.modal_open() && !app.files.busy && files.query.is_some());
    sender
        .send(Ok(crate::workspace_ui::WorkspaceResult::Resources {
            entries: vec![phistudio_core::resources::Resource {
                name: "late.png".into(),
                kind: phistudio_core::resources::ResourceKind::Image,
                bytes: 1,
                references: 0,
            }],
            undo: None,
            changed: false,
        }))
        .unwrap();
    files.update(&mut app);
    assert!(!app.modal_open() && !app.files.busy && files.query.is_none());
    assert!(!app.workspace.loading && app.workspace.resources.is_empty());
}
fn media_fixture(root: &std::path::Path) -> phistudio_core::NewProjectMedia {
    let audio = root.join("audio.ogg");
    let illustration = root.join("art.png");
    std::fs::write(&audio, b"fixture audio").unwrap();
    std::fs::write(&illustration, b"fixture image").unwrap();
    phistudio_core::NewProjectMedia {
        audio,
        audio_name: "audio.ogg".into(),
        illustration,
        illustration_name: "art.png".into(),
    }
}
#[test]
fn package_import_rejects_a_loose_json_file_without_changing_the_project() {
    let root = tempfile::tempdir().unwrap();
    let chart = phistudio_core::ChartDocument::empty("Loose chart", 120.).unwrap();
    let input = root.path().join("loose.json");
    let bytes = chart.to_json().unwrap();
    std::fs::write(&input, &bytes).unwrap();
    let mut app = Workbench::new(
        phistudio_core::ChartDocument::empty("Current", 150.).unwrap(),
        None,
    );
    let original = app.editor.document().as_json().clone();
    let mut files = FileManager::new(Some(root.path().to_owned()));
    files.request(
        &mut app,
        FileRequest::ImportPath {
            path: input.clone(),
            name: "loose.json".into(),
        },
    );
    let result = files
        .worker
        .take()
        .unwrap()
        .recv_timeout(std::time::Duration::from_secs(3))
        .expect("Archive validation result");
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(input).unwrap(), bytes);
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(
        phistudio_core::list_projects(root.path())
            .unwrap()
            .projects
            .is_empty()
    );
}
#[test]
fn export_captures_unsaved_edits_without_saving_the_working_copy() {
    let root = tempfile::tempdir().unwrap();
    let document = phistudio_core::ChartDocument::empty("Saved", 120.).unwrap();
    let location = phistudio_core::create_project(root.path(), &document).unwrap();
    let mut app = Workbench::new(document, Some(location.chart.clone()));
    app.execute(phistudio_core::Command::AddNote {
        line: 0,
        note: phistudio_core::Note::default(),
    });
    let mut files = FileManager::new(Some(root.path().to_owned()));
    files.request(&mut app, FileRequest::ExportCurrent);
    let result = files
        .worker
        .take()
        .unwrap()
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap()
        .unwrap();
    let WorkerResult::Exported(package) = result else {
        panic!("Expected package snapshot");
    };
    let imported = phistudio_core::import_project(
        package.path(),
        &root.path().join("roundtrip"),
        &package.name,
        true,
    )
    .unwrap();
    let chart = &imported.charts[imported.suggested.unwrap()].path;
    assert_eq!(phistudio_core::load_chart(chart).unwrap().note_count(), 1);
    assert_eq!(
        phistudio_core::load_chart(&location.chart)
            .unwrap()
            .note_count(),
        0
    );
    assert!(app.editor.is_dirty());
    assert!(app.editor.can_undo());
}
#[test]
fn creating_another_chart_retains_dirty_history_until_the_switch_is_resolved() {
    let root = tempfile::tempdir().unwrap();
    let mut app = Workbench::new(
        phistudio_core::ChartDocument::empty("Current", 120.).unwrap(),
        None,
    );
    app.execute(phistudio_core::Command::AddNote {
        line: 0,
        note: phistudio_core::Note::default(),
    });
    app.show_projects = true;
    let mut files = FileManager::new(Some(root.path().to_owned()));
    files.request(
        &mut app,
        FileRequest::CreateProject {
            document: phistudio_core::ChartDocument::empty("New", 170.).unwrap(),
            media: media_fixture(root.path()),
        },
    );
    let result = files
        .worker
        .take()
        .unwrap()
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap()
        .unwrap();
    let WorkerResult::Created(location) = result else {
        panic!("Expected new project");
    };
    app.open_created_project(location.clone());
    assert!(!app.show_projects);
    assert_eq!(app.files.pending_open, Some(location.chart));
    assert_eq!(app.editor.document().name(), "Current");
    assert_eq!(app.editor.document().note_count(), 1);
    assert!(app.editor.is_dirty());
    assert!(app.editor.can_undo());
}
#[test]
fn deleting_an_active_project_clears_recovery_and_returns_to_projects() {
    let root = tempfile::tempdir().unwrap();
    let document = phistudio_core::ChartDocument::empty("Delete current", 120.).unwrap();
    let location = phistudio_core::create_project(root.path(), &document).unwrap();
    let workspace = location.assets.clone().unwrap();
    let mut app = Workbench::new(document, Some(location.chart.clone()));
    app.managed_storage = true;
    app.execute(phistudio_core::Command::AddNote {
        line: 0,
        note: phistudio_core::Note::default(),
    });
    let mut files = FileManager::new(Some(root.path().to_owned()));
    files.checkpoint(&app, false).unwrap();
    assert!(root.path().join("recovery/session.json").exists());
    files.request(
        &mut app,
        FileRequest::TrashProject {
            workspace: workspace.clone(),
            name: "Delete current".into(),
        },
    );
    let result = files
        .worker
        .take()
        .unwrap()
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap()
        .unwrap();
    let WorkerResult::Deleted { trash, library } = result else {
        panic!("Expected deletion");
    };
    app.project_deleted(trash, library);
    assert!(app.show_projects && app.file.is_none() && !app.editor.is_dirty());
    assert!(app.files.last_deleted.is_some());
    assert!(!workspace.exists());
    assert!(!root.path().join("recovery/session.json").exists());
}
#[test]
fn deleting_another_project_keeps_current_edits_and_recovery() {
    let root = tempfile::tempdir().unwrap();
    let document = phistudio_core::ChartDocument::empty("Current", 120.).unwrap();
    let location = phistudio_core::create_project(root.path(), &document).unwrap();
    let other = phistudio_core::create_project(
        root.path(),
        &phistudio_core::ChartDocument::empty("Other", 130.).unwrap(),
    )
    .unwrap();
    let mut app = Workbench::new(document, Some(location.chart));
    app.execute(phistudio_core::Command::AddNote {
        line: 0,
        note: phistudio_core::Note::default(),
    });
    let mut files = FileManager::new(Some(root.path().to_owned()));
    files.checkpoint(&app, false).unwrap();
    let recovery = root.path().join("recovery/session.json");
    let before = std::fs::read(&recovery).unwrap();
    files.request(
        &mut app,
        FileRequest::TrashProject {
            workspace: other.assets.unwrap(),
            name: "Other".into(),
        },
    );
    let result = files
        .worker
        .take()
        .unwrap()
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap()
        .unwrap();
    let WorkerResult::Deleted { trash, library } = result else {
        panic!("Expected deletion");
    };
    app.project_deleted(trash, library);
    assert!(app.editor.is_dirty());
    assert_eq!(app.editor.document().note_count(), 1);
    assert_eq!(std::fs::read(recovery).unwrap(), before);
}
