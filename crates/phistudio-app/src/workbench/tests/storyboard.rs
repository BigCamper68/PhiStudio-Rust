use super::*;

#[test]
fn storyboard_shortcuts_clipboard_group_drag_and_delete_share_exact_undo() {
    use phistudio_core::{StoryEvent, StoryKind};
    let mut app = Workbench::new(ChartDocument::empty("Story tools", 120.).unwrap(), None);
    for (kind, start) in [
        (StoryKind::ScaleX, 1),
        (StoryKind::Color, 2),
        (StoryKind::Text, 3),
    ] {
        app.execute(Command::EditStoryboard {
            line: 0,
            kind,
            index: None,
            event: Some(StoryEvent::new(
                kind,
                Beat::new(start, 0, 1).unwrap(),
                Beat::new(start + 1, 0, 1).unwrap(),
            )),
        });
    }
    let original = app.editor.document().as_json().clone();
    let ctx = egui::Context::default();
    let size = Vec2::new(1194., 834.);
    app.open_storyboard();
    for _ in 0..3 {
        frame(&mut app, &ctx, size, vec![], false);
    }
    frame(
        &mut app,
        &ctx,
        size,
        shortcut(
            egui::Key::A,
            egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
        ),
        false,
    );
    assert_eq!(app.story_selection().len(), 3);
    frame(
        &mut app,
        &ctx,
        size,
        shortcut(
            egui::Key::C,
            egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
        ),
        false,
    );
    assert_eq!(app.tools.clipboard.as_ref().unwrap().len(), 3);
    app.beat = 8.;
    frame(
        &mut app,
        &ctx,
        size,
        shortcut(
            egui::Key::V,
            egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
        ),
        false,
    );
    assert_eq!(
        app.editor
            .document()
            .story_events(0, StoryKind::ScaleX)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(app.story_selection().len(), 3);
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &original);
    app.beat = 0.;
    app.select_all_stories(false);
    app.tool = Tool::BoxSelect;
    for _ in 0..2 {
        frame(&mut app, &ctx, size, vec![], false);
    }
    let grid = crate::grid_input::geometry(app.grid_area, &app);
    let start = egui::pos2(
        grid.event_rect.left() + grid.event_rect.width() / 12.,
        grid.y(1.5),
    );
    let end = start - egui::vec2(0., grid.zoom * 2.);
    frame(&mut app, &ctx, size, vec![pointer(start, true)], false);
    frame(
        &mut app,
        &ctx,
        size,
        vec![egui::Event::PointerMoved(end)],
        false,
    );
    frame(&mut app, &ctx, size, vec![pointer(end, false)], false);
    assert_eq!(
        app.editor
            .document()
            .story_events(0, StoryKind::ScaleX)
            .unwrap()[0]
            .start_time,
        Beat::new(3, 0, 1).unwrap()
    );
    assert_eq!(app.story_selection().len(), 3);
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &original);
    app.select_all_stories(false);
    app.delete_selected();
    assert!(StoryKind::EDITABLE.into_iter().all(|kind| {
        app.editor
            .document()
            .story_events(0, kind)
            .unwrap()
            .is_empty()
    }));
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &original);
}

