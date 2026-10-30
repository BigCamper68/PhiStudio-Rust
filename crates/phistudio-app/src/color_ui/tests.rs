use super::*;
use phistudio_core::{Beat, ChartDocument, Command, StoryEvent, StoryKind};
fn frame(app: &mut Workbench, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(873., 393.))),
            events,
            ..Default::default()
        },
        |ctx| app.color_window(ctx),
    )
}
#[test]
fn palette_edits_a_story_draft_until_apply_and_theme_cancel_restores_all_colors() {
    let mut app = Workbench::new(ChartDocument::empty("Palette", 120.).unwrap(), None);
    app.commit_authoring(Command::EditStoryboard {
        line: 0,
        kind: StoryKind::Color,
        index: None,
        event: Some(StoryEvent::new(StoryKind::Color, Beat::ZERO, Beat::ONE)),
    });
    let id = StoryId {
        line: 0,
        kind: StoryKind::Color,
        index: 0,
    };
    app.select_story(Some(id));
    let original = app.editor.document().as_json().clone();
    app.open_story_color(id, false, [255, 255, 255]);
    app.palette.as_mut().unwrap().color = Hsva {
        h: 1. / 3.,
        s: 1.,
        v: 1.,
        a: 1.,
    };
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    let mut apply = None;
    for _ in 0..3 {
        let out = frame(&mut app, &ctx, vec![]);
        for shape in out.shapes {
            if let egui::Shape::Text(t) = shape.1
                && t.galley.job.text == "Apply color"
            {
                apply = Some(t.pos + t.galley.size() * 0.5);
            }
        }
    }
    let pos = apply.unwrap();
    assert!(pos.y < 393., "Palette actions must stay visible");
    let events = [true, false]
        .into_iter()
        .map(|pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        })
        .collect();
    frame(&mut app, &ctx, events);
    assert!(app.palette.is_none());
    assert_eq!(app.editor.document().as_json(), &original);
    assert_eq!(
        app.advanced.story.as_ref().unwrap().event.start,
        serde_json::json!([0, 255, 0])
    );
    let old = app.options.theme;
    app.open_color(Target::Theme(None), old.colors[6]);
    app.palette.as_mut().unwrap().color = Hsva {
        h: 0.8,
        s: 0.9,
        v: 0.9,
        a: 1.,
    };
    frame(&mut app, &ctx, vec![]);
    assert_ne!(app.options.theme, old);
    assert!(app.close_color_picker());
    assert_eq!(app.options.theme, old);
    assert_eq!(app.editor.document().as_json(), &original);
    assert_eq!(parse_hex("#3Af"), Some([51, 170, 255]));
    assert_eq!(parse_hex("#0044FF"), Some([0, 68, 255]));
    assert!(parse_hex("#xxz").is_none());
}

#[test]
fn note_effect_palette_and_volume_reset_preserve_the_selected_note_and_undo() {
    use phistudio_core::{Note, NoteId, NoteSoundGain};
    let mut app = Workbench::new(ChartDocument::empty("Note effects", 120.).unwrap(), None);
    app.execute(Command::AddNote {
        line: 0,
        note: Note::default(),
    });
    let id = NoteId { line: 0, index: 0 };
    app.select(Some(id));
    app.execute(Command::SetNoteVolume {
        id,
        gain: Some(NoteSoundGain {
            source: None,
            rendered: "quiet.wav".into(),
            volume: 0.25,
        }),
    });
    app.set_note_volume(id, 1.);
    assert_eq!(app.selected, Some(id));
    assert!(app.editor.document().note_sound_gain(id).is_none());
    let before = app.editor.document().as_json().clone();
    app.open_note_color(id, [255, 255, 255]);
    app.palette.as_mut().unwrap().color = Hsva {
        h: 1. / 3.,
        s: 1.,
        v: 1.,
        a: 1.,
    };
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    let mut apply = None;
    for _ in 0..3 {
        for shape in frame(&mut app, &ctx, vec![]).shapes {
            if let egui::Shape::Text(t) = shape.1
                && t.galley.job.text == "Apply color"
            {
                apply = Some(t.pos + t.galley.size() * 0.5);
            }
        }
    }
    let pos = apply.unwrap();
    frame(
        &mut app,
        &ctx,
        [true, false]
            .into_iter()
            .map(|pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            })
            .collect(),
    );
    assert!(app.palette.is_none());
    assert_eq!(app.selected, Some(id));
    assert_eq!(
        app.editor.document().note(0, 0).unwrap().tint_hit_effects,
        Some([0, 255, 0])
    );
    app.editor.undo();
    assert_eq!(app.editor.document().as_json(), &before);
}
