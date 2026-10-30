use super::*;
use egui::{Context, Event, Key, Modifiers, RawInput, Rect, pos2, vec2};
fn key(key: Key) -> Event {
    Event::Key {
        key,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}
#[test]
fn note_property_formula_uses_the_existing_sparse_undo_and_redo() {
    use crate::workbench::Workbench;
    use phistudio_core::{ChartDocument, Command, Note, NoteId};
    let mut app = Workbench::new(ChartDocument::empty("Expressions", 120.).unwrap(), None);
    app.execute(Command::AddNote {
        line: 0,
        note: Note::default(),
    });
    app.select(Some(NoteId { line: 0, index: 0 }));
    app.editor.mark_saved();
    let before = app.editor.document().as_json().clone();
    let ctx = Context::default();
    let render = |app: &mut Workbench, events| {
        ctx.run(
            RawInput {
                events,
                screen_rect: Some(Rect::from_min_size(pos2(0., 0.), vec2(450., 1000.))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.properties(ui));
            },
        )
    };
    let output = render(&mut app, vec![]);
    let label = output
        .shapes
        .iter()
        .find_map(|s| match &s.1 {
            egui::Shape::Text(t) if t.galley.job.text == "Position X" => Some(t.pos),
            _ => None,
        })
        .unwrap();
    let field = output
        .shapes
        .iter()
        .find_map(|s| match &s.1 {
            egui::Shape::Text(t)
                if t.galley.job.text == "0"
                    && t.pos.x > label.x
                    && (t.pos.y - label.y).abs() < 5. =>
            {
                Some(t.pos + vec2(3., 5.))
            }
            _ => None,
        })
        .unwrap();
    for pressed in [true, false] {
        render(
            &mut app,
            vec![
                Event::PointerMoved(field),
                Event::PointerButton {
                    pos: field,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    render(&mut app, vec![]);
    let id = ctx.memory(|m| m.focus()).unwrap();
    let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
    state.set_ccursor_range(Some(egui::text::CCursorRange::two(
        egui::text::CCursor::new(0),
        egui::text::CCursor::new(100),
    )));
    state.store(&ctx, id);
    render(&mut app, vec![Event::Text("675/2".into())]);
    assert_eq!(
        app.editor.document().as_json(),
        &before,
        "Typing must not edit the chart"
    );
    render(&mut app, vec![key(Key::Enter)]);
    assert_eq!(app.editor.document().note(0, 0).unwrap().x, 337.5);
    let after = app.editor.document().as_json().clone();
    app.editor.undo();
    assert_eq!(app.editor.document().as_json(), &before);
    app.editor.redo();
    assert_eq!(app.editor.document().as_json(), &after);
}
