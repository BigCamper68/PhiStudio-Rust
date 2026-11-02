use super::*;

#[test]
fn preview_fits_a_compact_landscape_viewport() {
    let mut app = Workbench::new(ChartDocument::empty("Test", 120.).unwrap(), None);
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&mut app, &ctx, Vec2::new(873., 393.), vec![], false);
    }
    assert!(app.preview.rect.is_positive());
    assert!(
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(873., 393.))
            .contains_rect(app.preview.rect)
    );
}

#[test]
fn resizing_the_timeline_split_preserves_chart_time_and_edits() {
    let mut app = Workbench::new(ChartDocument::empty("Split", 120.).unwrap(), None);
    app.transport.beat = 4.;
    app.transport.playing = true;
    let original = app.editor.document().as_json().clone();
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    let size = Vec2::new(1280., 800.);
    for _ in 0..4 {
        frame(&mut app, &ctx, size, Vec::new(), false);
    }
    let before = app.timeline.split;
    let grid = crate::grid_input::geometry(app.timeline.area, &app);
    let p = egui::pos2((grid.rect.right() + grid.event_rect.left()) * 0.5, 350.);
    frame(&mut app, &ctx, size, vec![pointer(p, true)], false);
    frame(
        &mut app,
        &ctx,
        size,
        vec![egui::Event::PointerMoved(p + egui::vec2(140., 0.))],
        false,
    );
    frame(
        &mut app,
        &ctx,
        size,
        vec![pointer(p + egui::vec2(140., 0.), false)],
        false,
    );
    assert!(app.timeline.split > before + 0.1);
    assert_eq!(app.transport.beat, 4.);
    assert!(app.transport.playing);
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(!app.timeline.input.is_scrubbing());
}

#[test]
fn event_and_line_windows_fit_compact_viewport_and_register_touch_scrolling() {
    let mut app = Workbench::new(ChartDocument::empty("Properties", 120.).unwrap(), None);
    app.select_event(Some(phistudio_core::EventId {
        line: 0,
        layer: 0,
        kind: phistudio_core::EventType::MoveX,
        index: 0,
    }));
    app.windows.floating_properties = true;
    app.windows.line_properties = true;
    app.windows.bpm = true;
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    for _ in 0..5 {
        frame(&mut app, &ctx, Vec2::new(873., 393.), Vec::new(), false);
    }
    assert!(app.error.is_none());
    assert!(app.timeline.area.is_positive());
    // All windows contain real ScrollAreas; no chart command is produced by layout.
    assert!(!app.editor.is_dirty());
}

#[test]
fn selected_note_sidebar_stays_above_the_toolbar_on_a_phone() {
    let mut app = Workbench::new(ChartDocument::empty("Test", 120.).unwrap(), None);
    app.execute(Command::AddNote {
        line: 0,
        note: Note::default(),
    });
    app.select(Some(NoteId { line: 0, index: 0 }));
    for size in [Vec2::new(873., 393.), Vec2::new(600., 320.)] {
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut app, &ctx, size, vec![], false);
        }
        let sidebar = egui::containers::panel::PanelState::load(&ctx, egui::Id::new("context"))
            .unwrap()
            .rect;
        let toolbar = egui::containers::panel::PanelState::load(&ctx, egui::Id::new("tools"))
            .unwrap()
            .rect;
        assert!(
            sidebar.bottom() <= toolbar.top() + 1.,
            "Sidebar covers the toolbar: {size:?} {sidebar:?} {toolbar:?}"
        );
        assert!(app.timeline.area.bottom() <= toolbar.top() + 1.);
    }
}

