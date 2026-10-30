use super::*;
use crate::scroll::TouchScroll as _;
#[test]
fn a_toolbar_swipe_scrolls_horizontally_without_pressing_its_buttons() {
    let ctx = egui::Context::default();
    let mut router = TouchRouter::default();
    let mut clicked = 0;
    let mut area = egui::Rect::NOTHING;
    let mut offset = 0.;
    let mut time = 0.;
    let mut frame = |events| {
        time += 1. / 60.;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(300., 100.),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| {
                crate::scroll::begin_frame(ctx);
                egui::CentralPanel::default().show(ctx, |ui| {
                    let output = egui::ScrollArea::horizontal()
                        .id_source("toolbar")
                        .touch_show_x(ui, |ui| {
                            ui.horizontal(|ui| {
                                for i in 0..20 {
                                    if ui.button(format!("Tool {i}")).clicked() {
                                        clicked += 1;
                                    }
                                }
                            });
                        });
                    area = output.inner_rect;
                    offset = output.state.offset.x;
                });
            },
        );
        (area, offset, clicked)
    };
    frame(vec![]);
    let (area, _, _) = frame(vec![]);
    let start = area.center();
    for (phase, dx, t) in [
        (TouchPhase::Start, 0., 1.),
        (TouchPhase::Move, -60., 1.05),
        (TouchPhase::Move, -130., 1.1),
        (TouchPhase::End, -150., 1.11),
    ] {
        let (events, canceled) =
            router.event_with_context(&ctx, phase, 9, start + egui::vec2(dx, 0.), t);
        assert!(!canceled);
        frame(events);
    }
    let (_, offset, clicked) = frame(vec![]);
    assert!(offset > 140.);
    assert_eq!(clicked, 0);
}
