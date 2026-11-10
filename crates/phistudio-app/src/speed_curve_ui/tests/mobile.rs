use super::*;
use crate::touch::TouchRouter;

struct Phone {
    ctx: egui::Context,
    router: TouchRouter,
    modal_input: crate::runtime::ModalInput,
    size: egui::Vec2,
}
impl Phone {
    fn batch(&mut self, app: &mut Workbench, touches: &[(egui::TouchPhase, Pos2)]) {
        let mut events = vec![];
        let mut time = self.ctx.input(|i| i.time);
        let mut canceled = false;
        for &(phase, pos) in touches {
            time += 0.02;
            let (next, cancel) = self
                .router
                .event_with_context(&self.ctx, phase, 1, pos, time);
            events.extend(next);
            canceled |= cancel;
        }
        self.frame(app, events, canceled);
    }
    fn frame(&mut self, app: &mut Workbench, mut events: Vec<egui::Event>, mut canceled: bool) {
        if self.modal_input.update(app) {
            let mut cancel = self.router.cancel();
            cancel.append(&mut events);
            events = cancel;
            canceled = true;
        }
        let time = self.ctx.input(|i| i.time) + 1. / 60.;
        let _ = self.ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, self.size)),
                pixels_per_point: Some(2.75),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| app.sound_ui(ctx, canceled),
        );
    }
    fn touch(&mut self, app: &mut Workbench, phase: egui::TouchPhase, pos: Pos2) {
        let now = self.ctx.input(|i| i.time) + 1. / 60.;
        let (events, canceled) = self
            .router
            .event_with_context(&self.ctx, phase, 1, pos, now);
        self.frame(app, events, canceled);
    }
    #[track_caller]
    fn settle(&mut self, app: &mut Workbench) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.frame(app, vec![], false);
            if app.speed_curve.profile.is_some()
                && app.speed_curve.worker.is_none()
                && !app.speed_curve.commit_pending
                && app
                    .speed_curve
                    .target
                    .is_some_and(|t| t.revision == app.editor.revision())
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "open={} profile={} worker={} pending={} target={:?} rev={} selection={:?} error={:?}",
                app.speed_curve.open,
                app.speed_curve.profile.is_some(),
                app.speed_curve.worker.is_some(),
                app.speed_curve.commit_pending,
                app.speed_curve.target.map(|t| (t.revision, t.id)),
                app.editor.revision(),
                app.selected,
                app.speed_curve.error
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        for _ in 0..3 {
            self.frame(app, vec![], false);
        }
    }
}

#[test]
fn routed_phone_taps_add_points_and_vertical_drags_move_them() {
    for size in [vec2(360., 780.), vec2(873., 393.)] {
        let mut phone = Phone {
            ctx: context(),
            router: TouchRouter::default(),
            modal_input: Default::default(),
            size,
        };
        let mut app = app();
        phone.settle(&mut app);
        let original = app.editor.document().as_json().clone();
        app.speed_curve.mode = Mode::Add;
        let distance = graph::distance_at(app.speed_curve.profile.as_ref().unwrap(), 2.)
            - app.speed_curve.limit * 0.6;
        let pos = point(&phone.ctx, &app, 2., distance);
        assert!(
            crate::scroll::at(&phone.ctx, pos).is_none(),
            "Graph was captured as a scroll panel"
        );
        phone.touch(&mut app, egui::TouchPhase::Start, pos);
        phone.touch(&mut app, egui::TouchPhase::End, pos);
        phone.settle(&mut app);
        let added = app.editor.document().as_json().clone();
        assert_ne!(
            added["judgeLineList"][0]["eventLayers"][0]["speedEvents"],
            original["judgeLineList"][0]["eventLayers"][0]["speedEvents"],
            "Tap failed at {size:?}: {:?}",
            app.speed_curve.error
        );
        app.speed_curve.mode = Mode::Edit;
        let node = app
            .speed_curve
            .points
            .iter()
            .find(|p| p.beat == Beat::new(2, 0, 1).unwrap())
            .unwrap()
            .clone();
        let down = point(&phone.ctx, &app, 2., node.distance);
        let up = down + vec2(0., -24.);
        phone.touch(&mut app, egui::TouchPhase::Start, down);
        phone.touch(&mut app, egui::TouchPhase::Move, up);
        assert!(
            app.speed_curve.drag.is_some(),
            "Drag did not start at {size:?}"
        );
        assert_eq!(app.editor.document().as_json(), &added);
        phone.touch(&mut app, egui::TouchPhase::End, up);
        phone.settle(&mut app);
        assert_ne!(
            app.editor.document().as_json(),
            &added,
            "Drag did not commit at {size:?}: {:?}",
            app.speed_curve.error
        );
        app.history_step_now(false);
        assert_eq!(app.editor.document().as_json(), &added);
        app.history_step_now(false);
        assert_eq!(app.editor.document().as_json(), &original);
    }
}

