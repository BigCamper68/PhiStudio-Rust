use super::*;

#[test]
fn compact_and_tablet_workspace_switch_add_undo_and_return_use_real_widgets() {
    for size in [vec2(873., 393.), vec2(1194., 834.)] {
        let mut app = app();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut app, &ctx, size, vec![]);
        }
        let out = frame(&mut app, &ctx, size, vec![]);
        assert!(app.timeline.area.is_positive());
        assert_eq!(
            out.shapes
                .iter()
                .filter(|s| matches!(&s.1, egui::Shape::Text(t) if t.galley.job.text == "+ Shader"))
                .count(),
            1
        );
        let add = text_position(&out, "+ Shader");
        click(&mut app, &ctx, size, add);
        let grid = crate::grid_input::geometry(app.timeline.area, &app);
        let start = pos2(grid.rect.left() + surface::EFFECT_WIDTH * 0.5, grid.y(0.25));
        let end = pos2(start.x, grid.y(1.25));
        frame(&mut app, &ctx, size, vec![pointer(start, true)]);
        frame(
            &mut app,
            &ctx,
            size,
            vec![egui::Event::PointerMoved(end), pointer(end, false)],
        );
        assert_eq!(app.shaders.effects.len(), 1);
        assert!(app.editor.is_dirty());
        app.shader_key(egui::Key::Z, egui::Modifiers::COMMAND);
        frame(&mut app, &ctx, size, vec![]);
        assert!(app.shaders.effects.is_empty());
        assert!(!app.editor.is_dirty());
        app.shader_key(egui::Key::Y, egui::Modifiers::COMMAND);
        frame(&mut app, &ctx, size, vec![]);
        assert_eq!(app.shaders.effects.len(), 1);
        let out = frame(&mut app, &ctx, size, vec![]);
        let selector = text_position(&out, "Shaders");
        click(&mut app, &ctx, size, selector);
        let out = frame(&mut app, &ctx, size, vec![]);
        let events = text_position(&out, "Events");
        click(&mut app, &ctx, size, events);
        assert!(app.advanced.right_mode == RightMode::Events);
    }
}

#[test]
fn ruler_scrub_keeps_playback_intent_and_uses_final_release_position() {
    let mut app = app();
    let ctx = egui::Context::default();
    let size = vec2(1194., 834.);
    app.add_shader();
    for _ in 0..3 {
        frame(&mut app, &ctx, size, vec![]);
    }
    let grid = crate::grid_input::geometry(app.timeline.area, &app);
    let pos = pos2(grid.rect.left() - 12., grid.y(2.));
    app.transport.playing = true;
    frame(
        &mut app,
        &ctx,
        size,
        vec![egui::Event::PointerMoved(pos), pointer(pos, true)],
    );
    frame(
        &mut app,
        &ctx,
        size,
        vec![egui::Event::PointerMoved(pos + vec2(0., 48.))],
    );
    assert!(app.transport.playing && app.is_scrubbing());
    let before = app.transport.beat;
    let release = pos + vec2(0., 96.);
    frame(&mut app, &ctx, size, vec![pointer(release, false)]);
    assert!(app.transport.playing && !app.is_scrubbing());
    assert!(
        (app.transport.beat - before - 0.5).abs() < 0.01,
        "final release must add another half beat"
    );
}

