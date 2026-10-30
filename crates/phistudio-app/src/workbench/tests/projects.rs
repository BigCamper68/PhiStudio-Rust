use super::*;

#[test]
fn note_viewport_index_reuses_cursor_frames_and_tracks_edit_undo_and_document_changes() {
    let mut app = Workbench::new(ChartDocument::empty("Indexed notes", 120.).unwrap(), None);
    app.execute(Command::AddNote {
        line: 0,
        note: Note {
            start_time: Beat::new(2, 0, 1).unwrap(),
            end_time: Beat::new(2, 0, 1).unwrap(),
            ..Note::default()
        },
    });
    let cached = app.notes();
    assert_eq!(app.note_candidates(1., 3.), vec![0]);
    app.beat = 500.;
    assert!(app.note_candidates(499., 501.).is_empty());
    assert!(std::sync::Arc::ptr_eq(&cached, &app.notes()));
    app.execute(Command::AddNote {
        line: 0,
        note: Note {
            start_time: Beat::new(500, 0, 1).unwrap(),
            end_time: Beat::new(500, 0, 1).unwrap(),
            ..Note::default()
        },
    });
    assert_eq!(app.note_candidates(499., 501.), vec![1]);
    app.history_step(false);
    assert!(app.note_candidates(499., 501.).is_empty());
    app.history_step(true);
    assert_eq!(app.note_candidates(499., 501.), vec![1]);
    app.replace_document(ChartDocument::empty("Empty", 120.).unwrap(), None, None);
    assert!(app.note_candidates(-1., 1000.).is_empty());
}

#[test]
fn undo_active_line_creation_returns_to_an_existing_line() {
    let mut app = Workbench::new(ChartDocument::empty("Test", 120.).unwrap(), None);
    app.execute(Command::AddLine { name: "New".into() });
    app.line = 1;
    frame(
        &mut app,
        &egui::Context::default(),
        Vec2::new(1280., 800.),
        shortcut(
            egui::Key::Z,
            egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
        ),
        false,
    );
    assert_eq!(app.editor.document().line_count(), 1);
    assert_eq!(app.line, 0);
}

#[test]
fn open_shortcut_is_for_packages_and_save_uses_the_project_library() {
    let mut app = Workbench::new(
        ChartDocument::empty("Package workflow", 120.).unwrap(),
        None,
    );
    app.managed_storage = true;
    frame(
        &mut app,
        &egui::Context::default(),
        Vec2::new(873., 393.),
        shortcut(egui::Key::O, egui::Modifiers::COMMAND),
        false,
    );
    assert!(app.package_dialog);
    assert!(app.path_input.is_empty());
    app.package_dialog = false;
    app.save_action();
    assert!(matches!(
        app.native_request,
        Some(crate::files::FileRequest::SaveWorkspace)
    ));
    assert!(!app.package_dialog);
}

#[test]
fn recovery_prompt_blocks_editing_and_restores_an_unsaved_document_with_its_location() {
    let mut app = Workbench::new(ChartDocument::empty("Saved", 120.).unwrap(), None);
    let mut recovered = Editor::new(ChartDocument::empty("Recovered", 150.).unwrap());
    recovered
        .execute(Command::AddNote {
            line: 0,
            note: Note::default(),
        })
        .unwrap();
    let expected = recovered.document().as_json().clone();
    let location = phistudio_core::ProjectLocation {
        chart: PathBuf::from("/private/projects/example/chart.json"),
        assets: Some(PathBuf::from("/private/projects/example/assets")),
    };
    app.managed_storage = true;
    app.tool = Tool::Place(NoteKind::Tap);
    app.recovery = Some(phistudio_core::RecoverySnapshot {
        document: recovered.document().clone(),
        location: Some(location.clone()),
        beat: 12.25,
        line: 0,
    });
    let ctx = egui::Context::default();
    let size = Vec2::new(873., 393.);
    frame(&mut app, &ctx, size, vec![], false);
    frame(
        &mut app,
        &ctx,
        size,
        shortcut(egui::Key::Space, egui::Modifiers::NONE),
        false,
    );
    let pos = app.grid_area.left_top() + egui::vec2(50., 50.);
    frame(
        &mut app,
        &ctx,
        size,
        vec![pointer(pos, true), pointer(pos, false)],
        false,
    );
    assert_eq!(app.editor.document().note_count(), 0);
    assert!(!app.playing);
    app.restore_recovery();
    assert!(app.recovery.is_none());
    assert_eq!(app.editor.document().as_json(), &expected);
    assert!(app.editor.is_dirty());
    assert!(!app.editor.can_undo());
    assert_eq!(app.file, Some(location.chart));
    assert_eq!(app.asset_root(), location.assets.as_deref());
    assert_eq!(app.beat, 12.25);
    assert!(app.managed_storage);
}

