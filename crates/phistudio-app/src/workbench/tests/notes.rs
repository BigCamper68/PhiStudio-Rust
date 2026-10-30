use super::*;

#[test]
fn canceling_a_creation_gesture_leaves_the_chart_unchanged() {
    let mut app = Workbench::new(ChartDocument::empty("Test", 120.).unwrap(), None);
    app.drag = Some(timeline::Gesture::Create {
        start: Beat::ZERO,
        end: Beat::ONE,
        x: 0.,
        kind: NoteKind::Hold,
        grid: crate::grid_input::geometry(
            egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(600., 300.)),
            &app,
        ),
    });
    frame(
        &mut app,
        &egui::Context::default(),
        Vec2::new(1280., 800.),
        vec![],
        true,
    );
    assert!(app.drag.is_none());
    assert!(!app.editor.is_dirty());
}

#[test]
fn placement_swipe_scrolls_and_a_batched_tap_uses_its_down_position() {
    let mut app = Workbench::new(ChartDocument::empty("Test", 120.).unwrap(), None);
    app.tool = Tool::Place(NoteKind::Tap);
    app.beat = 8.;
    let grid = test_grid(&app);
    let down = egui::pos2(grid.x(135.), grid.y(10.));
    // A far-away hover from the previous frame must never become the down origin.
    grid_events(
        &mut app,
        &[egui::Event::PointerMoved(egui::pos2(10., 10.))],
        false,
    );
    grid_events(
        &mut app,
        &[pointer(down, true), pointer(down, false)],
        false,
    );
    let note = app.editor.document().note(0, 0).unwrap();
    assert_eq!(note.start_time, Beat::new(10, 0, 1).unwrap());
    assert_eq!(note.x, 135.);
    app.editor.mark_saved();
    let end = down + egui::vec2(0., 96.);
    grid_events(
        &mut app,
        &[
            pointer(down, true),
            egui::Event::PointerMoved(end),
            pointer(end, false),
        ],
        false,
    );
    assert_eq!(app.editor.document().note_count(), 1);
    assert!(!app.editor.is_dirty());
    assert!((app.beat - 9.).abs() < 1e-9);
}

#[test]
fn moving_requires_selection_and_commits_the_final_release_once() {
    let mut app = Workbench::new(ChartDocument::empty("Test", 120.).unwrap(), None);
    app.beat = 1.;
    let start = Beat::new(2, 1, 3).unwrap();
    let end = Beat::new(3, 2, 3).unwrap();
    app.execute(Command::AddNote {
        line: 0,
        note: Note {
            kind: NoteKind::Hold,
            start_time: start,
            end_time: end,
            x: 0.,
            ..Note::default()
        },
    });
    let grid = test_grid(&app);
    let pos = egui::pos2(grid.x(0.), grid.y(3.));
    grid_events(&mut app, &[pointer(pos, true), pointer(pos, false)], false);
    assert_eq!(app.selected, Some(NoteId { line: 0, index: 0 }));
    let moved = pos + egui::vec2(60., -96.);
    // Release position is authoritative even with no intervening move event.
    grid_events(
        &mut app,
        &[pointer(pos, true), pointer(moved, false)],
        false,
    );
    let note = app.editor.document().note(0, 0).unwrap();
    assert_eq!(note.start_time, start.checked_add(Beat::ONE).unwrap());
    assert_eq!(note.end_time, end.checked_add(Beat::ONE).unwrap());
    assert!(note.x > 0.);
    app.editor.undo();
    assert_eq!(app.editor.document().note(0, 0).unwrap().start_time, start);
    app.editor.undo();
    assert_eq!(app.editor.document().note_count(), 0);
}

#[test]
fn hold_tap_has_a_positive_grid_duration_and_cancel_never_commits() {
    let mut app = Workbench::new(ChartDocument::empty("Test", 120.).unwrap(), None);
    app.tool = Tool::Place(NoteKind::Hold);
    let grid = test_grid(&app);
    let pos = egui::pos2(grid.x(0.), grid.y(2.));
    grid_events(&mut app, &[pointer(pos, true), pointer(pos, false)], false);
    let note = app.editor.document().note(0, 0).unwrap();
    assert_eq!(
        note.end_time.checked_sub(note.start_time).unwrap(),
        Beat::new(0, 1, 4).unwrap()
    );
    app.editor.undo();
    grid_events(&mut app, &[pointer(pos, true)], false);
    grid_events(&mut app, &[pointer(pos, false)], true);
    assert_eq!(app.editor.document().note_count(), 0);
}

#[test]
fn pinch_cancels_hold_keeps_its_chart_anchor_and_blocks_the_remaining_finger() {
    let mut app = Workbench::new(ChartDocument::empty("Test", 120.).unwrap(), None);
    app.beat = 8.;
    app.tool = Tool::Place(NoteKind::Hold);
    let mut router = crate::touch::TouchRouter::default();
    let a = egui::pos2(220., 180.);
    let b = egui::pos2(320., 180.);
    let anchor = test_grid(&app).raw_beat(180.);
    for (phase, id, pos) in [
        (egui::TouchPhase::Start, 1, a),
        (egui::TouchPhase::Start, 2, b),
        (egui::TouchPhase::Move, 1, a - egui::vec2(50., 0.)),
        (egui::TouchPhase::Move, 2, b + egui::vec2(50., 0.)),
    ] {
        let (events, canceled) = router.event(phase, id, pos);
        grid_events(&mut app, &events, canceled);
    }
    assert!((app.zoom - 192.).abs() < 1e-5);
    assert!((test_grid(&app).raw_beat(180.) - anchor).abs() < 1e-6);
    for (phase, id, pos) in [
        (egui::TouchPhase::End, 1, a),
        (egui::TouchPhase::Move, 2, b + egui::vec2(0., 70.)),
        (egui::TouchPhase::End, 2, b),
    ] {
        let (events, canceled) = router.event(phase, id, pos);
        grid_events(&mut app, &events, canceled);
    }
    assert_eq!(app.editor.document().note_count(), 0);
    assert!(app.drag.is_none());
}

