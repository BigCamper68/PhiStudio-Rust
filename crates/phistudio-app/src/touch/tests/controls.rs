use super::*;

#[test]
fn vertical_panel_swipes_over_fields_scroll_without_editing_values() {
    use crate::scroll::TouchScroll as _;
    for kind in 0..4 {
        let ctx = egui::Context::default();
        ctx.set_style(crate::theme::style());
        let mut router = TouchRouter::default();
        let mut value = 7.;
        let mut text = "Original".to_string();
        let mut clicks = 0;
        let mut widget = egui::Rect::NOTHING;
        let mut offset = 0.;
        let mut time = 0.;
        let mut run = |events| {
            time += 1. / 60.;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        Pos2::ZERO,
                        egui::vec2(300., 240.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    crate::scroll::begin_frame(ctx);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let out = egui::ScrollArea::vertical()
                            .id_source("scroll-fields")
                            .touch_show(ui, |ui| {
                                for i in 0..25 {
                                    let response = match kind {
                                        0 => ui.add(crate::numeric::Value::new(&mut value)),
                                        1 => ui.text_edit_singleline(&mut text),
                                        2 => {
                                            let response =
                                                ui.button("Do not activate while scrolling");
                                            clicks += usize::from(response.clicked());
                                            response
                                        }
                                        _ => ui.add(egui::Slider::new(&mut value, 0. ..=20.)),
                                    };
                                    if i == 3 {
                                        widget = response.rect;
                                    }
                                }
                            });
                        offset = out.state.offset.y;
                    });
                },
            );
            (widget, offset)
        };
        for _ in 0..3 {
            run(Vec::new());
        }
        let start = run(Vec::new()).0.center();
        run(router
            .event_with_context(&ctx, TouchPhase::Start, 1, start, 0.)
            .0);
        for (i, y) in [20., 40., 60., 80.].into_iter().enumerate() {
            run(router
                .event_with_context(
                    &ctx,
                    TouchPhase::Move,
                    1,
                    start - egui::vec2(0., y),
                    0.02 * (i + 1) as f64,
                )
                .0);
        }
        let (_, offset) = run(router
            .event_with_context(&ctx, TouchPhase::End, 1, start - egui::vec2(0., 80.), 0.1)
            .0);
        assert!(
            offset > 40.,
            "Widget kind {kind} blocked scrolling: {offset}"
        );
        assert_eq!(value, 7.);
        assert_eq!(text, "Original");
        assert_eq!(clicks, 0);
        assert!(
            !ctx.wants_keyboard_input(),
            "Scrolling focused a text field"
        );
    }
}

#[test]
fn panel_taps_and_horizontal_editing_still_reach_controls() {
    use crate::scroll::TouchScroll as _;
    for floating in [false, true] {
        let ctx = egui::Context::default();
        ctx.set_style(crate::theme::style());
        let mut router = TouchRouter::default();
        let mut value = 7.;
        let mut text = "Text".to_owned();
        let mut clicks = 0;
        let mut button_rect = egui::Rect::NOTHING;
        let mut number_rect = button_rect;
        let mut text_rect = button_rect;
        let mut time = 0.;
        let mut run = |events| {
            time += 0.02;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        Pos2::ZERO,
                        egui::vec2(500., 360.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    crate::scroll::begin_frame(ctx);
                    let mut content = |ui: &mut egui::Ui| {
                        egui::ScrollArea::vertical()
                            .id_source("panel-controls")
                            .max_height(180.)
                            .touch_show(ui, |ui| {
                                let r = ui.button("Activate");
                                button_rect = r.rect;
                                clicks += usize::from(r.clicked());
                                number_rect = ui
                                    .add(crate::numeric::Value::new(&mut value).speed(1.))
                                    .rect;
                                text_rect = ui.text_edit_singleline(&mut text).rect;
                                ui.add_space(500.);
                            });
                    };
                    if floating {
                        egui::Window::new("Floating properties")
                            .fixed_pos(egui::pos2(50., 40.))
                            .show(ctx, &mut content);
                    } else {
                        egui::CentralPanel::default().show(ctx, &mut content);
                    }
                },
            );
            (button_rect, number_rect, text_rect, clicks, value)
        };
        for _ in 0..3 {
            run(Vec::new());
        }
        let start = run(Vec::new()).0.center();
        run(router
            .event_with_context(&ctx, TouchPhase::Start, 1, start, 0.)
            .0);
        assert_eq!(
            run(router
                .event_with_context(&ctx, TouchPhase::End, 1, start, 0.1)
                .0)
            .3,
            1
        );
        let start = run(Vec::new()).1.center();
        run(router
            .event_with_context(&ctx, TouchPhase::Start, 1, start, 0.2)
            .0);
        for i in 1..=8 {
            run(router
                .event_with_context(
                    &ctx,
                    TouchPhase::Move,
                    1,
                    start + egui::vec2(i as f32 * 5., 0.),
                    0.2 + i as f64 * 0.02,
                )
                .0);
        }
        let result = run(router
            .event_with_context(&ctx, TouchPhase::End, 1, start + egui::vec2(40., 0.), 0.4)
            .0);
        assert!(
            result.4 > 7.,
            "Horizontal edit was blocked in floating={floating}"
        );
        let start = run(Vec::new()).2.center();
        run(router
            .event_with_context(&ctx, TouchPhase::Start, 1, start, 0.5)
            .0);
        run(router
            .event_with_context(&ctx, TouchPhase::End, 1, start, 0.6)
            .0);
        assert!(
            ctx.wants_keyboard_input(),
            "Text tap did not focus in floating={floating}"
        );
    }
}

