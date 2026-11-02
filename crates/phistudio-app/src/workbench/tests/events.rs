use super::*;

#[test]
fn notes_and_events_are_edited_together_on_one_clock() {
    use phistudio_core::{EventId, EventType};
    let mut app = Workbench::new(ChartDocument::empty("Combined", 120.).unwrap(), None);
    app.transport.beat = 2.;
    let grid = test_grid(&app);
    let events = grid.events();
    assert!(grid.rect.right() < events.rect.left());
    for beat in [0., 2., 3.25, 100.] {
        assert_eq!(grid.y(beat), events.y(beat));
    }
    app.timeline.tool = Tool::Place(NoteKind::Tap);
    let note_pos = egui::pos2(grid.x(0.), grid.y(3.));
    grid_events(
        &mut app,
        &[pointer(note_pos, true), pointer(note_pos, false)],
        false,
    );
    assert_eq!(app.editor.document().note_count(), 1);
    app.timeline.tool = Tool::Event;
    let start = egui::pos2(events.rect.left() + events.rect.width() * 0.1, events.y(3.));
    let end = egui::pos2(start.x, events.y(4.));
    grid_events(
        &mut app,
        &[pointer(start, true), pointer(end, false)],
        false,
    );
    let id = EventId {
        line: 0,
        layer: 0,
        kind: EventType::MoveX,
        index: 1,
    };
    assert_eq!(app.events.selected, Some(id));
    assert_eq!(app.editor.document().note_count(), 1);
    app.timeline.tool = Tool::Select;
    grid_events(
        &mut app,
        &[pointer(note_pos, true), pointer(note_pos, false)],
        false,
    );
    assert!(app.selected.is_some());
    assert!(app.events.selected.is_none());
    let middle = egui::pos2(start.x, events.y(3.5));
    grid_events(
        &mut app,
        &[pointer(middle, true), pointer(middle, false)],
        false,
    );
    assert_eq!(app.events.selected, Some(id));
    assert!(app.selected.is_none());
    let original = app.editor.document().as_json().clone();
    app.timeline.tool = Tool::Select;
    app.transport.playing = true;
    let blank = egui::pos2(grid.x(600.), events.y(3.5));
    grid_events(
        &mut app,
        &[
            pointer(blank, true),
            pointer(blank + egui::vec2(0., 48.), false),
        ],
        false,
    );
    let after = test_grid(&app);
    assert_eq!(app.transport.beat, 2.5);
    assert!(app.transport.playing);
    assert_eq!(after.y(3.), after.events().y(3.));
    assert_eq!(app.editor.document().as_json(), &original);
}

#[test]
fn event_create_move_resize_and_cancel_use_one_history_action_per_gesture() {
    use phistudio_core::{EventId, EventType};
    let mut app = Workbench::new(ChartDocument::empty("Events", 120.).unwrap(), None);
    app.timeline.tool = Tool::Event;
    app.transport.beat = 2.;
    let original = app.editor.document().as_json().clone();
    let grid = test_grid(&app).events();
    let x = grid.rect.left() + grid.rect.width() * 0.1;
    let down = egui::pos2(x, grid.y(3.));
    let up = egui::pos2(x, grid.y(4.));
    grid_events(&mut app, &[pointer(down, true), pointer(up, false)], false);
    let id = EventId {
        line: 0,
        layer: 0,
        kind: EventType::MoveX,
        index: 1,
    };
    assert_eq!(app.events.selected, Some(id));
    let created = app.editor.document().event(id).unwrap();
    assert_eq!(created.start_time, Beat::new(3, 0, 1).unwrap());
    assert_eq!(created.end_time, Beat::new(4, 0, 1).unwrap());
    app.editor.undo();
    assert_eq!(app.editor.document().as_json(), &original);
    app.editor.redo();
    app.timeline.tool = Tool::Select;
    let middle = egui::pos2(x, grid.y(3.5));
    let shifted = middle - egui::vec2(0., grid.zoom * 0.5);
    grid_events(
        &mut app,
        &[pointer(middle, true), pointer(shifted, false)],
        false,
    );
    assert_eq!(
        app.editor.document().event(id).unwrap().start_time,
        Beat::new(3, 1, 2).unwrap()
    );
    let end = egui::pos2(x, grid.y(4.5));
    let longer = egui::pos2(x, grid.y(5.));
    grid_events(
        &mut app,
        &[pointer(end, true), pointer(longer, false)],
        false,
    );
    assert_eq!(
        app.editor.document().event(id).unwrap().end_time,
        Beat::new(5, 0, 1).unwrap()
    );
    app.editor.undo();
    app.editor.undo();
    assert_eq!(app.editor.document().event(id).unwrap(), created);
    let before = app.editor.document().as_json().clone();
    let rev = app.editor.revision();
    app.timeline.tool = Tool::Event;
    grid_events(&mut app, &[pointer(egui::pos2(x, grid.y(5.)), true)], false);
    grid_events(&mut app, &[], true);
    grid_events(
        &mut app,
        &[pointer(egui::pos2(x, grid.y(6.)), false)],
        false,
    );
    assert_eq!(app.editor.document().as_json(), &before);
    assert_eq!(app.editor.revision(), rev);
}