#[test]
fn storyboard_grid_creates_moves_resizes_deletes_and_cancels_with_sparse_undo() {
    use phistudio_core::StoryKind;
    let mut app = Workbench::new(ChartDocument::empty("Story grid", 120.).unwrap(), None);
    app.open_storyboard();
    let raw = app.editor.document().as_json().clone();
    for (i, kind) in StoryKind::EDITABLE.iter().copied().enumerate() {
        let grid = test_grid(&app);
        let g = grid.events();
        let x = g.rect.left() + (i as f32 + 0.5) * g.rect.width() / 6.;
        app.set_tool(Tool::Event);
        grid_events(
            &mut app,
            &[
                pointer(egui::pos2(x, g.y(2.)), true),
                egui::Event::PointerMoved(egui::pos2(x, g.y(3.))),
                pointer(egui::pos2(x, g.y(3.)), false),
            ],
            false,
        );
        let events = app.editor.document().story_events(0, kind).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].start_time.as_f64(), 2.);
        assert_eq!(events[0].end_time.as_f64(), 3.);
        app.set_tool(Tool::Select);
        let pos = egui::pos2(x, g.y(2.5));
        grid_events(
            &mut app,
            &[
                pointer(pos, true),
                egui::Event::PointerMoved(pos + egui::vec2(0., -g.zoom * 0.5)),
                pointer(pos + egui::vec2(0., -g.zoom * 0.5), false),
            ],
            false,
        );
        assert_eq!(
            app.editor.document().story_events(0, kind).unwrap()[0]
                .start_time
                .as_f64(),
            2.5
        );
        let end = egui::pos2(x, g.y(3.5));
        grid_events(
            &mut app,
            &[pointer(end, true), pointer(egui::pos2(x, g.y(4.)), false)],
            false,
        );
        assert_eq!(
            app.editor.document().story_events(0, kind).unwrap()[0]
                .end_time
                .as_f64(),
            4.
        );
        let before = app.editor.document().as_json().clone();
        app.delete_selected();
        assert!(
            app.editor
                .document()
                .story_events(0, kind)
                .unwrap()
                .is_empty()
        );
        app.history_step(false);
        assert_eq!(app.editor.document().as_json(), &before);
        app.history_step(false);
        app.history_step(false);
        app.history_step(false);
        assert_eq!(app.editor.document().as_json(), &raw);
        app.set_tool(Tool::Event);
        grid_events(
            &mut app,
            &[
                pointer(pos, true),
                egui::Event::PointerMoved(pos - egui::vec2(0., 50.)),
            ],
            false,
        );
        grid_events(&mut app, &[], true);
        assert_eq!(app.editor.document().as_json(), &raw);
    }
    assert!(!StoryKind::EDITABLE.contains(&StoryKind::Incline));
}

#[test]
fn storyboard_eraser_stroke_is_one_sparse_transaction_and_cancel_keeps_raw_events() {
    use phistudio_core::{StoryEvent, StoryKind};
    let mut app = Workbench::new(ChartDocument::empty("Erase", 120.).unwrap(), None);
    for kind in [StoryKind::ScaleX, StoryKind::ScaleY] {
        app.commit_authoring(Command::EditStoryboard {
            line: 0,
            kind,
            index: None,
            event: Some(StoryEvent::new(
                kind,
                Beat::ONE,
                Beat::new(3, 0, 1).unwrap(),
            )),
        });
    }
    app.execute(Command::AddNote {
        line: 0,
        note: Note {
            start_time: Beat::new(2, 0, 1).unwrap(),
            end_time: Beat::new(2, 0, 1).unwrap(),
            ..Default::default()
        },
    });
    app.open_storyboard();
    app.set_tool(Tool::Erase);
    let original = app.editor.document().as_json().clone();
    let history = app.editor.history_cursor();
    let g = test_grid(&app);
    let a = egui::pos2(g.event_rect.left() + g.event_rect.width() / 12., g.y(2.));
    let b = egui::pos2(g.event_rect.left() + g.event_rect.width() / 4., g.y(2.));
    grid_events(
        &mut app,
        &[pointer(a, true), egui::Event::PointerMoved(b)],
        false,
    );
    grid_events(&mut app, &[], true);
    assert_eq!(app.editor.document().as_json(), &original);
    grid_events(
        &mut app,
        &[
            pointer(a, true),
            egui::Event::PointerMoved(b),
            pointer(b, false),
        ],
        false,
    );
    assert_eq!(app.editor.history_cursor(), history + 1);
    assert!(
        app.editor
            .document()
            .story_events(0, StoryKind::ScaleX)
            .unwrap()
            .is_empty()
    );
    assert!(
        app.editor
            .document()
            .story_events(0, StoryKind::ScaleY)
            .unwrap()
            .is_empty()
    );
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &original);
    let left = egui::pos2(g.x(0.), g.y(2.));
    grid_events(
        &mut app,
        &[
            pointer(left, true),
            egui::Event::PointerMoved(b),
            pointer(b, false),
        ],
        false,
    );
    assert_eq!(app.editor.document().note_count(), 0);
    assert!(
        app.editor
            .document()
            .story_events(0, StoryKind::ScaleX)
            .unwrap()
            .is_empty()
    );
    assert!(
        app.editor
            .document()
            .story_events(0, StoryKind::ScaleY)
            .unwrap()
            .is_empty()
    );
    assert_eq!(app.editor.history_cursor(), history + 1);
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &original);
}
