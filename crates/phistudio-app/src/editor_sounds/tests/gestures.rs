use super::*;
use crate::{
    grid_input::{self, Grid},
    workbench::Tool,
};
use egui::{Pos2, Rect, pos2, vec2};
use phistudio_core::{EventId, EventType, LineEvent, NoteId};

fn grid(app: &Workbench) -> Grid {
    grid_input::geometry(Rect::from_min_size(Pos2::ZERO, vec2(900., 600.)), app)
}
fn input(app: &mut Workbench, grid: Grid, events: &[egui::Event], canceled: bool) {
    grid_input::handle(app, events, grid, Rect::EVERYTHING, |_| true, canceled);
}
fn button(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}
fn note_app(kind: NoteKind) -> Workbench {
    let mut app = app();
    app.transport.beat = 2.;
    app.execute(Command::AddNote {
        line: 0,
        note: Note {
            kind,
            start_time: Beat::new(3, 0, 1).unwrap(),
            end_time: Beat::new(if kind == NoteKind::Hold { 4 } else { 3 }, 0, 1).unwrap(),
            x: 0.,
            ..Default::default()
        },
    });
    app.select(Some(NoteId { line: 0, index: 0 }));
    app.timeline.tool = Tool::Select;
    dominant(&mut app);
    app
}
fn event_app() -> (Workbench, EventId) {
    let mut app = app();
    app.transport.beat = 2.;
    assert!(app.commit_authoring(Command::AddEvent {
        line: 0,
        layer: 0,
        kind: EventType::MoveX,
        event: LineEvent {
            start_time: Beat::new(3, 0, 1).unwrap(),
            end_time: Beat::new(4, 0, 1).unwrap(),
            ..Default::default()
        }
    }));
    let id = app.events.selected.unwrap();
    app.timeline.tool = Tool::Select;
    dominant(&mut app);
    (app, id)
}
fn changed_gesture(app: &mut Workbench, grid: Grid, down: Pos2, up: Pos2) {
    let before = app.editor.document().as_json().clone();
    input(app, grid, &[button(down, true)], false);
    input(app, grid, &[egui::Event::PointerMoved(up)], false);
    assert_eq!(dominant(app), None, "Staged motion must not play feedback");
    assert_eq!(app.editor.document().as_json(), &before);
    input(app, grid, &[button(up, false)], false);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(app.editor.document().as_json(), &before);
    assert_eq!(dominant(app), Some(Event::MoveResize));
    assert_eq!(
        dominant(app),
        None,
        "A committed gesture plays exactly once"
    );
    app.history_step_now(false);
    assert_eq!(
        app.editor.document().as_json(),
        &before,
        "One undo restores the entire gesture"
    );
    assert_eq!(dominant(app), Some(Event::Undo));
}
#[test]
fn all_note_moves_and_both_hold_endpoints_share_one_committed_sound() {
    for kind in [
        NoteKind::Tap,
        NoteKind::Flick,
        NoteKind::Drag,
        NoteKind::Hold,
    ] {
        let mut app = note_app(kind);
        let grid = grid(&app);
        let down = pos2(
            grid.x(0.),
            grid.y(if kind == NoteKind::Hold { 3.5 } else { 3. }),
        );
        changed_gesture(&mut app, grid, down, down - vec2(0., grid.zoom * 0.5));
    }
    for (start, end) in [(3., 2.5), (4., 4.5)] {
        let mut app = note_app(NoteKind::Hold);
        let grid = grid(&app);
        changed_gesture(
            &mut app,
            grid,
            pos2(grid.x(0.), grid.y(start)),
            pos2(grid.x(0.), grid.y(end)),
        );
    }
}
#[test]
fn event_moves_and_both_event_endpoints_use_the_same_sound_as_notes() {
    for (start, end) in [(3.5, 4.), (3., 2.5), (4., 4.5)] {
        let (mut app, _) = event_app();
        let grid = grid(&app);
        let x = grid.event_rect.left() + grid.event_rect.width() * 0.1;
        changed_gesture(&mut app, grid, pos2(x, grid.y(start)), pos2(x, grid.y(end)));
    }
}
#[test]
fn dragging_a_mixed_selection_emits_one_shared_sound_and_keeps_one_undo() {
    let (mut app, id) = event_app();
    app.execute(Command::AddNote {
        line: 0,
        note: Note {
            start_time: Beat::new(3, 0, 1).unwrap(),
            end_time: Beat::new(3, 0, 1).unwrap(),
            ..Default::default()
        },
    });
    app.set_selection(phistudio_core::tools::Selection {
        notes: vec![NoteId { line: 0, index: 0 }],
        events: vec![id],
    });
    dominant(&mut app);
    let grid = grid(&app);
    let down = pos2(grid.x(0.), grid.y(3.));
    changed_gesture(&mut app, grid, down, down - vec2(0., grid.zoom * 0.5));
}
#[test]
fn canceled_and_returned_gestures_do_not_emit_move_feedback() {
    for event in [false, true] {
        for canceled in [false, true] {
            let mut app = if event {
                event_app().0
            } else {
                note_app(NoteKind::Hold)
            };
            let grid = grid(&app);
            let x = if event {
                grid.event_rect.left() + grid.event_rect.width() * 0.1
            } else {
                grid.x(0.)
            };
            let down = pos2(x, grid.y(3.5));
            let moved = down - vec2(0., grid.zoom * 0.5);
            let before = app.editor.document().as_json().clone();
            let revision = app.editor.revision();
            input(
                &mut app,
                grid,
                &[button(down, true), egui::Event::PointerMoved(moved)],
                false,
            );
            assert_eq!(dominant(&mut app), None);
            if canceled {
                input(&mut app, grid, &[], true);
            }
            input(&mut app, grid, &[button(down, false)], false);
            assert_eq!(app.editor.document().as_json(), &before);
            assert_eq!(app.editor.revision(), revision);
            assert_eq!(dominant(&mut app), None);
        }
    }
}
#[test]
fn taps_rejected_edits_and_property_changes_do_not_use_the_gesture_sound() {
    let mut app = note_app(NoteKind::Tap);
    let grid = grid(&app);
    let down = pos2(grid.x(0.), grid.y(3.));
    input(
        &mut app,
        grid,
        &[button(down, true), button(down, false)],
        false,
    );
    assert_eq!(dominant(&mut app), None);
    let original = app.editor.document().note(0, 0).unwrap();
    app.timeline.drag = Some(grid_input::Gesture::Move {
        id: NoteId {
            line: 0,
            index: 999,
        },
        current: original.clone(),
        original: original.clone(),
        origin: down,
        grid,
        moved: true,
    });
    input(
        &mut app,
        grid,
        &[button(down - vec2(0., grid.zoom), false)],
        false,
    );
    assert!(app.error.is_some());
    assert_eq!(dominant(&mut app), None);
    app.execute(Command::UpdateNote {
        id: NoteId { line: 0, index: 0 },
        note: Note {
            x: 135.,
            ..original
        },
    });
    assert_eq!(dominant(&mut app), Some(Event::Edit));
}

#[test]
fn touch_cancel_discards_staged_movement_without_a_commit_sound() {
    let mut app = note_app(NoteKind::Hold);
    let grid = grid(&app);
    let down = pos2(grid.x(0.), grid.y(3.5));
    let moved = down - vec2(0., grid.zoom * 0.5);
    let touch = |phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos,
        force: 1.,
    };
    let before = app.editor.document().as_json().clone();
    input(
        &mut app,
        grid,
        &[touch(egui::TouchPhase::Start, down), button(down, true)],
        false,
    );
    input(
        &mut app,
        grid,
        &[
            touch(egui::TouchPhase::Move, moved),
            egui::Event::PointerMoved(moved),
        ],
        false,
    );
    input(
        &mut app,
        grid,
        &[touch(egui::TouchPhase::Cancel, moved), button(moved, false)],
        false,
    );
    assert_eq!(app.editor.document().as_json(), &before);
    assert_eq!(dominant(&mut app), None);
}