#[test]
fn a_project_browser_blocks_shortcuts_without_discarding_the_current_history() {
    let mut app = Workbench::new(ChartDocument::empty("Current", 120.).unwrap(), None);
    app.execute(Command::AddNote {
        line: 0,
        note: Note::default(),
    });
    app.library = Some(phistudio_core::ProjectLibrary::default());
    app.show_projects = true;
    let ctx = egui::Context::default();
    frame(
        &mut app,
        &ctx,
        Vec2::new(600., 320.),
        shortcut(egui::Key::Z, egui::Modifiers::COMMAND),
        false,
    );
    assert_eq!(app.editor.document().note_count(), 1);
    assert!(app.editor.is_dirty());
    app.show_projects = false;
    frame(
        &mut app,
        &ctx,
        Vec2::new(600., 320.),
        shortcut(egui::Key::Z, egui::Modifiers::COMMAND),
        false,
    );
    assert_eq!(app.editor.document().note_count(), 0);
}

#[test]
fn saving_a_managed_project_keeps_its_private_location() {
    let root = tempfile::tempdir().unwrap();
    let saved = ChartDocument::empty("Saved project", 120.).unwrap();
    let location = phistudio_core::create_project(root.path(), &saved).unwrap();
    let mut app = Workbench::new(saved, Some(location.chart.clone()));
    app.asset_directory = location.assets.clone();
    app.managed_storage = true;
    app.execute(Command::AddNote {
        line: 0,
        note: Note::default(),
    });
    app.save_action();
    assert_eq!(app.file, Some(location.chart.clone()));
    assert_eq!(app.asset_root(), location.assets.as_deref());
    assert!(!app.editor.is_dirty());
    assert_eq!(
        load_chart(&location.chart).unwrap().as_json(),
        app.editor.document().as_json()
    );
}

#[test]
fn opening_another_document_keeps_grid_layout_and_audio_preferences() {
    let root = tempfile::tempdir().unwrap();
    let mut app = Workbench::new(ChartDocument::empty("First", 120.).unwrap(), None);
    app.lanes = 21;
    app.division = 12;
    app.zoom = 140.;
    app.music_volume = 0.3;
    app.hitsound_volume = 0.6;
    app.show_context = false;
    let expected = app.preferences();
    let path = root.path().join("second.json");
    save_chart(&ChartDocument::empty("Second", 170.).unwrap(), &path).unwrap();
    app.open(path);
    assert_eq!(app.preferences(), expected);
}

#[test]
fn back_closes_advanced_forms_and_shortcut_capture_before_requesting_exit() {
    let mut app = Workbench::new(ChartDocument::empty("Back", 120.).unwrap(), None);
    let raw = app.editor.document().as_json().clone();
    app.open_chart_properties();
    app.back();
    assert!(app.advanced.metadata.is_none());
    app.advanced.history = true;
    app.back();
    assert!(!app.advanced.history);
    app.advanced.shortcuts_window = true;
    app.advanced.shortcut_capture = Some("Space".into());
    app.back();
    assert!(app.advanced.shortcut_capture.is_none());
    assert!(app.advanced.shortcuts_window);
    app.back();
    assert!(!app.advanced.shortcuts_window);
    assert!(!app.close_requested);
    assert_eq!(app.editor.document().as_json(), &raw);
}

#[test]
fn skin_library_and_selected_skin_survive_switching_chart_documents() {
    let mut app = Workbench::new(ChartDocument::empty("First", 120.).unwrap(), None);
    app.options.skin = Some("skin-fixture".into());
    app.skins.loaded = true;
    app.skins.root = Some("/private/skins".into());
    app.skins.entries.push(phistudio_core::skins::SkinEntry {
        id: "skin-fixture".into(),
        name: "Custom".into(),
        author: "Author".into(),
        description: String::new(),
        assets: "/private/skins/skin-fixture/assets".into(),
    });
    app.options.theme.colors[6] = [200, 80, 240, 255];
    app.replace_document(ChartDocument::empty("Second", 150.).unwrap(), None, None);
    assert!(app.skins.loaded);
    assert_eq!(app.skins.entries[0].id, "skin-fixture");
    assert_eq!(app.options.skin.as_deref(), Some("skin-fixture"));
    assert_eq!(app.options.theme.colors[6], [200, 80, 240, 255]);
}
