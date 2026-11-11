use super::*;
use phistudio_core::{ChartDocument, Command, DefaultNoteSound, EditorPreferences};
fn app() -> Workbench {
    Workbench::new(ChartDocument::empty("Notes", 120.).unwrap(), None)
}
fn place(app: &mut Workbench, kind: NoteKind) {
    app.place_note_with_defaults(0, kind, "4".parse().unwrap(), "6".parse().unwrap(), 125.);
}
#[test]
fn manual_creation_uses_persistent_defaults_with_one_undo() {
    let mut app = app();
    let root = tempfile::tempdir().unwrap();
    let mut preferences = EditorPreferences::default();
    preferences.options.new_note_defaults = NoteCreationDefaults {
        size: 1.5,
        speed: 1.2,
        above: 0,
        is_fake: 1,
        judge_area: Some(2.),
        tint_hit_effects: Some([12, 34, 56]),
        ..Default::default()
    };
    phistudio_core::save_editor_preferences(root.path(), &preferences).unwrap();
    crate::settings::Preferences::new(Some(root.path().to_owned())).update(&mut app);
    let original = app.editor.document().as_json().clone();
    place(&mut app, NoteKind::Hold);
    assert!(app.error.is_none());
    let note = app.editor.document().note(0, 0).unwrap();
    assert_eq!(
        (note.size, note.speed, note.above, note.is_fake),
        (1.5, 1.2, 0, 1)
    );
    assert_eq!(note.judge_area, Some(2.));
    app.history_step_now(false);
    assert_eq!(app.editor.document().as_json(), &original);
}
#[test]
fn native_volume_preparation_is_atomic_cached_and_uses_the_original_source() {
    let mut app = app();
    let assets = tempfile::tempdir().unwrap();
    app.file = Some(assets.path().join("chart.json"));
    app.options.new_note_defaults.volume = 0.6;
    place(&mut app, NoteKind::Tap);
    assert_eq!(app.editor.document().note_count(), 0);
    let Some(FileRequest::PrepareNoteCreation(request)) = app.files.request.take() else {
        panic!("Missing sound preparation")
    };
    let (target, result) = worker::prepare(*request);
    assert!(result.is_ok(), "{:?}", result.as_ref().err());
    app.receive_note_creation(target, result);
    assert!(app.error.is_none());
    assert_eq!(app.editor.document().note_count(), 1);
    let gain = app
        .editor
        .document()
        .note_sound_gain(phistudio_core::NoteId { line: 0, index: 0 })
        .unwrap();
    assert_eq!(gain.volume, 0.6);
    assert_eq!(gain.source, None);
    assert!(assets.path().join(&gain.rendered).is_file());
    place(&mut app, NoteKind::Flick);
    assert!(app.files.request.is_none());
    assert_eq!(app.editor.document().note_count(), 2);
    app.select(Some(phistudio_core::NoteId { line: 0, index: 0 }));
    app.use_selected_note_defaults();
    assert_eq!(app.options.new_note_defaults.volume, 0.6);
    assert_eq!(app.options.new_note_defaults.sound, None);
    app.history_step_now(false);
    assert_eq!(app.editor.document().note_count(), 1);
    app.history_step_now(false);
    assert_eq!(app.editor.document().note_count(), 0);
}
#[test]
fn changed_revision_or_defaults_discard_worker_output() {
    for change_defaults in [false, true] {
        let mut app = app();
        let assets = tempfile::tempdir().unwrap();
        app.file = Some(assets.path().join("chart.json"));
        app.options.new_note_defaults.volume = 0.6;
        place(&mut app, NoteKind::Tap);
        let Some(FileRequest::PrepareNoteCreation(request)) = app.files.request.take() else {
            panic!("Missing request")
        };
        if change_defaults {
            app.options.new_note_defaults.size = 2.;
        } else {
            app.editor
                .execute(Command::RenameChart("Changed".into()))
                .unwrap();
        }
        let before = app.editor.document().as_json().clone();
        let revision = app.editor.revision();
        let (target, result) = worker::prepare(*request);
        app.receive_note_creation(target, result);
        assert_eq!(app.editor.document().as_json(), &before);
        assert_eq!(app.editor.revision(), revision);
    }
}
#[test]
fn copying_selected_custom_sound_keeps_a_stable_project_scope() {
    let mut app = app();
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("projects/project-a");
    app.workspace.root = Some(root.path().to_owned());
    app.file = Some(project.join("generation-one/chart.json"));
    let mut raw = serde_json::json!(Note::default());
    raw["hitsound"] = serde_json::json!("test.wav");
    app.editor
        .execute(Command::InsertNote {
            line: 0,
            value: raw,
        })
        .unwrap();
    app.select(Some(phistudio_core::NoteId { line: 0, index: 0 }));
    app.use_selected_note_defaults();
    assert_eq!(
        app.options.new_note_defaults.sound,
        Some(DefaultNoteSound {
            reference: "test.wav".into(),
            project: Some(project.clone())
        })
    );
    app.file = Some(project.join("generation-two/chart.json"));
    assert_eq!(
        app.options
            .new_note_defaults
            .sound_reference(app.note_default_scope().as_deref()),
        Some("test.wav")
    );
}
#[test]
fn an_old_project_reply_cannot_consume_a_new_projects_pending_creation() {
    let mut app = app();
    let assets = tempfile::tempdir().unwrap();
    app.file = Some(assets.path().join("chart.json"));
    app.options.new_note_defaults.volume = 0.6;
    place(&mut app, NoteKind::Tap);
    let Some(FileRequest::PrepareNoteCreation(old)) = app.files.request.take() else {
        panic!("Missing request")
    };
    app.replace_document(
        ChartDocument::empty("Next", 120.).unwrap(),
        Some(assets.path().join("next.json")),
        Some(assets.path().to_owned()),
    );
    place(&mut app, NoteKind::Hold);
    let Some(FileRequest::PrepareNoteCreation(new)) = app.files.request.take() else {
        panic!("Missing request")
    };
    let (target, result) = worker::prepare(*old);
    app.receive_note_creation(target, result);
    assert!(app.note_creation.pending.is_some());
    assert_eq!(app.editor.document().note_count(), 0);
    let (target, result) = worker::prepare(*new);
    app.receive_note_creation(target, result);
    assert_eq!(app.editor.document().note_count(), 1);
    assert_eq!(
        app.editor.document().note(0, 0).unwrap().kind,
        NoteKind::Hold
    );
}

#[test]
fn cached_or_default_sounds_cannot_bypass_an_in_flight_creation() {
    let mut app = app();
    let assets = tempfile::tempdir().unwrap();
    app.file = Some(assets.path().join("chart.json"));
    app.options.new_note_defaults.volume = 0.6;
    place(&mut app, NoteKind::Tap);
    assert!(app.note_creation.pending.is_some());
    app.options.new_note_defaults.volume = 1.;
    place(&mut app, NoteKind::Hold);
    assert_eq!(app.editor.document().note_count(), 0);
    assert!(app.files.request.is_some());
    assert!(app.error.as_deref().unwrap().contains("Wait"));
}
