use super::*;

#[test]
fn fling_at_a_nested_list_boundary_does_not_move_its_parent() {
    use crate::scroll::TouchScroll as _;
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    let mut router = TouchRouter::default();
    let mut time = 0.;
    let mut viewport = egui::Rect::NOTHING;
    let mut parent_offset = 0.;
    let mut child_offset = 0.;
    let mut run = |events| {
        time += 1. / 60.;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(400., 300.),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| {
                crate::scroll::begin_frame(ctx);
                egui::CentralPanel::default().show(ctx, |ui| {
                    let parent = egui::ScrollArea::vertical()
                        .id_source("parent-list")
                        .touch_show(ui, |ui| {
                            ui.add_space(30.);
                            let child = egui::ScrollArea::vertical()
                                .id_source("nested-list")
                                .max_height(100.)
                                .touch_show(ui, |ui| {
                                    for _ in 0..8 {
                                        ui.button("Nested row").clicked();
                                    }
                                });
                            child_offset = child.state.offset.y;
                            viewport = child.inner_rect;
                            ui.add_space(650.);
                        });
                    parent_offset = parent.state.offset.y;
                });
            },
        );
        (parent_offset, child_offset, viewport)
    };
    for _ in 0..4 {
        run(Vec::new());
    }
    let p = run(Vec::new()).2.center();
    run(router
        .event_with_context(&ctx, TouchPhase::Start, 1, p, 0.)
        .0);
    run(router
        .event_with_context(&ctx, TouchPhase::Move, 1, p - egui::vec2(0., 50.), 0.01)
        .0);
    run(router
        .event_with_context(&ctx, TouchPhase::End, 1, p - egui::vec2(0., 70.), 0.02)
        .0);
    for i in 1..=120 {
        let mut events = router.frame_start();
        events.extend(router.scroll_momentum(&ctx, 0.02 + i as f64 / 60.));
        run(events);
    }
    let (parent, child, _) = run(Vec::new());
    assert!(
        parent < 0.5,
        "Nested fling escaped into the parent: {parent}"
    );
    assert!(child > 100., "Nested fling reset its list: {child}");
}

#[test]
fn fast_panel_fling_keeps_its_offset_after_inertia_finishes() {
    use crate::scroll::TouchScroll as _;
    for floating in [false, true] {
        for (focus, release_y, direction) in [
            (false, 170., 1.),
            (true, 170., 1.),
            (false, 137., 1.),
            (true, 137., 1.),
            (false, 137., -1.),
            (true, 137., -1.),
        ] {
            let ctx = egui::Context::default();
            ctx.set_style(crate::theme::style());
            let mut router = TouchRouter::default();
            let mut text = "Keep this value".to_owned();
            let mut viewport = egui::Rect::NOTHING;
            let mut field = viewport;
            let mut offset = 0.;
            let mut time = 0.;
            let mut run = |events| {
                time += 1. / 60.;
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
                            let out = egui::ScrollArea::vertical()
                                .id_source("fling-regression")
                                .max_height(220.)
                                .touch_show(ui, |ui| {
                                    field = ui.text_edit_singleline(&mut text).rect;
                                    for _ in 0..65 {
                                        ui.button("Scrollable row").clicked();
                                    }
                                });
                            offset = out.state.offset.y;
                            viewport = out.inner_rect;
                        };
                        if floating {
                            egui::Window::new("Panel fling")
                                .fixed_pos(egui::pos2(30., 30.))
                                .show(ctx, &mut content);
                        } else {
                            egui::CentralPanel::default().show(ctx, &mut content);
                        }
                    },
                );
                (offset, viewport, field)
            };
            for _ in 0..4 {
                run(Vec::new());
            }
            if focus {
                let p = run(Vec::new()).2.center();
                run(router
                    .event_with_context(&ctx, TouchPhase::Start, 1, p, 0.)
                    .0);
                run(router
                    .event_with_context(&ctx, TouchPhase::End, 1, p, 0.02)
                    .0);
            }
            if direction < 0. {
                let anchor = run(Vec::new()).1.center();
                let zone = crate::scroll::at(&ctx, anchor).unwrap();
                let mut state = egui::scroll_area::State::load(&ctx, zone.id).unwrap();
                state.offset.y = 800.;
                state.store(&ctx, zone.id);
            }
            let (initial, viewport, _) = run(Vec::new());
            let p = viewport.center();
            run(router
                .event_with_context(&ctx, TouchPhase::Start, 1, p, 1.)
                .0);
            run(router
                .event_with_context(
                    &ctx,
                    TouchPhase::Move,
                    1,
                    p - egui::vec2(0., 140. * direction),
                    1.02,
                )
                .0);
            let mut previous = run(router
                .event_with_context(
                    &ctx,
                    TouchPhase::End,
                    1,
                    p - egui::vec2(0., release_y * direction),
                    1.021,
                )
                .0)
            .0;
            assert!((previous - initial) * direction > 100.);
            for i in 1..=150 {
                let mut events = router.frame_start();
                events.extend(router.scroll_momentum(&ctx, 1.021 + i as f64 / 60.));
                let current = run(events).0;
                assert!(
                    (current - previous) * direction >= -0.5,
                    "Offset jumped backward after fling: {previous} -> {current}, frame {i}, floating={floating}, focus={focus}"
                );
                previous = current;
            }
            assert!(
                (previous - initial) * direction > 300.,
                "Fling lost its position"
            );
            assert_eq!(text, "Keep this value");
        }
    }
}

#[test]
fn panel_scroll_inertia_stops_on_new_touch_or_suspend() {
    use crate::scroll::TouchScroll as _;
    let ctx = egui::Context::default();
    let mut router = TouchRouter::default();
    let mut offset = 0.;
    let mut zone = egui::Rect::NOTHING;
    let mut time = 0.;
    let mut run = |events| {
        time += 0.02;
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
                    let out = egui::ScrollArea::vertical().touch_show(ui, |ui| {
                        for _ in 0..50 {
                            ui.button("Item").clicked();
                        }
                    });
                    offset = out.state.offset.y;
                    zone = out.inner_rect;
                });
            },
        );
        (offset, zone)
    };
    for _ in 0..3 {
        run(Vec::new());
    }
    let p = run(Vec::new()).1.center();
    assert!(
        crate::scroll::at(&ctx, p).is_some(),
        "Unregistered panel at {p:?}"
    );
    run(router
        .event_with_context(&ctx, TouchPhase::Start, 1, p, 0.)
        .0);
    run(router
        .event_with_context(&ctx, TouchPhase::Move, 1, p - egui::vec2(0., 50.), 0.05)
        .0);
    let before = run(router
        .event_with_context(&ctx, TouchPhase::End, 1, p - egui::vec2(0., 60.), 0.06)
        .0)
    .0;
    for i in 1..=5 {
        run(router.scroll_momentum(&ctx, 0.06 + i as f64 * 0.02));
    }
    let after = run(Vec::new()).0;
    assert!(
        after > before + 15.,
        "Inertia did not advance the panel: {before} -> {after}"
    );
    run(router
        .event_with_context(&ctx, TouchPhase::Start, 2, p, 0.3)
        .0);
    assert!(router.scroll_momentum(&ctx, 0.4).is_empty());
    run(router.cancel());
    assert!(router.scroll_momentum(&ctx, 0.5).is_empty());
}
