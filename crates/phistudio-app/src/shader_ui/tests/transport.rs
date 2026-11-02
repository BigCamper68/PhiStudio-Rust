use super::*;

#[test]
fn storyboard_and_shaders_share_touch_pan_and_pinch_anchors() {
    let mut results = Vec::new();
    for mode in [RightMode::Events, RightMode::Storyboard, RightMode::Shaders] {
        let mut app = app();
        app.advanced.right_mode = mode;
        app.beat = 4.;
        app.playing = true;
        let original = app.editor.document().as_json().clone();
        let grid =
            crate::grid_input::geometry(Rect::from_min_max(pos2(0., 80.), pos2(900., 700.)), &app);
        let area = grid.full_rect();
        let a = pos2(grid.event_rect.center().x, 350.);
        let b = a + vec2(80., 0.);
        crate::grid_input::handle(
            &mut app,
            &[
                pointer(a, true),
                egui::Event::PointerMoved(a + vec2(0., 48.)),
                pointer(a + vec2(0., 96.), false),
            ],
            grid,
            area,
            |_| true,
            false,
        );
        assert!(app.playing && !app.is_scrubbing());
        assert!((app.beat - 5.).abs() < 0.001);
        let grid =
            crate::grid_input::geometry(Rect::from_min_max(pos2(0., 80.), pos2(900., 700.)), &app);
        let mut router = crate::touch::TouchRouter::default();
        for (phase, id, pos) in [
            (egui::TouchPhase::Start, 1, a),
            (egui::TouchPhase::Start, 2, b),
            (egui::TouchPhase::Move, 2, b + vec2(80., 40.)),
            (egui::TouchPhase::End, 1, a),
        ] {
            let (events, canceled) = router.event(phase, id, pos);
            crate::grid_input::handle(&mut app, &events, grid, area, |_| true, canceled);
            assert!(app.playing && app.is_scrubbing());
        }
        let (events, canceled) = router.event(egui::TouchPhase::End, 2, b + vec2(80., 40.));
        crate::grid_input::handle(&mut app, &events, grid, area, |_| true, canceled);
        assert!(app.playing && !app.is_scrubbing());
        assert_eq!(app.editor.document().as_json(), &original);
        results.push((app.beat, app.zoom));
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[1], results[2]);
}

#[test]
fn shader_keyframe_grid_preserves_the_independent_clock_and_unknown_fields() {
    let mut app = app();
    app.add_shader();
    assert!(app.shader_command(ShaderAction::Tempo(json!(60.))));
    app.refresh_shaders();
    app.shaders.draft.as_mut().unwrap().raw["vars"]["power"] = json!([
        {"startTime":[0,0,1],"endTime":[4,0,1],"start":0.2,"end":0.8,"easingType":2,"custom":"keep"}
    ]);
    assert!(app.flush_shader_draft());
    app.refresh_shaders();
    app.shaders.track = Some("power".into());
    app.shaders.page = ShaderPage::Animation;
    app.shaders.animation = app
        .shaders
        .draft
        .as_ref()
        .and_then(|d| animation_draft(&d.raw, "power", 0));
    let original = app.editor.document().as_json().clone();
    let grid =
        crate::grid_input::geometry(Rect::from_min_max(pos2(0., 80.), pos2(900., 1100.)), &app);
    let a = pos2(parameter_x(&app, grid, "power"), grid.y(2.));
    let b = a - vec2(0., 96.);
    let count = app.editor.history_rows().len();
    crate::grid_input::handle(
        &mut app,
        &[
            pointer(a, true),
            egui::Event::PointerMoved(b),
            pointer(b, false),
        ],
        grid,
        grid.full_rect(),
        |_| true,
        false,
    );
    let raw = &app.editor.document().as_json()[EXTRA_KEY]["effects"][0]["vars"]["power"][0];
    assert_eq!(raw["startTime"], json!([1, 0, 1]));
    assert_eq!(raw["endTime"], json!([5, 0, 1]));
    assert_eq!(raw["custom"], "keep");
    assert_eq!(app.editor.history_rows().len(), count + 1);
    app.history_step(false);
    app.refresh_shaders();
    assert_eq!(app.editor.document().as_json(), &original);
    app.shader_key(egui::Key::Delete, egui::Modifiers::NONE);
    app.refresh_shaders();
    assert_eq!(app.shaders.effects.len(), 1);
    assert_eq!(app.shaders.effects[0].vars["power"], json!(0.2));
    assert_eq!(app.shaders.page, ShaderPage::Animation);
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &original);
}
