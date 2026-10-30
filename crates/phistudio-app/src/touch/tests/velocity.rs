use super::*;

#[test]
fn velocity_resists_last_move_jitter_in_both_directions_and_accepts_a_real_reversal() {
    for sign in [-1., 1.] {
        let mut motion = MotionHistory::new(1., 0.);
        motion.push(1.02, sign * 80.);
        motion.push(1.04, sign * 160.);
        motion.push(1.041, sign * 157.);
        assert!(motion.velocity() * sign > 2000.);
        for i in 1..=10 {
            motion.push(1.041 + i as f64 * 0.02, sign * (157. - i as f32 * 20.));
        }
        assert!(motion.velocity() * sign < -900.);
    }
}

#[test]
fn batched_android_timestamps_preserve_swipe_velocity_across_clock_epochs() {
    use crate::scroll::TouchScroll as _;
    let ctx = egui::Context::default();
    for _ in 0..3 {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(300., 240.),
                )),
                ..Default::default()
            },
            |ctx| {
                crate::scroll::begin_frame(ctx);
                egui::CentralPanel::default().show(ctx, |ui| {
                    egui::ScrollArea::vertical().touch_show(ui, |ui| {
                        ui.add_space(1000.);
                    });
                });
            },
        );
    }
    let start = egui::pos2(100., 180.);
    let end = start - egui::vec2(0., 60.);
    // Three hardware samples were delivered together in a single frame.
    // The physical 600 px/s swipe must not become a 2500 px/s frame spike.
    let mut router = TouchRouter::default();
    router.event_with_context(&ctx, TouchPhase::Start, 1, start, (100000., 10.));
    router.event_with_context(&ctx, TouchPhase::Move, 1, end, (100000.1, 10.0001));
    router.event_with_context(&ctx, TouchPhase::End, 1, end, (100000.11, 10.0002));
    let zone = crate::scroll::at(&ctx, start).unwrap();
    let before = egui::scroll_area::State::load(&ctx, zone.id)
        .unwrap()
        .offset
        .y;
    let events = router.scroll_momentum(&ctx, 10.0202);
    let after = egui::scroll_area::State::load(&ctx, zone.id)
        .unwrap()
        .offset
        .y;
    assert!(
        (after - before - 12.).abs() < 0.01,
        "Batched events changed inertia: {before} -> {after}"
    );
    assert!(
        !events.iter().any(|e| matches!(e, Event::Scroll(_))),
        "Touch inertia must not become a wheel event"
    );
}