#[test]
fn an_android_drag_delivered_in_one_frame_moves_the_point_once() {
    let mut phone = Phone {
        ctx: context(),
        router: TouchRouter::default(),
        modal_input: Default::default(),
        size: vec2(360., 780.),
    };
    let mut app = app();
    add(&phone.ctx, &mut app);
    phone.settle(&mut app);
    app.speed_curve.mode = Mode::Edit;
    let node = app
        .speed_curve
        .points
        .iter()
        .find(|p| p.beat == Beat::new(2, 0, 1).unwrap())
        .unwrap()
        .clone();
    let down = point(&phone.ctx, &app, 2., node.distance);
    let up = down + vec2(0., 32.);
    let before = app.editor.document().as_json().clone();
    let mut events = vec![];
    for (phase, pos, time) in [
        (egui::TouchPhase::Start, down, 1.),
        (egui::TouchPhase::Move, down + vec2(0., 16.), 1.04),
        (egui::TouchPhase::End, up, 1.08),
    ] {
        events.extend(
            phone
                .router
                .event_with_context(&phone.ctx, phase, 1, pos, time)
                .0,
        );
    }
    phone.frame(&mut app, events, false);
    phone.settle(&mut app);
    assert_ne!(
        app.editor.document().as_json(),
        &before,
        "A batched phone drag was lost"
    );
    app.history_step_now(false);
    assert_eq!(app.editor.document().as_json(), &before);
}

#[test]
fn batched_phone_taps_add_select_delete_and_report_duplicate_beats() {
    use egui::TouchPhase::{End, Start};
    let mut phone = Phone {
        ctx: context(),
        router: TouchRouter::default(),
        modal_input: Default::default(),
        size: vec2(360., 780.),
    };
    let mut app = app();
    phone.settle(&mut app);
    app.speed_curve.mode = Mode::Add;
    let distance = graph::distance_at(app.speed_curve.profile.as_ref().unwrap(), 2.);
    let pos = point(&phone.ctx, &app, 2., distance);
    phone.batch(&mut app, &[(Start, pos), (End, pos)]);
    phone.settle(&mut app);
    let added = app.editor.document().as_json().clone();
    let node = app
        .speed_curve
        .points
        .iter()
        .find(|p| p.beat == Beat::new(2, 0, 1).unwrap())
        .unwrap()
        .clone();
    app.speed_curve.mode = Mode::Edit;
    let pos = point(&phone.ctx, &app, 2., node.distance);
    phone.batch(&mut app, &[(Start, pos), (End, pos)]);
    phone.settle(&mut app);
    assert_eq!(
        app.editor.document().as_json(),
        &added,
        "Selecting a node must not snap its distance or create history"
    );
    assert!(
        app.speed_curve
            .selected
            .is_some_and(|i| app.speed_curve.points[i].beat == node.beat)
    );
    app.speed_curve.mode = Mode::Delete;
    phone.batch(&mut app, &[(Start, pos), (End, pos)]);
    phone.settle(&mut app);
    assert!(!app.speed_curve.points.iter().any(|p| p.beat == node.beat));
    app.history_step_now(false);
    assert_eq!(app.editor.document().as_json(), &added);
    app.select(Some(NoteId { line: 0, index: 0 }));
    phone.settle(&mut app);
    app.speed_curve.mode = Mode::Add;
    let pos = point(&phone.ctx, &app, 4., 0.);
    phone.batch(&mut app, &[(Start, pos), (End, pos)]);
    assert!(
        app.speed_curve
            .error
            .as_deref()
            .is_some_and(|e| e.contains("distinct"))
    );
    assert_eq!(app.editor.document().as_json(), &added);
}

