use super::*;

#[test]
fn second_finger_cancels_and_remaining_finger_never_clicks() {
    let mut router = TouchRouter::default();
    assert!(!router.event(TouchPhase::Start, 1, egui::pos2(100., 100.)).1);
    let (events, canceled) = router.event(TouchPhase::Start, 2, egui::pos2(200., 100.));
    assert!(canceled);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::PointerButton { pressed: false, pos, .. } if pos.x < 0.))
    );
    router.event(TouchPhase::End, 1, egui::pos2(100., 100.));
    let remaining = router.event(TouchPhase::Move, 2, egui::pos2(220., 100.)).0;
    assert!(
        !remaining
            .iter()
            .any(|e| matches!(e, Event::PointerMoved(_)))
    );
    router.event(TouchPhase::End, 2, egui::pos2(220., 100.));
    let fresh = router.event(TouchPhase::Start, 3, egui::pos2(120., 100.)).0;
    assert!(
        fresh
            .iter()
            .any(|e| matches!(e, Event::PointerButton { pressed: true, .. }))
    );
}

#[test]
fn canceling_touch_cannot_click_an_egui_button() {
    let ctx = egui::Context::default();
    let mut router = TouchRouter::default();
    let mut clicked = false;
    let mut rect = egui::Rect::NOTHING;
    let mut run = |events: Vec<Event>| {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(300., 200.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let r = ui.button("Open");
                    rect = r.rect;
                    clicked |= r.clicked();
                });
            },
        );
        rect
    };
    let pos = run(vec![]).center();
    run(router.event(TouchPhase::Start, 1, pos).0);
    run(router
        .event(TouchPhase::Start, 2, pos + egui::vec2(30., 0.))
        .0);
    drop(run);
    assert!(!clicked);
}