#[test]
fn long_combo_popup_scrolls_by_touch_then_accepts_a_tap() {
    use crate::scroll::TouchCombo as _;
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    let mut router = TouchRouter::default();
    let mut selected = 0;
    let mut rect = egui::Rect::NOTHING;
    let mut visible = None;
    let mut time = 0.;
    let mut run = |events| {
        time += 0.02;
        visible = None;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(500., 360.),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| {
                crate::scroll::begin_frame(ctx);
                egui::CentralPanel::default().show(ctx, |ui| {
                    rect = egui::ComboBox::from_id_source("long-choice")
                        .selected_text(format!("Choice {selected}"))
                        .touch_show_ui(ui, |ui| {
                            for i in 0..80 {
                                let r =
                                    ui.selectable_value(&mut selected, i, format!("Choice {i}"));
                                if i > 0
                                    && ui.clip_rect().contains_rect(r.rect)
                                    && visible.is_none()
                                {
                                    visible = Some((i, r.rect));
                                }
                            }
                        })
                        .response
                        .rect;
                });
            },
        );
        (rect, visible, selected)
    };
    for _ in 0..3 {
        run(Vec::new());
    }
    let p = run(Vec::new()).0.center();
    run(router
        .event_with_context(&ctx, TouchPhase::Start, 1, p, 0.)
        .0);
    run(router
        .event_with_context(&ctx, TouchPhase::End, 1, p, 0.02)
        .0);
    let p = run(Vec::new()).1.expect("Open combo popup").1.center();
    let zone = crate::scroll::at(&ctx, p).expect("Registered combo viewport");
    run(router
        .event_with_context(&ctx, TouchPhase::Start, 1, p, 0.1)
        .0);
    run(router
        .event_with_context(&ctx, TouchPhase::Move, 1, p - egui::vec2(0., 70.), 0.15)
        .0);
    let result = run(router
        .event_with_context(&ctx, TouchPhase::End, 1, p - egui::vec2(0., 80.), 0.17)
        .0);
    assert_eq!(result.2, 0, "Popup swipe selected an entry");
    assert!(
        egui::scroll_area::State::load(&ctx, zone.id)
            .unwrap()
            .offset
            .y
            > 40.
    );
    let (expected, rect) = run(Vec::new()).1.unwrap();
    let p = rect.center();
    run(router
        .event_with_context(&ctx, TouchPhase::Start, 1, p, 0.3)
        .0);
    let result = run(router
        .event_with_context(&ctx, TouchPhase::End, 1, p, 0.32)
        .0);
    assert_eq!(result.2, expected, "Popup did not accept the selected row");
}
