use super::*;
use egui::{Pos2, pos2, vec2};
use phistudio_core::{ChartDocument, Command, Note};
use std::time::{Duration, Instant};
fn app() -> Workbench {
    let mut raw = ChartDocument::empty("Speed UI", 120.)
        .unwrap()
        .as_json()
        .clone();
    raw["judgeLineList"][0]["eventLayers"][0]["speedEvents"] = serde_json::json!([{"startTime":[0,0,1],"endTime":[8,0,1],"start":10.,"end":10.,"easingType":1,"custom":"preserve"}]);
    raw["judgeLineList"][0]["notes"] = serde_json::json!([
        Note {
            start_time: Beat::new(4, 0, 1).unwrap(),
            end_time: Beat::new(4, 0, 1).unwrap(),
            ..Default::default()
        },
        Note {
            start_time: Beat::new(6, 0, 1).unwrap(),
            end_time: Beat::new(6, 0, 1).unwrap(),
            ..Default::default()
        }
    ]);
    let mut app = Workbench::new(ChartDocument::from_value(raw).unwrap(), None);
    app.select(Some(NoteId { line: 0, index: 0 }));
    app.open_speed_curve();
    app
}
fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::fonts::definitions());
    ctx
}
fn frame(ctx: &egui::Context, app: &mut Workbench, events: Vec<egui::Event>, canceled: bool) {
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1280., 800.))),
            events,
            ..Default::default()
        },
        |ctx| app.ui(ctx, canceled),
    );
}
fn wait(ctx: &egui::Context, app: &mut Workbench, condition: impl Fn(&Workbench) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        frame(ctx, app, vec![], false);
        if condition(app) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Curve did not settle: {:?}",
            app.speed_curve.error
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    for _ in 0..2 {
        frame(ctx, app, vec![], false);
    }
}
fn button(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}
fn point(ctx: &egui::Context, app: &Workbench, beat: f64, distance: f64) -> Pos2 {
    let rect = ctx
        .data_mut(|d| d.get_temp::<egui::Rect>(egui::Id::new("speed-curve-plot")))
        .unwrap();
    pos2(
        rect.left() + ((beat - app.speed_curve.start) / app.speed_curve.span) as f32 * rect.width(),
        rect.center().y - (distance / app.speed_curve.limit) as f32 * rect.height() * 0.5,
    )
}
fn click(ctx: &egui::Context, app: &mut Workbench, pos: Pos2) {
    for pressed in [true, false] {
        frame(
            ctx,
            app,
            vec![egui::Event::PointerMoved(pos), button(pos, pressed)],
            false,
        );
    }
}
fn add(ctx: &egui::Context, app: &mut Workbench) {
    wait(ctx, app, |app| app.speed_curve.profile.is_some());
    app.speed_curve.mode = Mode::Add;
    let distance = graph::distance_at(app.speed_curve.profile.as_ref().unwrap(), 2.) + 100.;
    let pos = point(ctx, app, 2., distance);
    let revision = app.editor.revision();
    click(ctx, app, pos);
    wait(ctx, app, |app| {
        app.editor.revision() > revision
            && app.speed_curve.profile.is_some()
            && !app.speed_curve.busy()
            && app
                .speed_curve
                .target
                .is_some_and(|t| t.revision == app.editor.revision())
            && app.speed_curve.worker.is_none()
    });
}
#[test]
fn real_canvas_add_and_drag_commit_once_and_cancellation_keeps_the_document() {
    let ctx = context();
    let mut app = app();
    let initial = app.editor.document().as_json().clone();
    add(&ctx, &mut app);
    let added = app.editor.document().as_json().clone();
    assert_ne!(initial, added);
    app.speed_curve.mode = Mode::Edit;
    let node = app
        .speed_curve
        .points
        .iter()
        .find(|p| p.beat == Beat::new(2, 0, 1).unwrap())
        .unwrap()
        .clone();
    let down = point(&ctx, &app, 2., node.distance);
    let up = point(&ctx, &app, 2.5, node.distance + 200.);
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(down), button(down, true)],
        false,
    );
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(up)], false);
    assert!(app.speed_curve.drag.is_some());
    assert_eq!(app.editor.document().as_json(), &added);
    frame(&ctx, &mut app, vec![button(up, false)], true);
    for _ in 0..6 {
        frame(&ctx, &mut app, vec![], false);
    }
    assert_eq!(app.editor.document().as_json(), &added);
    let down = point(&ctx, &app, 2., node.distance);
    let up = point(&ctx, &app, 2.5, node.distance + 200.);
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(down), button(down, true)],
        false,
    );
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(up)], false);
    frame(&ctx, &mut app, vec![button(up, false)], false);
    let revision = app.editor.revision();
    wait(&ctx, &mut app, |app| app.editor.revision() > revision);
    assert_eq!(
        app.editor.document().as_json()["judgeLineList"][0]["notes"],
        initial["judgeLineList"][0]["notes"]
    );
    app.history_step_now(false);
    assert_eq!(app.editor.document().as_json(), &added);
    app.history_step_now(false);
    assert_eq!(app.editor.document().as_json(), &initial);
}
#[test]
fn a_pending_curve_edit_cannot_overwrite_a_new_note_revision() {
    let ctx = context();
    let mut app = app();
    wait(&ctx, &mut app, |a| a.speed_curve.profile.is_some());
    let beat = Beat::new(2, 0, 1).unwrap();
    let distance = graph::distance_at(app.speed_curve.profile.as_ref().unwrap(), 2.) + 100.;
    app.speed_curve.points.insert(
        1,
        SpeedCurvePoint {
            beat,
            source_beat: beat,
            distance,
            easing: None,
            fixed: false,
        },
    );
    app.speed_curve.request_fit(true);
    let id = app.selected.unwrap();
    let mut note = app.editor.document().note(0, 0).unwrap();
    note.speed = 2.;
    app.execute(Command::UpdateNote { id, note });
    let expected = app.editor.document().as_json().clone();
    wait(&ctx, &mut app, |a| {
        a.speed_curve.profile.is_some() && a.speed_curve.worker.is_none()
    });
    assert_eq!(app.editor.document().as_json(), &expected);
}
#[test]
fn selection_changes_refresh_the_nonmodal_panel_and_multiselection_clears_it() {
    let ctx = context();
    let mut app = app();
    wait(&ctx, &mut app, |a| a.speed_curve.profile.is_some());
    assert!(!app.modal_open());
    app.select(Some(NoteId { line: 0, index: 1 }));
    wait(&ctx, &mut app, |a| {
        a.speed_curve.profile.is_some()
            && a.speed_curve
                .source
                .as_ref()
                .is_some_and(|s| s.id.index == 1)
    });
    app.set_selection(phistudio_core::tools::Selection {
        notes: vec![NoteId { line: 0, index: 0 }, NoteId { line: 0, index: 1 }],
        events: vec![],
    });
    frame(&ctx, &mut app, vec![], false);
    assert!(app.speed_curve.profile.is_none());
    assert!(app.speed_curve.source.is_none());
    app.back();
    assert!(!app.speed_curve.open);
}
#[test]
fn delete_in_the_curve_window_never_deletes_the_selected_chart_note() {
    let ctx = context();
    let mut app = app();
    add(&ctx, &mut app);
    app.speed_curve.selected = app
        .speed_curve
        .points
        .iter()
        .position(|p| p.beat == Beat::new(2, 0, 1).unwrap());
    let notes = app.editor.document().as_json()["judgeLineList"][0]["notes"].clone();
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::Delete,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
        false,
    );
    assert_eq!(
        app.editor.document().as_json()["judgeLineList"][0]["notes"],
        notes
    );
}
#[test]
fn a_clock_from_an_older_preview_cannot_accept_a_pending_curve() {
    let ctx = context();
    let mut app = app();
    app.speed_curve
        .record_native_clock(app.session, app.editor.revision(), 4.);
    wait(&ctx, &mut app, |a| a.speed_curve.profile.is_some());
    let beat = Beat::new(2, 0, 1).unwrap();
    let distance = graph::distance_at(app.speed_curve.profile.as_ref().unwrap(), 2.) + 100.;
    app.speed_curve.points.insert(
        1,
        SpeedCurvePoint {
            beat,
            source_beat: beat,
            distance,
            easing: None,
            fixed: false,
        },
    );
    app.speed_curve.request_fit(true);
    let mut note = app.editor.document().note(0, 1).unwrap();
    note.start_time = Beat::new(8, 0, 1).unwrap();
    note.end_time = note.start_time;
    app.execute(Command::UpdateNote {
        id: NoteId { line: 0, index: 1 },
        note,
    });
    let expected = app.editor.document().as_json().clone();
    frame(&ctx, &mut app, vec![], false);
    assert!(app.speed_curve.clock_waiting);
    assert!(app.speed_curve.profile.is_none());
    app.speed_curve
        .record_native_clock(app.session, app.editor.revision(), 5.);
    wait(&ctx, &mut app, |a| {
        a.speed_curve.profile.is_some() && a.speed_curve.worker.is_none()
    });
    assert_eq!(app.editor.document().as_json(), &expected);
}
#[test]
fn closing_the_window_cancels_a_pending_apply_and_touch_cancel_never_commits() {
    let ctx = context();
    let mut app = app();
    add(&ctx, &mut app);
    let before = app.editor.document().as_json().clone();
    app.speed_curve.points[1].distance += 100.;
    app.speed_curve.request_fit(true);
    app.back();
    assert!(!app.speed_curve.open);
    for _ in 0..10 {
        frame(&ctx, &mut app, vec![], false);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(app.editor.document().as_json(), &before);
    app.open_speed_curve();
    wait(&ctx, &mut app, |a| a.speed_curve.profile.is_some());
    app.speed_curve.mode = Mode::Edit;
    let node = app.speed_curve.points[1].clone();
    let down = point(&ctx, &app, node.beat.as_f64(), node.distance);
    let moved = down + vec2(0., 30.);
    let touch = |phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos,
        force: 1.,
    };
    frame(
        &ctx,
        &mut app,
        vec![
            touch(egui::TouchPhase::Start, down),
            egui::Event::PointerMoved(down),
            button(down, true),
        ],
        false,
    );
    frame(
        &ctx,
        &mut app,
        vec![
            touch(egui::TouchPhase::Move, moved),
            egui::Event::PointerMoved(moved),
        ],
        false,
    );
    frame(
        &ctx,
        &mut app,
        vec![touch(egui::TouchPhase::Cancel, moved), button(moved, false)],
        true,
    );
    for _ in 0..8 {
        frame(&ctx, &mut app, vec![], false);
    }
    assert_eq!(app.editor.document().as_json(), &before);
}