#[test]
fn phone_up_uses_its_final_position_and_a_second_finger_cancels_the_edit() {
    use egui::TouchPhase::{End, Move, Start};
    let mut phone = Phone {
        ctx: context(),
        router: TouchRouter::default(),
        modal_input: Default::default(),
        size: vec2(360., 780.),
    };
    let mut app = app();
    add(&phone.ctx, &mut app);
    phone.settle(&mut app);
    app.speed_curve.mode = Mode::Edit;
    let node = app
        .speed_curve
        .points
        .iter()
        .find(|p| p.beat == Beat::new(2, 0, 1).unwrap())
        .unwrap()
        .clone();
    let down = point(&phone.ctx, &app, 2., node.distance);
    let before = app.editor.document().as_json().clone();
    phone.touch(&mut app, Start, down);
    phone.touch(&mut app, End, down + vec2(0., 32.));
    phone.settle(&mut app);
    assert_ne!(app.editor.document().as_json(), &before);
    app.history_step_now(false);
    assert_eq!(app.editor.document().as_json(), &before);
    app.select(Some(NoteId { line: 0, index: 0 }));
    phone.settle(&mut app);
    let down = point(&phone.ctx, &app, 2., node.distance);
    phone.touch(&mut app, Start, down);
    phone.touch(&mut app, Move, down + vec2(0., 24.));
    let (events, canceled) =
        phone
            .router
            .event_with_context(&phone.ctx, Start, 2, down + vec2(20., 0.), 5.);
    assert!(canceled);
    phone.frame(&mut app, events, canceled);
    phone.touch(&mut app, End, down + vec2(0., 24.));
    let (events, canceled) =
        phone
            .router
            .event_with_context(&phone.ctx, End, 2, down + vec2(20., 0.), 5.1);
    phone.frame(&mut app, events, canceled);
    phone.settle(&mut app);
    assert_eq!(app.editor.document().as_json(), &before);
    assert!(!app.speed_curve.captures_input());
}

#[test]
fn dragging_in_add_mode_places_a_new_point_at_the_finger_release() {
    use egui::TouchPhase::{End, Move, Start};
    let mut phone = Phone {
        ctx: context(),
        router: TouchRouter::default(),
        modal_input: Default::default(),
        size: vec2(360., 780.),
    };
    let mut app = app();
    phone.settle(&mut app);
    let original = app.editor.document().as_json().clone();
    app.speed_curve.mode = Mode::Add;
    let distance = graph::distance_at(app.speed_curve.profile.as_ref().unwrap(), 2.);
    let down = point(&phone.ctx, &app, 2., distance);
    phone.batch(
        &mut app,
        &[
            (Start, down),
            (Move, down + vec2(0., 12.)),
            (End, down + vec2(0., 24.)),
        ],
    );
    phone.settle(&mut app);
    assert_ne!(
        app.editor.document().as_json()["judgeLineList"][0]["eventLayers"][0]["speedEvents"],
        original["judgeLineList"][0]["eventLayers"][0]["speedEvents"]
    );
    let node = app
        .speed_curve
        .points
        .iter()
        .find(|p| p.beat == Beat::new(2, 0, 1).unwrap())
        .unwrap();
    assert!((node.distance - distance).abs() > 100.);
    app.history_step_now(false);
    assert_eq!(app.editor.document().as_json(), &original);
}

#[test]
fn a_real_dialog_still_resets_the_platform_pointer_during_a_graph_gesture() {
    let mut phone = Phone {
        ctx: context(),
        router: TouchRouter::default(),
        modal_input: Default::default(),
        size: vec2(360., 780.),
    };
    let mut app = app();
    phone.settle(&mut app);
    let before = app.editor.document().as_json().clone();
    app.speed_curve.mode = Mode::Add;
    let pos = point(&phone.ctx, &app, 2., 0.);
    phone.touch(&mut app, egui::TouchPhase::Start, pos);
    assert!(
        app.modal_open(),
        "The graph must reserve chart input while pressed"
    );
    assert!(app.speed_curve.captures_input());
    app.sounds.open = true;
    phone.frame(&mut app, vec![], false);
    assert!(!app.speed_curve.captures_input());
    app.sounds.open = false;
    phone.touch(&mut app, egui::TouchPhase::End, pos);
    phone.settle(&mut app);
    assert_eq!(app.editor.document().as_json(), &before);
    app.speed_curve.mode = Mode::Add;
    phone.touch(&mut app, egui::TouchPhase::Start, pos);
    phone.touch(&mut app, egui::TouchPhase::End, pos);
    phone.settle(&mut app);
    assert_ne!(
        app.editor.document().as_json(),
        &before,
        "The dialog must release input for the next graph gesture"
    );
}
