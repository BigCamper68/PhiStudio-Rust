use super::*;

#[test]
fn paired_shader_grids_reflow_with_the_inspector() {
    let mut app = app();
    app.add_shader();
    app.transport.beat = 2.;
    let ctx = egui::Context::default();
    let size = vec2(1194., 834.);
    for _ in 0..4 {
        frame(&mut app, &ctx, size, vec![]);
    }
    let original = app.editor.document().as_json().clone();
    let before = crate::grid_input::geometry(app.timeline.area, &app);
    assert!(before.rect.right() < before.event_rect.left());
    assert!((before.rect.left() - app.timeline.area.left() - 38.).abs() < 0.1);
    assert!((app.timeline.area.right() - before.event_rect.right() - 14.).abs() < 0.1);
    let handle = pos2(app.timeline.area.right() - 3., 350.);
    frame(&mut app, &ctx, size, vec![pointer(handle, true)]);
    frame(
        &mut app,
        &ctx,
        size,
        vec![egui::Event::PointerMoved(handle - vec2(80., 0.))],
    );
    frame(
        &mut app,
        &ctx,
        size,
        vec![pointer(handle - vec2(80., 0.), false)],
    );
    let narrow = crate::grid_input::geometry(app.timeline.area, &app);
    assert!(narrow.rect.width() < before.rect.width() - 25.);
    assert_eq!(before.y(3.), narrow.y(3.));
    assert_eq!(app.transport.beat, 2.);
    let out = frame(&mut app, &ctx, size, vec![]);
    for label in ["Notes", "Lock X"] {
        assert!(
            !out.shapes.iter().any(
                |s| matches!(&s.1,egui::Shape::Text(t) if t.galley.job.text==label && t.pos.y > 40.)
            ),
            "Unexpected note-grid UI: {label}"
        );
    }
    app.windows.sidebar = false;
    for _ in 0..3 {
        frame(&mut app, &ctx, size, vec![]);
    }
    let wide = crate::grid_input::geometry(app.timeline.area, &app);
    assert!(wide.rect.width() > narrow.rect.width() + 100.);
    let out = frame(&mut app, &ctx, size, vec![]);
    assert!(
        (text_position(&out, "X 1").x - wide.rect.left() - surface::EFFECT_WIDTH / 2.).abs() < 2.
    );
    assert_eq!(app.editor.document().as_json(), &original);
}

#[test]
fn preview_header_style_and_size_are_shared_by_all_workspaces() {
    for size in [vec2(873., 393.), vec2(1194., 834.)] {
        let mut header = None;
        let mut preview_height: Option<f32> = None;
        for mode in [RightMode::Events, RightMode::Storyboard, RightMode::Shaders] {
            let mut app = app();
            app.advanced.right_mode = mode;
            if mode == RightMode::Shaders {
                app.add_shader();
            }
            app.windows.scroll_to_properties = false;
            let ctx = egui::Context::default();
            ctx.set_style(crate::theme::style());
            let mut out = frame(&mut app, &ctx, size, vec![]);
            for _ in 0..5 {
                out = frame(&mut app, &ctx, size, vec![]);
            }
            let format = out
                .shapes
                .iter()
                .find_map(|s| match &s.1 {
                    egui::Shape::Text(t) if t.galley.job.text == "PREVIEW" => {
                        Some(t.galley.job.sections[0].format.clone())
                    }
                    _ => None,
                })
                .expect("Shared Preview header");
            if let Some((font, color)) = &header {
                assert_eq!(&format.font_id, font);
                assert_eq!(&format.color, color);
            } else {
                header = Some((format.font_id, format.color));
            }
            let height = app.preview.rect.height();
            assert!(height > 120., "Preview is too small on {size:?}: {height}");
            if let Some(previous) = preview_height {
                assert!(
                    (height - previous).abs() < 8.,
                    "Preview differs between workspaces"
                );
            } else {
                preview_height = Some(height);
            }
            assert!(
                !out.shapes.iter().any(
                    |s| matches!(&s.1, egui::Shape::Text(t) if t.galley.job.text == "Effects")
                )
            );
        }
    }
}

#[test]
fn shader_inspector_stays_inside_the_screen_with_the_native_theme() {
    let mut app = app();
    app.shaders.preset = 9;
    app.add_shader();
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    let size = vec2(873., 393.);
    for page in [
        ShaderPage::Effect,
        ShaderPage::Parameters,
        ShaderPage::Animation,
        ShaderPage::Effect,
    ] {
        app.shader_navigate(page, Some("color".into()));
        if page == ShaderPage::Animation {
            app.shader_create_animation();
        }
        for _ in 0..6 {
            frame(&mut app, &ctx, size, vec![]);
        }
        let panel =
            egui::containers::panel::PanelState::load(&ctx, egui::Id::new("context")).unwrap();
        assert!(
            panel.rect.right() <= size.x + 0.5,
            "Inspector overflow on {page:?}: {:?}",
            panel.rect
        );
        assert!((panel.rect.left() - app.timeline.area.right()).abs() < 0.5);
    }
}

#[test]
fn inspector_resize_uses_down_and_final_touch_release_when_events_are_batched() {
    for mode in [RightMode::Events, RightMode::Shaders] {
        let mut app = app();
        app.advanced.right_mode = mode;
        app.transport.beat = 2.;
        let ctx = egui::Context::default();
        ctx.set_style(crate::theme::style());
        let size = vec2(873., 393.);
        for _ in 0..4 {
            frame(&mut app, &ctx, size, vec![]);
        }
        let original = app.editor.document().as_json().clone();
        let before = app.timeline.area.right();
        let a = pos2(before - 1., size.y * 0.5);
        let b = a - vec2(70., 0.);
        frame(
            &mut app,
            &ctx,
            size,
            vec![
                pointer(a, true),
                egui::Event::PointerMoved(a - vec2(30., 0.)),
                pointer(b, false),
                egui::Event::PointerGone,
            ],
        );
        assert!(app.timeline.area.right() < before - 50.);
        assert!((app.timeline.area.right() - b.x).abs() < 1.);
        assert_eq!(app.transport.beat, 2.);
        assert_eq!(app.editor.document().as_json(), &original);
        app.set_tool(crate::workbench::Tool::Event);
        let a = pos2(app.timeline.area.right() - 1., size.y * 0.5);
        let b = a + vec2(40., 0.);
        frame(
            &mut app,
            &ctx,
            size,
            vec![pointer(a, true), egui::Event::PointerMoved(b)],
        );
        let release = b + vec2(30., 0.);
        frame(
            &mut app,
            &ctx,
            size,
            vec![pointer(release, false), egui::Event::PointerGone],
        );
        assert!((app.timeline.area.right() - release.x).abs() < 1.);
        assert_eq!(app.editor.document().as_json(), &original);
        assert!(!app.timeline.input.is_scrubbing());
    }
}
