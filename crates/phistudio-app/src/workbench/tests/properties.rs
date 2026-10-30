use super::*;

#[test]
fn background_preview_uses_brightness_and_replaces_docked_and_floating_preview() {
    for (size, brightness, alpha) in [
        (Vec2::new(873., 393.), 0., 0),
        (Vec2::new(873., 393.), 0.2, 51),
        (Vec2::new(873., 393.), 0.5, 128),
        (Vec2::new(1194., 834.), 1., 255),
    ] {
        let mut app = Workbench::new(ChartDocument::empty("Background", 120.).unwrap(), None);
        assert!(!app.options.preview_behind_grid);
        app.preview.texture = Some(egui::TextureId::User(42));
        let ctx = egui::Context::default();
        let original = app.editor.document().as_json().clone();
        app.options.preview_behind_grid = true;
        app.options.preview_grid_brightness = brightness;
        app.preview.floating = true;
        for _ in 0..3 {
            frame(&mut app, &ctx, size, vec![], false);
        }
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ctx| app.ui(ctx, false),
        );
        let images: Vec<_> = out
            .shapes
            .iter()
            .filter_map(|shape| {
                let shape = &shape.1;
                if let egui::Shape::Mesh(mesh) = shape {
                    (mesh.texture_id == egui::TextureId::User(42)).then_some(mesh)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(images.len(), 1, "Docked/floating preview must disappear");
        assert!(
            images[0]
                .vertices
                .iter()
                .all(|v| v.color == egui::Color32::from_white_alpha(alpha))
        );
        assert!(app.grid_area.contains(app.preview.rect.center()));
        let grid = app.multi_line.base.unwrap();
        let surface = grid.full_rect();
        assert!(
            surface.contains_rect(app.preview.rect),
            "Preview must fit below all track/coordinate titles"
        );
        assert!(
            (app.preview.rect.width() / app.preview.rect.height() - app.preview_aspect()).abs()
                < 0.001
        );
        for shape in &out.shapes {
            if let egui::Shape::Mesh(mesh) = &shape.1
                && mesh.texture_id == egui::TextureId::User(42)
            {
                assert!(shape.0.top() >= surface.top());
                assert!(mesh.vertices.iter().all(|v| surface.contains(v.pos)));
            }
        }
        let button = out
            .shapes
            .iter()
            .find_map(|shape| match &shape.1 {
                egui::Shape::Text(t) if t.galley.job.text == "Fullscreen Preview ↗" => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        frame(
            &mut app,
            &ctx,
            size,
            vec![pointer(button, true), pointer(button, false)],
            false,
        );
        assert!(app.preview.fullscreen);
        app.close_fullscreen_preview();
        app.options.preview_behind_grid = false;
        app.preview.floating = false;
        frame(&mut app, &ctx, size, vec![], false);
        assert!(app.preview.rect.left() > app.grid_area.right());
        assert_eq!(app.editor.document().as_json(), &original);
    }
}

#[test]
fn real_properties_keep_their_position_after_a_fast_fling_settles() {
    for floating in [false, true] {
        let mut app = Workbench::new(ChartDocument::empty("Fling", 120.).unwrap(), None);
        app.compact_properties = floating;
        app.select_event(Some(phistudio_core::EventId {
            line: 0,
            layer: 0,
            kind: phistudio_core::EventType::MoveX,
            index: 0,
        }));
        let ctx = egui::Context::default();
        ctx.set_style(crate::theme::style());
        let size = Vec2::new(873., 393.);
        let mut router = crate::touch::TouchRouter::default();
        for _ in 0..5 {
            frame(&mut app, &ctx, size, Vec::new(), false);
        }
        let p = if floating {
            egui::pos2(190., 155.)
        } else {
            egui::pos2(750., 250.)
        };
        let zone = crate::scroll::at(&ctx, p).unwrap();
        for (phase, delta, time) in [
            (egui::TouchPhase::Start, 0., 1.),
            (egui::TouchPhase::Move, 100., 1.01),
            (egui::TouchPhase::End, 160., 1.02),
        ] {
            let (events, canceled) =
                router.event_with_context(&ctx, phase, 1, p - egui::vec2(0., delta), time);
            frame(&mut app, &ctx, size, events, canceled);
        }
        let mut last = egui::scroll_area::State::load(&ctx, zone.id)
            .unwrap()
            .offset
            .y;
        for i in 1..=150 {
            let mut events = router.frame_start();
            events.extend(router.scroll_momentum(&ctx, 1.02 + i as f64 / 60.));
            frame(&mut app, &ctx, size, events, false);
            let current = egui::scroll_area::State::load(&ctx, zone.id)
                .unwrap()
                .offset
                .y;
            assert!(
                current + 0.5 >= last,
                "Real properties reset: {last} -> {current}, frame {i}, floating={floating}"
            );
            last = current;
        }
    }
}

#[test]
fn a_floating_window_consumes_taps_over_the_grid() {
    let mut app = Workbench::new(ChartDocument::empty("Test", 120.).unwrap(), None);
    app.preview.floating = true;
    app.show_context = false;
    app.tool = Tool::Place(NoteKind::Tap);
    let ctx = egui::Context::default();
    let size = Vec2::new(873., 393.);
    for _ in 0..3 {
        frame(&mut app, &ctx, size, vec![], false);
    }
    let pos = app.preview.rect.center();
    assert!(app.grid_area.contains(pos));
    frame(
        &mut app,
        &ctx,
        size,
        vec![egui::Event::PointerMoved(pos), pointer(pos, true)],
        false,
    );
    frame(&mut app, &ctx, size, vec![pointer(pos, false)], false);
    assert_eq!(app.editor.document().note_count(), 0);
}

#[test]
fn selecting_objects_scrolls_to_properties_and_floating_forms_remove_docked_content() {
    let mut app = Workbench::new(ChartDocument::empty("Dock", 120.).unwrap(), None);
    app.execute(Command::AddNote {
        line: 0,
        note: Note::default(),
    });
    let ctx = egui::Context::default();
    let size = Vec2::new(873., 393.);
    for _ in 0..3 {
        frame(&mut app, &ctx, size, vec![], false);
    }
    app.select(Some(NoteId { line: 0, index: 0 }));
    for _ in 0..3 {
        frame(&mut app, &ctx, size, vec![], false);
    }
    let zone = crate::scroll::at(&ctx, egui::pos2(800., 180.)).unwrap();
    assert!(
        egui::scroll_area::State::load(&ctx, zone.id)
            .unwrap()
            .offset
            .y
            > 100.
    );
    let headers = |app: &mut Workbench| {
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ctx| app.ui(ctx, false),
        );
        out.shapes
            .into_iter()
            .filter_map(|s| {
                if let egui::Shape::Text(t) = s.1 {
                    (t.galley.job.text == "Start").then_some(t.pos.x)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(headers(&mut app).len(), 1);
    app.compact_properties = true;
    for _ in 0..3 {
        frame(&mut app, &ctx, size, vec![], false);
    }
    let labels = headers(&mut app);
    assert_eq!(labels.len(), 1, "Properties must be drawn once");
    assert!(labels[0] < 600., "Docked form remained visible");
    app.compact_properties = false;
    app.properties_scroll = true;
    for _ in 0..3 {
        frame(&mut app, &ctx, size, vec![], false);
    }
    let labels = headers(&mut app);
    assert_eq!(labels.len(), 1);
    assert!(labels[0] > 600.);
}