#[test]
fn dragging_an_event_group_moves_all_tracks_and_one_undo_restores_the_original_chart() {
    let mut app = group_app();
    let before = app.editor.document().as_json().clone();
    let selected = phistudio_core::tools::Selection {
        notes: vec![],
        events: [
            phistudio_core::EventType::Alpha,
            phistudio_core::EventType::Rotate,
        ]
        .into_iter()
        .map(|kind| phistudio_core::EventId {
            line: 0,
            layer: 0,
            kind,
            index: 0,
        })
        .collect(),
    };
    app.set_selection(selected);
    let grid = crate::grid_input::geometry(
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(900., 600.)),
        &app,
    );
    let x = grid.event_rect.left() + grid.event_rect.width() * 0.7;
    let a = egui::pos2(x, grid.y(0.5));
    let b = a - egui::vec2(0., grid.zoom * 2.);
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
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.selection().events.len(), 2);
    for id in app.selection().events {
        assert_eq!(
            app.editor.document().event(id).unwrap().start_time,
            Beat::new(2, 0, 1).unwrap()
        );
    }
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &before);
}

#[test]
fn app_linked_event_resize_preserves_other_axis_value_and_undoes_together() {
    use phistudio_core::{EventId, EventType, LineEvent};
    let mut raw = ChartDocument::empty("Link", 120.)
        .unwrap()
        .as_json()
        .clone();
    raw["judgeLineList"][0]["eventLayers"][0] = serde_json::json!({});
    let mut app = Workbench::new(ChartDocument::from_value(raw.clone()).unwrap(), None);
    app.options.xy_link = true;
    let event = LineEvent {
        end_time: Beat::new(2, 0, 1).unwrap(),
        start: 50.,
        end: 100.,
        ..Default::default()
    };
    assert!(app.commit_authoring(Command::AddEvent {
        line: 0,
        layer: 0,
        kind: EventType::MoveX,
        event
    }));
    assert_eq!(app.selection().events.len(), 1);
    let id = EventId {
        line: 0,
        layer: 0,
        kind: EventType::MoveX,
        index: 0,
    };
    let pair = app.editor.document().xy_pair(id).unwrap();
    let before = app.editor.document().event(pair).unwrap();
    let mut event = app.editor.document().event(id).unwrap();
    event.end_time = Beat::new(3, 0, 1).unwrap();
    event.end = 900.;
    assert!(app.commit_authoring(Command::UpdateEvent { id, event }));
    assert_eq!(app.editor.document().event(pair).unwrap().end, before.end);
    assert_eq!(
        app.editor.document().event(pair).unwrap().end_time,
        Beat::new(3, 0, 1).unwrap()
    );
    app.delete_selected();
    assert!(
        app.editor
            .document()
            .events(0, 0, EventType::MoveY)
            .unwrap()
            .is_empty()
    );
    for _ in 0..3 {
        app.history_step(false);
    }
    assert_eq!(app.editor.document().as_json(), &raw);
}