#[test]
fn box_add_and_erase_gestures_are_bounded_and_one_history_action() {
    let mut app = group_app();
    let before = app.editor.document().as_json().clone();
    let grid = crate::grid_input::geometry(
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(900., 600.)),
        &app,
    );
    app.set_tool(Tool::BoxSelect);
    let a = egui::pos2(grid.x(-270.) - 30., grid.y(2.) - 15.);
    let b = egui::pos2(grid.x(135.) + 30., grid.y(1.) + 15.);
    group_input(
        &mut app,
        grid,
        vec![
            seek_pointer_button(a, true),
            egui::Event::PointerMoved(b),
            seek_pointer_button(b, false),
        ],
        false,
    );
    assert_eq!(app.selection().notes.len(), 2);
    assert_eq!(app.editor.document().as_json(), &before);
    app.set_tool(Tool::Select);
    app.tools.additive = true;
    let point = egui::pos2(grid.x(-270.), grid.y(1.));
    group_input(
        &mut app,
        grid,
        vec![
            seek_pointer_button(point, true),
            seek_pointer_button(point, false),
        ],
        false,
    );
    assert_eq!(app.selection().notes, vec![NoteId { line: 0, index: 1 }]);
    app.tools.additive = false;
    app.set_tool(Tool::Erase);
    let next = egui::pos2(grid.x(135.), grid.y(2.));
    group_input(
        &mut app,
        grid,
        vec![
            seek_pointer_button(point, true),
            egui::Event::PointerMoved(next),
            seek_pointer_button(next, false),
        ],
        false,
    );
    assert_eq!(app.editor.document().note_count(), 1);
    assert_eq!(app.editor.undo_label(), Some("Delete selection"));
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &before);
}

#[test]
fn dragging_a_selected_group_preserves_spacing_and_cancel_discards_the_whole_draft() {
    for cancel in [false, true] {
        let mut app = group_app();
        let before = app.editor.document().as_json().clone();
        app.set_selection(phistudio_core::tools::Selection {
            notes: vec![NoteId { line: 0, index: 0 }, NoteId { line: 0, index: 1 }],
            events: vec![],
        });
        let grid = crate::grid_input::geometry(
            egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(900., 600.)),
            &app,
        );
        let a = egui::pos2(grid.x(-270.), grid.y(1.));
        let b = a + egui::vec2(grid.rect.width() / 10., -grid.zoom);
        group_input(
            &mut app,
            grid,
            vec![seek_pointer_button(a, true), egui::Event::PointerMoved(b)],
            false,
        );
        assert_eq!(app.editor.document().as_json(), &before);
        group_input(&mut app, grid, vec![seek_pointer_button(b, false)], cancel);
        if cancel {
            assert_eq!(app.editor.document().as_json(), &before);
        } else {
            assert_eq!(app.selection().notes.len(), 2);
            let rows = app.editor.document().notes(0);
            let a = &rows[1].1;
            let b = &rows[2].1;
            assert_eq!(b.start_time.checked_sub(a.start_time).unwrap(), Beat::ONE);
            assert!((b.x - a.x - 405.).abs() < 0.001);
            app.history_step(false);
            assert_eq!(app.editor.document().as_json(), &before);
        }
    }
}

#[test]
fn moving_objects_to_another_line_and_layer_keeps_the_result_visible() {
    let mut app = group_app();
    app.execute(Command::AddLine {
        name: "Destination".into(),
    });
    let selection = phistudio_core::tools::Selection {
        notes: vec![NoteId { line: 0, index: 0 }],
        events: vec![phistudio_core::EventId {
            line: 0,
            layer: 0,
            kind: phistudio_core::EventType::Rotate,
            index: 0,
        }],
    };
    let edit = phistudio_core::tools::relocate(app.editor.document(), &selection, 1, 2, false);
    assert!(app.apply_tool(edit));
    assert_eq!(app.line, 1);
    assert_eq!(app.events.layer, 2);
    assert_eq!(app.selection().len(), 2);
    assert_eq!(app.editor.document().notes(0).len(), 2);
    assert_eq!(app.editor.document().notes(1).len(), 1);
}

#[test]
fn the_motion_generator_draws_a_circle_with_equal_screen_radii() {
    let mut app = Workbench::new(ChartDocument::empty("Circle", 120.).unwrap(), None);
    app.tools.show = true;
    app.tools.page = crate::toolkit::Page::Motion;
    let ctx = egui::Context::default();
    let size = Vec2::new(1194., 834.);
    for _ in 0..4 {
        frame(&mut app, &ctx, size, vec![], false);
    }
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        },
        |ctx| app.ui(ctx, false),
    );
    let path = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.1 {
            egui::Shape::Path(path)
                if path.points.len() > 20
                    && path.stroke.color == crate::theme::accent()
                    && path.stroke.width == 1.5 =>
            {
                Some(path)
            }
            _ => None,
        })
        .expect("Motion generator must draw its real preview path");
    let bounds = egui::Rect::from_points(&path.points);
    assert!((bounds.width() - bounds.height()).abs() < 0.1, "{bounds:?}");
    assert!(bounds.height() > 30.);
    assert!(!app.editor.is_dirty());
}
