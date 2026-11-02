use super::*;

#[test]
fn preview_tap_enters_fullscreen_and_static_pause_target_returns_without_stopping_playback() {
    for size in [Vec2::new(873., 393.), Vec2::new(393., 873.)] {
        let mut app = Workbench::new(ChartDocument::empty("Fullscreen", 120.).unwrap(), None);
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut app, &ctx, size, vec![], false);
        }
        let original = app.editor.document().as_json().clone();
        app.transport.playing = true;
        let tap = app.preview.rect.center();
        frame(
            &mut app,
            &ctx,
            size,
            vec![pointer(tap, true), pointer(tap, false)],
            false,
        );
        assert!(app.preview.fullscreen);
        frame(&mut app, &ctx, size, vec![], false);
        assert!(app.transport.playing);
        assert!(!app.timeline.area.is_positive());
        let exit = app.fullscreen_exit_rect(app.preview.rect).center();
        frame(
            &mut app,
            &ctx,
            size,
            vec![pointer(exit, true), pointer(exit, false)],
            false,
        );
        assert!(!app.preview.fullscreen);
        assert!(app.transport.playing);
        assert_eq!(app.editor.document().as_json(), &original);
        app.preview.fullscreen = true;
        app.back();
        assert!(!app.preview.fullscreen);
        assert!(app.transport.playing);
    }
}

#[test]
fn holding_fullscreen_return_never_enters_scrub_or_pauses_playback() {
    for playing in [false, true] {
        let mut app = Workbench::new(
            ChartDocument::empty("Return transport", 120.).unwrap(),
            None,
        );
        let ctx = egui::Context::default();
        let size = Vec2::new(1194., 834.);
        app.preview.fullscreen = true;
        app.transport.playing = playing;
        app.transport.beat = 8.;
        for _ in 0..3 {
            frame(&mut app, &ctx, size, vec![], false);
        }
        let exit = app.fullscreen_exit_rect(app.preview.rect).center();
        frame(&mut app, &ctx, size, vec![pointer(exit, true)], false);
        assert!(!app.is_scrubbing());
        assert_eq!(app.transport.playing, playing);
        for _ in 0..5 {
            frame(&mut app, &ctx, size, vec![], false);
            assert!(!app.is_scrubbing());
            assert_eq!(app.transport.playing, playing);
        }
        let outside = exit + egui::vec2(200., 150.);
        frame(
            &mut app,
            &ctx,
            size,
            vec![egui::Event::PointerMoved(outside)],
            false,
        );
        assert!(!app.is_scrubbing());
        assert_eq!(app.transport.beat, 8.);
        frame(&mut app, &ctx, size, vec![pointer(outside, false)], false);
        assert!(app.preview.fullscreen);
        assert_eq!(app.transport.playing, playing);
        frame(&mut app, &ctx, size, vec![pointer(exit, true)], false);
        assert!(!app.is_scrubbing());
        frame(&mut app, &ctx, size, vec![pointer(exit, false)], false);
        assert!(!app.preview.fullscreen);
        assert_eq!(app.transport.playing, playing);
    }
}

#[test]
fn fullscreen_scrubbing_preserves_playback_and_final_release_coordinate() {
    for playing in [false, true] {
        let mut app = Workbench::new(ChartDocument::empty("Preview scrub", 120.).unwrap(), None);
        app.preview.fullscreen = true;
        app.transport.beat = 10.;
        app.transport.playing = playing;
        let ctx = egui::Context::default();
        let size = Vec2::new(873., 393.);
        frame(&mut app, &ctx, size, vec![], false);
        let start = app.preview.rect.center();
        frame(&mut app, &ctx, size, vec![pointer(start, true)], false);
        assert!(app.is_scrubbing());
        frame(
            &mut app,
            &ctx,
            size,
            vec![egui::Event::PointerMoved(start + egui::vec2(3., 96.))],
            false,
        );
        frame(
            &mut app,
            &ctx,
            size,
            vec![pointer(start + egui::vec2(4., 144.), false)],
            false,
        );
        assert!((app.transport.beat - 11.5).abs() < 1e-8);
        assert_eq!(app.transport.playing, playing);
        assert!(!app.is_scrubbing());
        assert!(app.preview.pick.is_none());
        assert!(app.preview.fullscreen);
    }
}