#[test]
fn vignette_color_radius_and_animation_have_visible_return_paths() {
    for size in [vec2(873., 393.), vec2(1194., 834.)] {
        let mut app = app();
        app.shaders.preset = PRESETS.iter().position(|p| *p == "vignette").unwrap();
        app.add_shader();
        let ctx = egui::Context::default();
        for _ in 0..4 {
            frame(&mut app, &ctx, size, vec![]);
        }
        let handle = pos2(app.timeline.area.right() - 3., size.y * 0.5);
        frame(&mut app, &ctx, size, vec![pointer(handle, true)]);
        frame(
            &mut app,
            &ctx,
            size,
            vec![egui::Event::PointerMoved(handle - vec2(200., 0.))],
        );
        frame(
            &mut app,
            &ctx,
            size,
            vec![pointer(handle - vec2(200., 0.), false)],
        );
        let original = app.editor.document().as_json().clone();
        choose_page(&mut app, &ctx, size, "Parameters");
        assert_eq!(app.shaders.page, ShaderPage::Parameters);
        assert_eq!(app.shaders.track.as_deref(), Some("color"));
        let out = frame(&mut app, &ctx, size, vec![]);
        for s in &out.shapes {
            if let egui::Shape::Text(t) = &s.1
                && t.galley.job.text == "Back to effect"
            {
                assert!(
                    s.0.contains_rect(Rect::from_min_size(t.pos, t.galley.size())),
                    "Back button was clipped at {size:?}"
                );
            }
        }
        click(&mut app, &ctx, size, text_position(&out, "color"));
        let out = frame(&mut app, &ctx, size, vec![]);
        let radius = out
            .shapes
            .iter()
            .rev()
            .find_map(|s| match &s.1 {
                egui::Shape::Text(t) if t.galley.job.text == "radius" => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("Parameter choice in popup");
        click(&mut app, &ctx, size, radius);
        assert_eq!(app.shaders.track.as_deref(), Some("radius"));
        assert!(app.shaders.animation_track().is_none());
        let out = frame(&mut app, &ctx, size, vec![]);
        click(&mut app, &ctx, size, text_position(&out, "Back to effect"));
        assert_eq!(app.shaders.page, ShaderPage::Effect);
        assert!(app.shaders.animation_track().is_none());
        assert_eq!(app.editor.document().as_json(), &original);
        choose_page(&mut app, &ctx, size, "Animation");
        assert_eq!(app.shaders.page, ShaderPage::Animation);
        for _ in 0..20 {
            frame(&mut app, &ctx, size, vec![]);
        }
        let out = frame(&mut app, &ctx, size, vec![]);
        click(
            &mut app,
            &ctx,
            size,
            text_position(&out, "Create animation"),
        );
        assert!(app.shaders.animation.is_some());
        assert!(is_animated(
            &app.editor.document().as_json()[EXTRA_KEY]["effects"][0]["vars"]["radius"]
        ));
        for _ in 0..20 {
            frame(&mut app, &ctx, size, vec![]);
        }
        app.shaders.animation.as_mut().unwrap().end = "unfinished".into();
        let out = frame(&mut app, &ctx, size, vec![]);
        click(&mut app, &ctx, size, text_position(&out, "Back to effect"));
        assert_eq!(app.shaders.page, ShaderPage::Effect);
        assert!(app.shaders.animation_track().is_none());
        assert!(!app.close_requested);
        assert!(app.error.is_none());
        app.history_step(false);
        app.refresh_shaders();
        assert_eq!(app.editor.document().as_json(), &original);
    }
}

#[test]
fn native_back_unwinds_animation_parameter_and_workspace_without_closing_the_app() {
    let mut app = app();
    app.shaders.preset = 9;
    app.add_shader();
    app.shader_navigate(ShaderPage::Animation, Some("radius".into()));
    app.back();
    assert_eq!(app.shaders.page, ShaderPage::Parameters);
    app.back();
    assert_eq!(app.shaders.page, ShaderPage::Effect);
    app.back();
    assert!(app.advanced.right_mode == RightMode::Events);
    assert!(!app.close_requested);
}

#[test]
fn parameter_refresh_keeps_the_selected_keyframe_after_a_commit() {
    let mut app = app();
    app.shaders.preset = 9;
    app.add_shader();
    app.shaders.draft.as_mut().unwrap().raw["vars"]["radius"] = json!([
        {"startTime":[0,0,1],"endTime":[1,0,1],"start":1.,"end":2.,"easingType":1},
        {"startTime":[1,0,1],"endTime":[2,0,1],"start":2.,"end":3.,"easingType":1}
    ]);
    assert!(app.flush_shader_draft());
    app.refresh_shaders();
    app.shader_navigate(ShaderPage::Animation, Some("radius".into()));
    app.shaders.animation = app
        .shaders
        .draft
        .as_ref()
        .and_then(|d| animation_draft(&d.raw, "radius", 1));
    app.shaders.animation.as_mut().unwrap().raw["end"] = json!(4.);
    assert!(app.flush_shader_draft());
    app.refresh_shaders();
    assert_eq!(app.shaders.animation.as_ref().unwrap().index, 1);
    assert_eq!(app.shaders.animation.as_ref().unwrap().raw["end"], 4.);
}

#[test]
fn shader_bpm_grid_and_transport_keep_the_same_playback_time() {
    let mut app = app();
    app.add_shader();
    assert!(app.shader_command(ShaderAction::Tempo(json!(60.))));
    app.refresh_shaders();
    app.set_grid_beat(3.);
    assert_eq!(app.grid_beat(), 3.);
    assert_eq!(app.transport.beat, 6.);
    assert_eq!(app.grid_bpm(), 60.);
    let seconds = app.editor.document().tempo().seconds_at(app.transport.beat);
    assert_eq!(seconds, 3.);
    app.shader_navigate(ShaderPage::Tempo, None);
    let original = app.editor.document().as_json().clone();
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&mut app, &ctx, vec2(873., 393.), vec![]);
    }
    assert_eq!(app.editor.document().as_json(), &original);
    app.shader_navigate(ShaderPage::Effect, None);
    assert_eq!(
        app.editor.document().tempo().seconds_at(app.transport.beat),
        seconds
    );
}