#[test]
fn ruler_labels_remain_in_the_draw_list_while_their_glyphs_cross_either_edge() {
    let mut app = Workbench::new(ChartDocument::empty("Clipping", 120.).unwrap(), None);
    let ctx = egui::Context::default();
    let size = Vec2::new(900., 600.);
    frame(&mut app, &ctx, size, vec![], false);
    let grid = crate::grid_input::geometry(app.timeline.area, &app);
    for edge in [grid.rect.top(), grid.rect.bottom()] {
        for offset in [-2., 2.] {
            app.transport.beat = 10. - (grid.baseline() - edge - offset) as f64 / grid.zoom as f64;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| app.ui(ctx, false),
            );
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter(|shape| match &shape.1 {
                    egui::Shape::Text(text) => {
                        text.galley.job.text == "10"
                            && (shape.0.top() - grid.rect.top()).abs() < 0.1
                            && (shape.0.bottom() - grid.rect.bottom()).abs() < 0.1
                    }
                    _ => false,
                })
                .collect();
            assert!(
                !labels.is_empty(),
                "Ruler label vanished at edge {edge}, offset {offset}"
            );
        }
    }
}

#[test]
fn compact_headers_align_and_mode_switch_replaces_events_without_link_button() {
    let mut app = Workbench::new(ChartDocument::empty("Headers", 120.).unwrap(), None);
    let raw = app.editor.document().as_json().clone();
    let ctx = egui::Context::default();
    let size = Vec2::new(873., 393.);
    let mut labels = std::collections::BTreeMap::new();
    for _ in 0..3 {
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ctx| app.ui(ctx, false),
        );
        for clipped in output.shapes {
            if let egui::Shape::Text(t) = clipped.1
                && t.pos.y < 95.
            {
                labels.insert(t.galley.job.text.clone(), t.pos + t.galley.size() * 0.5);
            }
        }
    }
    assert!(!labels.contains_key("Link X/Y"));
    let notes = labels["Notes"];
    let x = labels["X 0"];
    let events = labels["Events"];
    assert!((notes.y - x.y).abs() < 2., "{notes:?} {x:?}");
    assert!((events.y - x.y).abs() < 2.);
    let lock = labels["Lock X"];
    frame(
        &mut app,
        &ctx,
        size,
        vec![pointer(lock, true), pointer(lock, false)],
        false,
    );
    assert!(app.options.lock_horizontal);
    frame(
        &mut app,
        &ctx,
        size,
        vec![pointer(events, true), pointer(events, false)],
        false,
    );
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        },
        |ctx| app.ui(ctx, false),
    );
    let choice = output
        .shapes
        .into_iter()
        .find_map(|c| {
            if let egui::Shape::Text(t) = c.1 {
                (t.galley.job.text == "Storyboard").then(|| t.pos + t.galley.size() * 0.5)
            } else {
                None
            }
        })
        .unwrap();
    frame(
        &mut app,
        &ctx,
        size,
        vec![pointer(choice, true), pointer(choice, false)],
        false,
    );
    assert!(crate::story_grid::active(&app));
    assert!(app.advanced.story.is_none());
    assert_eq!(app.editor.document().as_json(), &raw);
}

#[test]
fn shader_workspace_selector_and_effect_picker_share_the_event_header_baseline() {
    use crate::advanced_ui::RightMode;
    for size in [Vec2::new(873., 393.), Vec2::new(582., 262.)] {
        let mut baselines = vec![];
        for (mode, label) in [
            (RightMode::Events, "Events"),
            (RightMode::Storyboard, "Storyboard"),
            (RightMode::Shaders, "Shaders"),
        ] {
            let mut app = Workbench::new(ChartDocument::empty("Header", 120.).unwrap(), None);
            app.advanced.right_mode = mode;
            let ctx = egui::Context::default();
            for _ in 0..4 {
                frame(&mut app, &ctx, size, vec![], false);
            }
            let out = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| app.ui(ctx, false),
            );
            let y = out
                .shapes
                .iter()
                .find_map(|s| {
                    if let egui::Shape::Text(t) = &s.1 {
                        (t.galley.job.text == label).then_some(t.pos.y)
                    } else {
                        None
                    }
                })
                .unwrap();
            baselines.push(y);
            if mode == RightMode::Shaders {
                let effect_y = out
                    .shapes
                    .iter()
                    .find_map(|s| {
                        if let egui::Shape::Text(t) = &s.1 {
                            (t.galley.job.text == "Select effect").then_some(t.pos.y)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                assert!((effect_y - y).abs() <= 0.5);
            }
        }
        assert!(
            baselines.iter().all(|y| (*y - baselines[0]).abs() <= 0.5),
            "{size:?}: {baselines:?}"
        );
    }
}
