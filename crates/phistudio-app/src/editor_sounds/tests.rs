mod gestures;
mod manager;
use super::*;
use egui::{WidgetInfo, WidgetType, output::OutputEvent};
use phistudio_core::{Beat, ChartDocument, Note, NoteKind};

fn app() -> Workbench {
    Workbench::new(ChartDocument::empty("Sounds", 120.).unwrap(), None)
}
fn dominant(app: &mut Workbench) -> Option<Event> {
    let result = app
        .sounds
        .pending
        .iter()
        .copied()
        .max_by_key(|e| priority(*e));
    app.sounds.pending.clear();
    result
}
#[test]
fn widget_feedback_ignores_focus_text_and_disabled_controls() {
    let button = WidgetInfo::new(WidgetType::Button);
    assert_eq!(
        widget_event(&OutputEvent::Clicked(button.clone())),
        Some(Event::Click)
    );
    assert_eq!(
        widget_event(&OutputEvent::FocusGained(button.clone())),
        None
    );
    let mut disabled = button;
    disabled.enabled = false;
    assert_eq!(widget_event(&OutputEvent::Clicked(disabled)), None);
    assert_eq!(
        widget_event(&OutputEvent::ValueChanged(WidgetInfo::new(
            WidgetType::TextEdit
        ))),
        None
    );
    assert_eq!(
        widget_event(&OutputEvent::Clicked(WidgetInfo::new(WidgetType::Slider))),
        None
    );
    assert_eq!(
        widget_event(&OutputEvent::ValueChanged(WidgetInfo::new(
            WidgetType::Slider
        ))),
        Some(Event::Value)
    );
}
#[test]
fn semantic_actions_emit_only_after_success_and_win_over_button_clicks() {
    let mut app = app();
    app.history_step_now(false);
    assert_eq!(dominant(&mut app), None);
    app.execute(Command::AddNote {
        line: 0,
        note: Note {
            kind: NoteKind::Tap,
            start_time: Beat::ZERO,
            end_time: Beat::ZERO,
            x: 0.,
            ..Default::default()
        },
    });
    app.sounds.emit(Event::Click);
    assert_eq!(dominant(&mut app), Some(Event::PlaceTap));
    app.history_step_now(false);
    assert_eq!(dominant(&mut app), Some(Event::Undo));
    app.history_step_now(true);
    assert_eq!(dominant(&mut app), Some(Event::Redo));
    let revision = app.editor.revision();
    app.execute(Command::DeleteNote(phistudio_core::NoteId {
        line: 0,
        index: 999,
    }));
    assert_eq!(app.editor.revision(), revision);
    assert_eq!(dominant(&mut app), None);
}
#[test]
fn real_button_release_emits_feedback_and_cancel_drops_pending_actions() {
    let ctx = egui::Context::default();
    let mut pos = egui::Pos2::ZERO;
    let render = |ctx: &egui::Context| {
        egui::CentralPanel::default()
            .show(ctx, |ui| ui.button("Button"))
            .inner
            .rect
            .center()
    };
    let _ = ctx.run(Default::default(), |ctx| pos = render(ctx));
    let events = [true, false]
        .into_iter()
        .map(|pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        })
        .collect();
    let output = ctx.run(
        egui::RawInput {
            events,
            ..Default::default()
        },
        |ctx| {
            render(ctx);
        },
    );
    assert!(
        output
            .platform_output
            .events
            .iter()
            .any(|e| widget_event(e) == Some(Event::Click))
    );
    let mut app = app();
    app.sounds.emit(Event::Place);
    let _ = ctx.run(Default::default(), |ctx| app.sound_ui(ctx, true));
    assert_eq!(dominant(&mut app), None);
}
#[test]
fn missing_custom_sound_falls_back_without_blocking_other_events() {
    let mut options = EditorSoundOptions::default();
    options.add_pack(phistudio_core::EditorSoundPack::default());
    options
        .selected_pack_mut()
        .unwrap()
        .events
        .entry(Event::Place)
        .or_default()
        .files = vec!["/missing/editor-sound.wav".into()];
    let prepared = prepare(Sources::new(&options));
    assert!(prepared.warning.unwrap().contains("editor-sound.wav"));
    assert_eq!(prepared.clips.len(), Event::ALL.len());
    assert!(prepared.clips[Event::Place as usize][0].is_none());
    assert!(prepared.bank[Event::Place.default_tone() as usize].length() > 0.);
}
#[test]
fn sound_customization_does_not_change_chart_history_and_survives_project_switches() {
    let mut app = app();
    let chart = app.editor.document().as_json().clone();
    app.options.editor_sounds.volume = 0.7;
    app.options
        .editor_sounds
        .add_pack(phistudio_core::EditorSoundPack::default());
    app.options
        .editor_sounds
        .selected_pack_mut()
        .unwrap()
        .events
        .entry(Event::Select)
        .or_default()
        .enabled = false;
    let preferences = app.preferences();
    assert!(!app.editor.is_dirty());
    assert!(!app.editor.can_undo());
    assert_eq!(app.editor.document().as_json(), &chart);
    app.replace_document(ChartDocument::empty("Next", 140.).unwrap(), None, None);
    assert_eq!(app.preferences(), preferences);
}

