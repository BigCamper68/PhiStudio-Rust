use super::*;

#[test]
fn released_modifier_keys_in_one_frame_still_undo_and_redo() {
    let mut app = Workbench::new(ChartDocument::empty("Test", 120.).unwrap(), None);
    app.execute(Command::AddNote {
        line: 0,
        note: Note::default(),
    });
    let ctx = egui::Context::default();
    // RawInput.modifiers is NONE: all key releases have already arrived.
    frame(
        &mut app,
        &ctx,
        Vec2::new(1280., 800.),
        shortcut(
            egui::Key::Z,
            egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
        ),
        false,
    );
    assert_eq!(app.editor.document().note_count(), 0);
    frame(
        &mut app,
        &ctx,
        Vec2::new(1280., 800.),
        shortcut(
            egui::Key::Z,
            egui::Modifiers::CTRL | egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        ),
        false,
    );
    assert_eq!(app.editor.document().note_count(), 1);
}

#[test]
fn clipboard_shortcuts_and_duplicate_keep_one_undo_and_survive_project_changes() {
    let mut app = group_app();
    let ctx = egui::Context::default();
    app.set_selection(phistudio_core::tools::Selection {
        notes: vec![NoteId { line: 0, index: 0 }, NoteId { line: 0, index: 1 }],
        events: vec![],
    });
    let before = app.editor.document().as_json().clone();
    let size = Vec2::new(1280., 800.);
    frame(
        &mut app,
        &ctx,
        size,
        shortcut(egui::Key::C, egui::Modifiers::COMMAND),
        false,
    );
    assert_eq!(app.tools.clipboard.as_ref().unwrap().len(), 2);
    assert_eq!(app.editor.document().as_json(), &before);
    app.beat = 8.;
    frame(
        &mut app,
        &ctx,
        size,
        shortcut(egui::Key::V, egui::Modifiers::COMMAND),
        false,
    );
    assert_eq!(app.editor.document().note_count(), 5);
    assert_eq!(app.selection().len(), 2);
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &before);
    app.replace_document(
        ChartDocument::empty("Other chart", 120.).unwrap(),
        None,
        None,
    );
    assert_eq!(app.tools.clipboard.as_ref().unwrap().len(), 2);
    app.paste_selection(false, false);
    assert_eq!(app.editor.document().note_count(), 2);
}

#[test]
fn batched_select_copy_clear_paste_shortcuts_keep_their_hardware_order() {
    let mut app = group_app();
    let ctx = egui::Context::default();
    app.beat = 8.;
    let command = egui::Modifiers::COMMAND;
    let events = [
        shortcut(egui::Key::A, command),
        shortcut(egui::Key::C, command),
        shortcut(egui::Key::A, command | egui::Modifiers::SHIFT),
        shortcut(egui::Key::V, command),
    ]
    .concat();
    frame(&mut app, &ctx, Vec2::new(1280., 800.), events, false);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.editor.document().note_count(), 6);
    assert_eq!(app.tools.clipboard.as_ref().unwrap().len(), 3);
    assert_eq!(
        app.editor.document().note(0, 3).unwrap().start_time,
        Beat::new(8, 0, 1).unwrap()
    );
    app.history_step(false);
    assert_eq!(app.editor.document().note_count(), 3);
}