#[test]
fn fullscreen_scrub_locks_its_axis_and_reverses_immediately_at_zero() {
    let size = Vec2::new(873., 393.);
    for playing in [false, true] {
        let mut app = Workbench::new(ChartDocument::empty("Scrub", 120.).unwrap(), None);
        let ctx = egui::Context::default();
        app.preview.fullscreen = true;
        app.transport.playing = playing;
        app.transport.beat = 10.;
        frame(&mut app, &ctx, size, vec![], false);
        let start = app.preview.rect.center();
        let touch = |phase, pos| egui::Event::Touch {
            device_id: egui::TouchDeviceId(0),
            id: egui::TouchId(7),
            phase,
            pos,
            force: 0.,
        };
        let step = (app.options.scroll_speed / app.timeline.zoom) as f64;
        frame(
            &mut app,
            &ctx,
            size,
            vec![touch(egui::TouchPhase::Start, start)],
            false,
        );
        for (x, y) in [(1., 60.), (180., 80.), (160., 20.)] {
            frame(
                &mut app,
                &ctx,
                size,
                vec![touch(egui::TouchPhase::Move, start + Vec2::new(x, y))],
                false,
            );
            assert!((app.transport.beat - (10. + y as f64 * step)).abs() < 1e-5);
            assert_eq!(app.transport.playing, playing);
            assert!(app.is_scrubbing());
        }
        frame(
            &mut app,
            &ctx,
            size,
            vec![touch(egui::TouchPhase::End, start + Vec2::new(100., 40.))],
            false,
        );
        assert!((app.transport.beat - (10. + 40. * step)).abs() < 1e-5);
        assert!(!app.is_scrubbing());
        assert!(app.preview.pick.is_none());
        app.transport.beat = step * 20.;
        frame(
            &mut app,
            &ctx,
            size,
            vec![touch(egui::TouchPhase::Start, start)],
            false,
        );
        frame(
            &mut app,
            &ctx,
            size,
            vec![touch(egui::TouchPhase::Move, start - Vec2::new(0., 100.))],
            false,
        );
        assert_eq!(app.transport.beat, 0.);
        frame(
            &mut app,
            &ctx,
            size,
            vec![touch(egui::TouchPhase::Move, start - Vec2::new(0., 90.))],
            false,
        );
        assert!((app.transport.beat - step * 10.).abs() < 1e-5);
        frame(
            &mut app,
            &ctx,
            size,
            vec![touch(egui::TouchPhase::End, start - Vec2::new(0., 80.))],
            false,
        );
        assert!((app.transport.beat - step * 20.).abs() < 1e-5);
        assert_eq!(app.transport.playing, playing);
    }
}

#[test]
fn fullscreen_multitouch_cancels_only_the_scrub_and_retains_playback_intent() {
    let size = Vec2::new(873., 393.);
    let mut app = Workbench::new(ChartDocument::empty("Multi", 120.).unwrap(), None);
    let ctx = egui::Context::default();
    app.preview.fullscreen = true;
    app.transport.playing = true;
    app.transport.beat = 5.;
    frame(&mut app, &ctx, size, vec![], false);
    let start = app.preview.rect.center();
    let touch = |id, phase| egui::Event::Touch {
        device_id: egui::TouchDeviceId(0),
        id: egui::TouchId(id),
        phase,
        pos: start,
        force: 0.,
    };
    frame(
        &mut app,
        &ctx,
        size,
        vec![touch(1, egui::TouchPhase::Start)],
        false,
    );
    frame(
        &mut app,
        &ctx,
        size,
        vec![touch(2, egui::TouchPhase::Start)],
        true,
    );
    assert!(app.transport.playing && app.is_scrubbing() && app.preview.blocked);
    frame(
        &mut app,
        &ctx,
        size,
        vec![touch(1, egui::TouchPhase::End)],
        false,
    );
    assert!(app.is_scrubbing());
    frame(
        &mut app,
        &ctx,
        size,
        vec![touch(2, egui::TouchPhase::End)],
        false,
    );
    assert!(app.transport.playing && !app.is_scrubbing());
    frame(
        &mut app,
        &ctx,
        size,
        vec![
            touch(3, egui::TouchPhase::Start),
            touch(3, egui::TouchPhase::Cancel),
        ],
        false,
    );
    assert!(!app.transport.playing);
}