#[test]
fn gesture_sound_is_independent_and_custom_audio_uses_the_correct_bank_slot() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("move.wav");
    std::fs::write(&file, crate::media::tests::wav_bytes()).unwrap();
    let mut options = EditorSoundOptions::default();
    options.add_pack(phistudio_core::EditorSoundPack::default());
    options
        .selected_pack_mut()
        .unwrap()
        .events
        .entry(Event::MoveResize)
        .or_default()
        .files = vec![file.to_string_lossy().into_owned()];
    let prepared = prepare(Sources::new(&options));
    assert!(prepared.warning.is_none());
    assert!(
        (prepared.clips[Event::MoveResize as usize][0]
            .as_ref()
            .unwrap()
            .length()
            - 0.01)
            .abs()
            < 1e-6
    );
    assert!((prepared.bank[Event::Edit.default_tone() as usize].length() - 0.045).abs() < 1e-6);
    let mut app = app();
    app.options
        .editor_sounds
        .add_pack(phistudio_core::EditorSoundPack::default());
    app.options
        .editor_sounds
        .selected_pack_mut()
        .unwrap()
        .events
        .entry(Event::MoveResize)
        .or_default()
        .enabled = false;
    app.sounds.emit(Event::MoveResize);
    app.sounds.emit(Event::Edit);
    app.sounds.emit(Event::Select);
    assert_eq!(dominant(&mut app), Some(Event::MoveResize));
    assert!(app.options.editor_sounds.rule(Event::Edit).enabled);
    assert_eq!(
        Event::MoveResize.default_tone(),
        phistudio_core::EditorSoundTone::Slide
    );
}

#[test]
fn variants_never_repeat_immediately_and_sequential_mode_is_predictable() {
    let mut random = 42;
    for count in 2..=4 {
        let mut last = None;
        let mut seen = BTreeSet::new();
        for _ in 0..100 {
            let next = choose_variant(count, last, true, &mut random).unwrap();
            assert!(next < count);
            assert_ne!(Some(next), last);
            seen.insert(next);
            last = Some(next);
        }
        assert_eq!(seen.len(), count);
    }
    assert_eq!(choose_variant(0, None, true, &mut random), None);
    assert_eq!(choose_variant(1, Some(0), true, &mut random), Some(0));
    assert_eq!(
        (0..4)
            .scan(None, |last, _| {
                *last = choose_variant(3, *last, false, &mut random);
                Some(last.unwrap())
            })
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 0]
    );
}
#[test]
fn manager_back_does_not_close_an_underlying_file_picker_and_preserves_custom_packs() {
    let mut app = app();
    app.options
        .editor_sounds
        .add_pack(phistudio_core::EditorSoundPack::default());
    app.sounds.open = true;
    app.files.media = Some(crate::media::MediaKind::SoundPack);
    app.back();
    assert!(app.files.media.is_none());
    assert!(app.sounds.open);
    app.back();
    assert!(!app.sounds.open);
    assert_eq!(app.options.editor_sounds.packs.len(), 1);
}

#[test]
fn pending_audio_import_keeps_its_pack_target_and_inherited_action_settings() {
    let mut app = app();
    app.options
        .editor_sounds
        .add_pack(phistudio_core::EditorSoundPack::default());
    let original = app.options.editor_sounds.selected_pack.clone();
    app.options
        .editor_sounds
        .selected_pack_mut()
        .unwrap()
        .events
        .entry(Event::Place)
        .or_default()
        .volume = 0.6;
    app.sounds.import_target = Some(original.clone());
    app.options
        .editor_sounds
        .add_pack(phistudio_core::EditorSoundPack::default());
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("tap.wav");
    std::fs::write(&file, crate::media::tests::wav_bytes()).unwrap();
    app.accept_media(
        crate::media::MediaKind::EditorSound(Event::PlaceTap),
        crate::media::MediaFile::new(file.clone(), "tap.wav".into(), false),
    );
    assert!(
        app.options
            .editor_sounds
            .rule(Event::PlaceTap)
            .files
            .is_empty()
    );
    let imported = app
        .options
        .editor_sounds
        .packs
        .iter()
        .find(|p| p.id == original)
        .unwrap()
        .rule(Event::PlaceTap);
    assert_eq!(imported.volume, 0.6);
    assert_eq!(imported.files, vec![file.to_string_lossy().into_owned()]);
}
