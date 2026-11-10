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
    assert_eq!(dominant(&mut app), Some(Event::Place));
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
    options.events.entry(Event::Place).or_default().file = Some("/missing/editor-sound.wav".into());
    let prepared = prepare(Sources::new(&options));
    assert!(prepared.warning.unwrap().contains("Place objects"));
    assert_eq!(
        prepared.clips.len(),
        Event::ALL.len() + phistudio_core::EditorSoundTone::ALL.len()
    );
    assert!(prepared.clips[Event::Place as usize].length() > 0.);
}
#[test]
fn sound_customization_does_not_change_chart_history_and_survives_project_switches() {
    let mut app = app();
    let chart = app.editor.document().as_json().clone();
    app.options.editor_sounds.volume = 0.7;
    app.options
        .editor_sounds
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
