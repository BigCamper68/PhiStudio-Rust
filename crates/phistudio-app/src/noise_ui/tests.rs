use super::*;
use egui::{Event, Modifiers, PointerButton, Rect, pos2, vec2};
fn app() -> Workbench {
    let mut app = Workbench::new(
        phistudio_core::ChartDocument::empty("Noise UI", 120.).unwrap(),
        None,
    );
    app.noise.format = Format::Native;
    app.advanced.right_mode = RightMode::NoiseFields;
    app.refresh_noise();
    app
}
fn button(pos: egui::Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}
fn input(app: &mut Workbench, events: &[Event], cancel: bool) {
    let grid =
        crate::grid_input::geometry(Rect::from_min_max(pos2(0., 80.), pos2(900., 700.)), app);
    crate::grid_input::handle(app, events, grid, grid.full_rect(), |_| true, cancel);
}
#[test]
fn field_properties_clipboard_and_history_share_the_project_document() {
    let mut app = app();
    app.add_noise();
    assert_eq!(noise::fields(app.editor.document()).unwrap().len(), 1);
    app.noise.draft.as_mut().unwrap().raw["future"] = json!({"x":3});
    app.noise.draft.as_mut().unwrap().raw["phiStudioName"] = json!("Selected field");
    assert!(app.flush_noise_draft());
    let saved = app.editor.document().as_json().clone();
    app.editor.mark_saved();
    app.noise_key(egui::Key::D, Modifiers::COMMAND);
    assert_eq!(noise::fields(app.editor.document()).unwrap().len(), 2);
    assert_eq!(
        app.editor.document().as_json()[noise::LIST][1]["future"],
        json!({"x":3})
    );
    app.noise_key(egui::Key::Z, Modifiers::COMMAND);
    assert_eq!(app.editor.document().as_json(), &saved);
    assert!(!app.editor.is_dirty());
    app.noise_key(egui::Key::Y, Modifiers::COMMAND);
    assert_eq!(noise::fields(app.editor.document()).unwrap().len(), 2);
}
#[test]
fn animation_keys_in_both_dialects_keep_unknown_json_and_validate_before_committing() {
    for format in [Format::Native, Format::Next] {
        let mut app = app();
        app.noise.format = format;
        app.add_noise();
        app.noise.animation = true;
        app.noise.track = if format == Format::Native {
            Track::Move
        } else {
            Track::MoveX
        };
        app.add_noise();
        app.refresh_noise();
        let original = app.editor.document().as_json().clone();
        let d = app.noise.key_draft.as_mut().unwrap();
        d.raw["unknown"] = json!(true);
        assert!(app.flush_noise_draft());
        assert_eq!(
            app.editor.document().as_json()[noise::LIST][0][app.noise.track.key()][0]["unknown"],
            true
        );
        app.noise.key_draft.as_mut().unwrap().times[0] = "unfinished".into();
        let rev = app.editor.revision();
        assert!(!app.flush_noise_draft());
        assert_eq!(app.editor.revision(), rev);
        assert!(app.noise_back());
        assert!(!app.noise.animation);
        assert_eq!(
            app.editor.document().as_json()["judgeLineList"],
            original["judgeLineList"]
        );
    }
}
#[test]
fn timeline_creation_and_cancellation_are_one_gesture_and_one_history_entry() {
    let mut app = app();
    app.set_tool(Tool::Event);
    let a = pos2(200., 550.);
    let b = pos2(200., 350.);
    input(
        &mut app,
        &[button(a, true), Event::PointerMoved(b), button(b, false)],
        false,
    );
    assert_eq!(noise::fields(app.editor.document()).unwrap().len(), 1);
    assert_eq!(app.editor.history_rows().len(), 1);
    let saved = app.editor.document().as_json().clone();
    input(&mut app, &[button(a, true), Event::PointerMoved(b)], false);
    input(&mut app, &[], true);
    assert_eq!(app.editor.document().as_json(), &saved);
    app.editor.undo();
    assert!(noise::fields(app.editor.document()).unwrap().is_empty());
}
#[test]
fn activation_and_color_tracks_are_editable_and_palette_apply_has_undo() {
    let mut app = app();
    app.noise.format = Format::Next;
    app.add_noise();
    app.apply_noise_color(None, [0, 255, 0]);
    assert_eq!(
        noise::fields(app.editor.document()).unwrap()[0][noise::COLOR],
        json!([0, 255, 0])
    );
    app.noise.animation = true;
    app.noise.track = Track::Color;
    app.add_noise();
    app.apply_noise_color(Some((0, true)), [0, 0, 255]);
    assert_eq!(
        noise::fields(app.editor.document()).unwrap()[0][noise::COLOR_EVENTS][0]["end"],
        json!([0, 0, 255])
    );
    app.editor.undo();
    assert_eq!(
        noise::fields(app.editor.document()).unwrap()[0][noise::COLOR_EVENTS][0]["end"],
        json!([0, 255, 0])
    );
    app.refresh_noise();
    app.noise.key_draft.as_mut().unwrap().raw["inst"] = json!(1);
    assert!(app.flush_noise_draft());
    app.apply_noise_color(Some((0, false)), [255, 255, 0]);
    let key = &noise::fields(app.editor.document()).unwrap()[0][noise::COLOR_EVENTS][0];
    assert_eq!(key["start"], json!([255, 255, 0]));
    assert_eq!(key["end"], key["start"]);
}
#[test]
fn noise_workspace_draws_on_phone_and_tablet_without_note_grid_or_shader_navigation() {
    for size in [vec2(873., 393.), vec2(1194., 834.)] {
        let mut app = app();
        app.add_noise();
        let ctx = egui::Context::default();
        let mut output = None;
        for _ in 0..3 {
            output = Some(ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| app.ui(ctx, false),
            ));
        }
        let labels: Vec<_> = output
            .unwrap()
            .shapes
            .into_iter()
            .filter_map(|s| {
                if let egui::Shape::Text(t) = s.1 {
                    Some(t.galley.job.text.clone())
                } else {
                    None
                }
            })
            .collect();
        assert!(labels.iter().any(|t| t == "Noise Fields"), "{labels:?}");
        assert!(!labels.iter().any(|t| t == "Animation"), "{labels:?}");
        assert!(!labels.iter().any(|t| t == "Properties"), "{labels:?}");
        assert!(!labels.iter().any(|t| t == "FX BPM"));
    }
}

#[test]
fn grouped_drag_overlap_cycle_and_empty_box_selection_keep_one_history_step() {
    let mut app = app();
    app.add_noise();
    app.noise_key(egui::Key::D, Modifiers::COMMAND);
    app.set_tool(Tool::Select);
    app.noise.selection = [0, 1].into();
    let grid =
        crate::grid_input::geometry(Rect::from_min_max(pos2(0., 80.), pos2(900., 700.)), &app);
    let origin = pos2(
        layout::column(&app, grid, layout::View::focus(&app), 0.)
            .center()
            .x,
        grid.y(2.),
    );
    let mut gesture = match grid::begin(&mut app, grid, origin, false).unwrap() {
        crate::grid_input::Gesture::Noise(g) => *g,
        _ => panic!(),
    };
    let saved = app.editor.document().as_json().clone();
    let history = app.editor.history_rows().len();
    let destination = origin - vec2(0., app.timeline.zoom);
    grid::update(&mut app, &mut gesture, destination);
    grid::finish(&mut app, gesture);
    assert_eq!(app.editor.history_rows().len(), history + 1);
    for row in noise::fields(app.editor.document()).unwrap() {
        assert_eq!(row["appearTime"], 0.5);
    }
    app.editor.undo();
    app.refresh_noise();
    assert_eq!(app.editor.document().as_json(), &saved);
    app.noise.selection = [0].into();
    app.noise.selected = Some(0);
    let tap = || grid::NoiseGesture::Select {
        origin,
        grid,
        ids: vec![0, 1],
        moved: false,
        additive: false,
        offset: 0.,
        horizontal: None,
    };
    grid::finish(&mut app, tap());
    let first = app.noise.selected;
    let gesture = match grid::begin(&mut app, grid, origin, false).unwrap() {
        crate::grid_input::Gesture::Noise(g) => *g,
        _ => panic!(),
    };
    grid::finish(&mut app, gesture);
    assert_ne!(app.noise.selected, first);
    app.select_noise(app.noise.selected, true);
    assert!(app.noise.selection.is_empty());
    assert!(app.noise.selected.is_none());
    app.noise.selection = [0, 1].into();
    grid::finish(
        &mut app,
        grid::NoiseGesture::Box {
            origin: pos2(grid.rect.center().x + 20., 300.),
            current: pos2(grid.rect.right() - 10., 400.),
            grid,
            additive: false,
        },
    );
    assert!(app.noise.selection.is_empty());
    assert!(app.noise.selected.is_none());
}

#[test]
fn duplicating_next_keys_places_them_after_the_original_in_one_undo_step() {
    let mut app = app();
    app.noise.format = Format::Next;
    app.add_noise();
    app.noise.animation = true;
    app.noise.track = Track::MoveX;
    app.add_noise();
    let source = app.editor.document().as_json().clone();
    let history = app.editor.history_rows().len();
    app.noise_key(egui::Key::D, Modifiers::COMMAND);
    let rows = noise::keys(
        &noise::fields(app.editor.document()).unwrap()[0],
        Track::MoveX,
    )
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["startTime"], rows[0]["endTime"]);
    assert_eq!(app.editor.history_rows().len(), history + 1);
    app.editor.undo();
    assert_eq!(app.editor.document().as_json(), &source);
}

#[test]
fn visiting_next_anchor_properties_keeps_implicit_defaults_and_cancel_restores_geometry() {
    let mut app = app();
    app.noise.format = Format::Next;
    app.add_noise();
    app.noise.animation = true;
    app.noise.track = Track::ScaleX;
    app.add_noise();
    let mut raw = app.editor.document().as_json()[noise::LIST][0]["scaleXEvents"][0].clone();
    raw.as_object_mut().unwrap().shift_remove("anchor");
    assert!(app.noise_actions(
        "Implicit anchor",
        vec![Action::Key {
            field: 0,
            track: Track::ScaleX,
            index: Some(0),
            value: Some(raw)
        }]
    ));
    app.refresh_noise();
    let source = app.editor.document().as_json().clone();
    let ctx = egui::Context::default();
    for _ in 0..3 {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1194., 834.))),
                ..Default::default()
            },
            |ctx| app.ui(ctx, false),
        );
    }
    assert!(app.flush_noise_draft());
    assert_eq!(app.editor.document().as_json(), &source);
    let original = app.noise.draft.as_ref().unwrap().raw.clone();
    app.noise.preview_drag = Some((original.clone(), pos2(0., 0.), 0));
    app.noise.draft.as_mut().unwrap().raw["bottomLeft"]["x"] = json!(-100.);
    app.cancel_noise_bounds_drag();
    assert_eq!(app.noise.draft.as_ref().unwrap().raw, original);
    app.noise.sample_target = Some(SampleTarget::Position);
    app.close_fullscreen_preview();
    assert!(app.noise.sample_target.is_none());
}

#[test]
fn paired_surfaces_route_creation_to_each_visible_animation_column() {
    for format in [Format::Native, Format::Next] {
        let mut app = app();
        app.noise.format = format;
        app.add_noise();
        app.set_tool(Tool::Event);
        for (index, track) in Track::all(format).iter().enumerate() {
            let area = Rect::from_min_max(pos2(0., 80.), pos2(900., 700.));
            let g = crate::grid_input::geometry(area, &app);
            assert!(g.rect.right() < g.event_rect.left());
            let col = layout::column(
                &app,
                g.events(),
                layout::View {
                    keys: true,
                    track: *track,
                },
                index as f64,
            );
            assert!(g.event_rect.contains(col.center()));
            let a = pos2(col.center().x, g.y(2.));
            let b = pos2(col.center().x, g.y(3.));
            input(
                &mut app,
                &[button(a, true), Event::PointerMoved(b), button(b, false)],
                false,
            );
            assert!(app.noise.animation);
            assert_eq!(app.noise.track, *track);
            let fields = noise::fields(app.editor.document()).unwrap();
            assert_eq!(fields.len(), 1);
            assert_eq!(
                noise::keys(&fields[0], *track).unwrap().len(),
                if track.native(format) { 2 } else { 1 }
            );
        }
        app.editor.undo();
        app.refresh_noise();
        let fields = noise::fields(app.editor.document()).unwrap();
        assert!(
            noise::keys(&fields[0], *Track::all(format).last().unwrap())
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn horizontal_field_layout_is_unbounded_and_does_not_change_playable_geometry() {
    let mut app = app();
    app.add_noise();
    app.set_tool(Tool::Select);
    let source = app.editor.document().as_json().clone();
    let history = app.editor.history_rows().len();
    let g = crate::grid_input::geometry(Rect::from_min_max(pos2(0., 80.), pos2(900., 700.)), &app);
    let w = layout::width(&app, g, false);
    let origin = pos2(
        layout::column(&app, g, layout::View::focus(&app), 0.)
            .center()
            .x,
        g.y(2.),
    );
    input(
        &mut app,
        &[
            button(origin, true),
            Event::PointerMoved(origin + vec2(w * 1.75, 0.)),
            button(origin + vec2(w * 1.75, 0.), false),
        ],
        false,
    );
    let row = &app.editor.document().as_json()[noise::LIST][0];
    let lane = layout::lane(row, 0);
    let step = f64::from(layout::grid_step(g));
    assert!((lane * f64::from(w) / step - (lane * f64::from(w) / step).round()).abs() < 1e-5);
    assert!((lane * f64::from(w) - f64::from(w) * 1.75).abs() <= step / 2.);
    let mut without_position = row.clone();
    without_position
        .as_object_mut()
        .unwrap()
        .shift_remove("editorPositionX");
    let mut before = source[noise::LIST][0].clone();
    before
        .as_object_mut()
        .unwrap()
        .shift_remove("editorPositionX");
    assert_eq!(without_position, before);
    assert_eq!(app.editor.history_rows().len(), history + 1);
    app.editor.undo();
    app.refresh_noise();
    assert_eq!(app.editor.document().as_json(), &source);
    app.editor.mark_saved();
    for offset in [-1_000_000., 2_000_000., -3_000_000.] {
        let prior = app.noise.offset;
        layout::pan(&mut app, g, offset);
        assert!((app.noise.offset - prior - offset / f64::from(w)).abs() < 1e-8);
    }
    assert!(!app.editor.is_dirty());
    let native = app.noise.rows[0].clone();
    let mut positioned = native.clone();
    positioned["editorPositionX"] = json!(432.);
    assert_eq!(layout::lane(&positioned, 0), 4.5);
}

#[test]
fn native_point_keys_hit_their_outgoing_intervals_without_reordering_source() {
    let mut app = app();
    app.add_noise();
    let mut field = app.noise.rows[0].clone();
    field["moveEvents"] = json!([
        {"time":1.5,"endPosition":{"x":0.2,"y":0.4},"easeTypeX":1},
        {"time":0.,"endPosition":{"x":0.,"y":0.}}
    ]);
    assert!(app.noise_actions(
        "Native interval fixture",
        vec![Action::Update {
            index: 0,
            value: field
        }]
    ));
    app.refresh_noise();
    assert!(app.focus_noise(true, Track::Move));
    let g = crate::grid_input::geometry(Rect::from_min_max(pos2(0., 80.), pos2(900., 700.)), &app);
    let col = layout::column(&app, g.events(), layout::View::focus(&app), 0.);
    assert_eq!(
        grid::hits(&app, g.events(), pos2(col.center().x, g.y(2.))),
        vec![1]
    );
    assert!(grid::hits(&app, g.events(), pos2(col.center().x, g.y(3.))).contains(&0));
    assert!(grid::hits(&app, g.events(), pos2(col.center().x, g.y(3.5))).is_empty());
    let saved = app.editor.document().as_json().clone();
    assert!(app.focus_noise(false, Track::Move));
    assert_eq!(app.editor.document().as_json(), &saved);
}

#[test]
fn only_field_grid_pans_horizontally_and_both_surfaces_share_vertical_time() {
    let mut app = app();
    app.noise.format = Format::Next;
    app.add_noise();
    app.set_tool(Tool::Select);
    app.editor.mark_saved();
    let g = crate::grid_input::geometry(Rect::from_min_max(pos2(0., 80.), pos2(900., 700.)), &app);
    let a = g.event_rect.center();
    let b = a - vec2(100., 0.);
    input(
        &mut app,
        &[button(a, true), Event::PointerMoved(b), button(b, false)],
        false,
    );
    assert_eq!(app.noise.offset, 0.);
    assert_eq!(app.grid_beat(), g.beat);
    let a = pos2(g.rect.right() - 20., g.rect.center().y);
    let b = a + vec2(240., 0.);
    input(
        &mut app,
        &[button(a, true), Event::PointerMoved(b), button(b, false)],
        false,
    );
    assert!((app.noise.offset + 240. / f64::from(layout::width(&app, g, false))).abs() < 0.001);
    let a = g.event_rect.center();
    let b = a + vec2(0., 100.);
    input(
        &mut app,
        &[button(a, true), Event::PointerMoved(b), button(b, false)],
        false,
    );
    let after =
        crate::grid_input::geometry(Rect::from_min_max(pos2(0., 80.), pos2(900., 700.)), &app);
    assert!(after.beat > g.beat);
    assert_eq!(after.y(3.), after.events().y(3.));
    assert!((app.noise.offset + 240. / f64::from(layout::width(&app, g, false))).abs() < 0.001);
    assert!(!app.editor.is_dirty());
    assert_eq!(app.editor.history_rows().len(), 1);
}

#[test]
fn field_list_jump_reveals_distant_fields_in_both_axes_without_editing_the_chart() {
    for (format, lane) in [(Format::Native, -400.), (Format::Next, 1000.)] {
        let mut app = app();
        app.add_noise();
        let start = Beat::new(322, 0, 1).unwrap();
        let mut value = noise::create(
            start,
            Beat::new(334, 0, 1).unwrap(),
            format,
            false,
            app.editor.document().tempo(),
        );
        layout::place(&mut value, lane);
        assert!(app.noise_actions("Distant field", vec![Action::Insert { index: 1, value }]));
        app.refresh_noise();
        app.editor.mark_saved();
        let saved = app.editor.document().as_json().clone();
        let history = app.editor.history_rows().len();
        let area = Rect::from_min_max(pos2(0., 80.), pos2(900., 700.));
        let g = crate::grid_input::geometry(area, &app);
        app.transport.playing = true;
        app.windows.sidebar = false;
        assert!(app.jump_noise_field(1, g));
        assert!(!app.windows.sidebar);
        assert_eq!(app.noise.selected, Some(1));
        assert_eq!(app.noise.selection, [1].into());
        assert!(!app.transport.playing);
        assert!(!app.noise.animation);
        let after = crate::grid_input::geometry(area, &app);
        let col = layout::column(&app, after, layout::View::focus(&app), lane);
        assert!((col.center().x - after.rect.center().x).abs() < 0.01);
        assert!(
            after
                .rect
                .contains(pos2(col.center().x, after.y(start.as_f64())))
        );
        assert_eq!(app.editor.document().as_json(), &saved);
        assert_eq!(app.editor.history_rows().len(), history);
        assert!(!app.editor.is_dirty());
        // Choosing the same list item again also reveals a field after manual navigation.
        layout::pan(&mut app, g, 100000.);
        app.set_grid_beat(0.);
        assert!(app.jump_noise_field(1, after));
        assert_eq!(app.grid_beat(), 321.);
        let offset = app.noise.offset;
        app.noise.draft.as_mut().unwrap().times[0] = "unfinished".into();
        assert!(!app.jump_noise_field(0, after));
        assert_eq!(app.noise.selected, Some(1));
        assert_eq!(app.noise.offset, offset);
        assert_eq!(app.editor.document().as_json(), &saved);
    }
}

#[test]
fn field_picker_clicks_jump_even_when_reselecting_the_current_field() {
    let _language = crate::i18n::Scope::enter(phistudio_core::Language::English);
    let mut app = app();
    app.add_noise();
    app.noise.draft.as_mut().unwrap().raw["phiStudioName"] = json!("Near");
    assert!(app.flush_noise_draft());
    let mut far = noise::create(
        Beat::new(322, 0, 1).unwrap(),
        Beat::new(334, 0, 1).unwrap(),
        Format::Next,
        false,
        app.editor.document().tempo(),
    );
    far["Name"] = json!("Far");
    layout::place(&mut far, 1000.);
    assert!(app.noise_actions(
        "Picker target",
        vec![Action::Insert {
            index: 1,
            value: far
        }]
    ));
    app.refresh_noise();
    app.editor.mark_saved();
    app.windows.sidebar = false;
    let source = app.editor.document().as_json().clone();
    let ctx = egui::Context::default();
    let frame = |app: &mut Workbench, events: Vec<Event>| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1194., 834.))),
                events,
                ..Default::default()
            },
            |ctx| app.ui(ctx, false),
        )
    };
    let label = |out: &egui::FullOutput, text: &str, last: bool| {
        let points: Vec<_> = out
            .shapes
            .iter()
            .filter_map(|s| match &s.1 {
                egui::Shape::Text(t) if t.galley.job.text == text => {
                    Some(t.pos + t.galley.size() / 2.)
                }
                _ => None,
            })
            .collect();
        if last {
            *points
                .last()
                .unwrap_or_else(|| panic!("Missing picker item {text}"))
        } else {
            *points
                .first()
                .unwrap_or_else(|| panic!("Missing picker label {text}"))
        }
    };
    for _ in 0..3 {
        frame(&mut app, vec![]);
    }
    for current in ["#1 · Near", "#2 · Far"] {
        app.noise.offset = -10000.;
        app.set_grid_beat(0.);
        app.windows.sidebar = false;
        let out = frame(&mut app, vec![]);
        let selector = label(&out, current, false);
        let mode = label(&out, "Noise Fields", false);
        assert!(mode.x < selector.x);
        assert!((mode.y - selector.y).abs() < 2.);
        frame(
            &mut app,
            vec![Event::PointerMoved(selector), button(selector, true)],
        );
        frame(&mut app, vec![button(selector, false)]);
        let out = frame(&mut app, vec![]);
        let item = label(&out, "#2 · Far", true);
        frame(
            &mut app,
            vec![Event::PointerMoved(item), button(item, true)],
        );
        frame(&mut app, vec![button(item, false)]);
        assert_eq!(app.noise.selected, Some(1));
        assert_eq!(app.grid_beat(), 321.);
        assert!(app.noise.offset > 900.);
        for _ in 0..3 {
            frame(&mut app, vec![]);
        }
        let g = crate::grid_input::geometry(app.timeline.area, &app);
        let col = layout::column(&app, g, layout::View::focus(&app), 1000.);
        assert!((col.center().x - g.rect.center().x).abs() < 0.01);
        assert_eq!(app.editor.document().as_json(), &source);
        assert!(!app.editor.is_dirty());
    }
}

#[test]
fn field_gestures_snap_both_axes_at_each_configurable_grid_density_and_undo_exactly() {
    for count in [3, 5, 7, 9, 11, 13, 17, 21, 25, 33] {
        for format in [Format::Native, Format::Next] {
            let mut app = app();
            app.timeline.lanes = count;
            app.noise.format = format;
            app.set_tool(Tool::Event);
            let area = Rect::from_min_max(pos2(0., 80.), pos2(900., 700.));
            let g = crate::grid_input::geometry(area, &app);
            let w = layout::width(&app, g, false);
            assert_eq!(w, layout::FIELD_WIDTH);
            let fine = f64::from(layout::grid_step(g));
            let on_grid = |lane: f64| {
                let position = lane * f64::from(w) / fine;
                assert!((position - position.round()).abs() < 1e-5);
            };
            let x = g.rect.left() + 1.76 * w;
            let a = pos2(x, g.y(1.13));
            let b = pos2(x, g.y(4.88));
            input(
                &mut app,
                &[button(a, true), Event::PointerMoved(b), button(b, false)],
                false,
            );
            let source = app.editor.document().as_json().clone();
            let raw = &source[noise::LIST][0];
            let created_lane = layout::lane(raw, 0);
            on_grid(created_lane);
            assert!(((created_lane - 1.26) * f64::from(w)).abs() <= fine / 2. + 0.001);
            assert_eq!(
                noise::beat(raw, "appearTime", format, app.editor.document().tempo()).unwrap(),
                Beat::new(1, 1, 4).unwrap()
            );
            assert_eq!(
                noise::beat(raw, "disappearTime", format, app.editor.document().tempo()).unwrap(),
                Beat::new(5, 0, 1).unwrap()
            );
            app.set_tool(Tool::Select);
            let col = layout::column(&app, g, layout::View::focus(&app), created_lane);
            let origin = pos2(col.center().x, g.y(3.5));
            let destination = origin + vec2(w * 1.7, g.y(4.1) - g.y(3.5));
            input(
                &mut app,
                &[
                    button(origin, true),
                    Event::PointerMoved(destination),
                    button(destination, false),
                ],
                false,
            );
            let raw = &app.editor.document().as_json()[noise::LIST][0];
            let moved_lane = layout::lane(raw, 0);
            on_grid(moved_lane);
            assert!(((moved_lane - created_lane - 1.7) * f64::from(w)).abs() <= fine / 2. + 0.001);
            assert_eq!(
                noise::beat(raw, "appearTime", format, app.editor.document().tempo()).unwrap(),
                Beat::new(1, 3, 4).unwrap()
            );
            assert_eq!(app.editor.history_rows().len(), 2);
            app.editor.undo();
            app.refresh_noise();
            assert_eq!(app.editor.document().as_json(), &source);
            let stable = app.editor.document().as_json().clone();
            app.timeline.lanes = if count == 11 { 5 } else { 11 };
            let resized = crate::grid_input::geometry(area, &app);
            let after = layout::column(&app, resized, layout::View::focus(&app), created_lane);
            assert_eq!(col, after);
            assert_ne!(layout::grid_step(g), layout::grid_step(resized));
            assert_eq!(app.editor.document().as_json(), &stable);
        }
    }
}

#[test]
fn disabling_field_grid_snapping_preserves_free_positions_and_fine_times() {
    let mut app = app();
    app.options.snap_x = false;
    app.options.snap_time = false;
    app.set_tool(Tool::Event);
    let g = crate::grid_input::geometry(Rect::from_min_max(pos2(0., 80.), pos2(900., 700.)), &app);
    let w = layout::width(&app, g, false);
    let x = g.rect.left() + 1.76 * w;
    let a = pos2(x, g.y(1.13));
    let b = pos2(x, g.y(4.88));
    input(
        &mut app,
        &[button(a, true), Event::PointerMoved(b), button(b, false)],
        false,
    );
    let raw = &app.editor.document().as_json()[noise::LIST][0];
    assert!((layout::lane(raw, 0) - 1.26).abs() < 1e-5);
    let time = noise::beat(
        raw,
        "appearTime",
        Format::Native,
        app.editor.document().tempo(),
    )
    .unwrap()
    .as_f64();
    assert!((time - 1.13).abs() < 0.001);
    assert!((time - 1.25).abs() > 0.1);
}

#[test]
fn grid_density_changes_only_fine_lines_and_snapping_not_numbered_columns_or_fields() {
    let mut app = app();
    app.add_noise();
    app.noise.offset = 4.25;
    app.editor.mark_saved();
    let source = app.editor.document().as_json().clone();
    let area = Rect::from_min_max(pos2(0., 80.), pos2(900., 700.));
    let g = crate::grid_input::geometry(area, &app);
    let columns: Vec<_> = (-2..14)
        .map(|lane| layout::column(&app, g, layout::View::focus(&app), lane as f64))
        .collect();
    let base_lines: Vec<_> = layout::grid_lines(g, app.noise.offset).collect();
    for count in [3, 5, 7, 9, 11, 13, 17, 21, 25, 33] {
        app.timeline.lanes = count;
        let changed = crate::grid_input::geometry(area, &app);
        let after: Vec<_> = (-2..14)
            .map(|lane| layout::column(&app, changed, layout::View::focus(&app), lane as f64))
            .collect();
        assert_eq!(after, columns);
        let fine: Vec<_> = layout::grid_lines(changed, app.noise.offset).collect();
        if count != g.lanes {
            assert_ne!(fine, base_lines);
        }
        assert_eq!(app.editor.document().as_json(), &source);
        assert!(!app.editor.is_dirty());
    }
}

#[test]
fn dense_native_animation_captions_follow_events_and_do_not_overlap() {
    let mut app = app();
    let mut field = noise::create(
        Beat::ZERO,
        Beat::new(8, 0, 1).unwrap(),
        Format::Native,
        false,
        app.editor.document().tempo(),
    );
    let times = std::iter::once(0.)
        .chain((0..=20).map(|n| 1. + n as f64 * 0.01))
        .chain(std::iter::once(3.));
    field["scaleEvents"]=times.enumerate().map(|(n,time)|json!({"time":time,"scale":{"x":n,"y":n},"anchor":{"x":0.5,"y":0.5},"easeTypeX":0,"easeTypeY":0})).collect();
    assert!(app.noise_actions(
        "Dense scale fixture",
        vec![Action::Insert {
            index: 0,
            value: field
        }]
    ));
    app.refresh_noise();
    app.select_noise(Some(0), false);
    assert!(app.focus_noise(true, Track::Scale));
    app.editor.mark_saved();
    let source = app.editor.document().as_json().clone();
    for width in [500., 900.] {
        for zoom in [24., 96., 280.] {
            app.timeline.zoom = zoom;
            let g = crate::grid_input::geometry(
                Rect::from_min_max(pos2(0., 20.), pos2(width, 700.)),
                &app,
            );
            let ctx = egui::Context::default();
            let out = ctx.run(Default::default(), |ctx| {
                grid::paint(&app, &ctx.debug_painter(), g)
            });
            let texts: Vec<_> = out
                .shapes
                .iter()
                .filter_map(|s| match &s.1 {
                    egui::Shape::Text(t) if t.galley.job.text.starts_with('[') => {
                        let visible = Rect::from_min_size(t.pos, t.galley.size()).intersect(s.0);
                        visible.is_positive().then_some(visible)
                    }
                    _ => None,
                })
                .collect();
            assert!(!texts.is_empty());
            assert!(
                texts.len() <= 6,
                "Dense keys should hide captions like Events"
            );
            for (i, a) in texts.iter().enumerate() {
                for b in &texts[i + 1..] {
                    assert!(
                        !a.intersects(*b),
                        "{width}px / {zoom}: {a:?} overlaps {b:?}"
                    );
                }
            }
            assert_eq!(app.editor.document().as_json(), &source);
            assert!(!app.editor.is_dirty());
        }
    }
}

#[test]
fn cursor_created_fields_match_their_numbers_independently_of_grid_or_camera() {
    for format in [Format::Native, Format::Next] {
        for density in [3, 5, 7, 9, 11, 13, 17, 21, 25, 33] {
            for snap in [true, false] {
                let mut app = app();
                app.noise.format = format;
                app.timeline.lanes = density;
                app.options.snap_x = snap;
                app.timeline.area = Rect::from_min_max(pos2(0., 80.), pos2(900., 700.));
                app.transport.beat = 13.5;
                for index in 0..8 {
                    app.noise.offset = if snap { 10000. } else { -10000. };
                    app.add_noise();
                    let row = &noise::fields(app.editor.document()).unwrap()[index];
                    assert_eq!(layout::lane(row, index), index as f64);
                    let g = crate::grid_input::geometry(app.timeline.area, &app);
                    let col = layout::column(&app, g, layout::View::focus(&app), index as f64);
                    assert!((col.center().x - g.rect.center().x).abs() < 0.01);
                    assert_eq!(app.transport.beat, 13.5);
                }
            }
        }
    }
}

#[test]
fn cursor_creation_uses_its_number_even_if_occupied_and_keeps_visible_camera_stable() {
    let mut app = app();
    app.timeline.area = Rect::from_min_max(pos2(0., 80.), pos2(900., 700.));
    for _ in 0..7 {
        app.add_noise();
    }
    let mut moved = app.noise.rows[0].clone();
    layout::place(&mut moved, 7.);
    moved["future"] = json!(true);
    assert!(app.noise_actions(
        "Moved field fixture",
        vec![Action::Update {
            index: 0,
            value: moved
        }]
    ));
    app.refresh_noise();
    app.noise.offset = 5.;
    let before = app.editor.document().as_json().clone();
    app.editor.mark_saved();
    app.add_noise();
    assert_eq!(layout::lane(&app.noise.rows[7], 7), 7.);
    assert_eq!(app.noise.rows[0], before[noise::LIST][0]);
    assert_eq!(app.noise.offset, 5.);
    app.editor.undo();
    app.refresh_noise();
    assert_eq!(app.editor.document().as_json(), &before);
    assert!(!app.editor.is_dirty());
}

mod gestures;

#[test]
fn new_projects_default_to_next_without_converting_existing_fields() {
    let mut app = Workbench::new(
        phistudio_core::ChartDocument::empty("Default format", 120.).unwrap(),
        None,
    );
    app.advanced.right_mode = RightMode::NoiseFields;
    app.refresh_noise();
    assert_eq!(app.noise.format, Format::Next);
    app.add_noise();
    let next = app.noise.rows[0].clone();
    assert_eq!(noise::format(&next).unwrap(), Format::Next);
    app.noise.format = Format::Native;
    app.add_noise();
    assert_eq!(app.noise.rows[0], next);
    assert_eq!(noise::format(&app.noise.rows[1]).unwrap(), Format::Native);
    app.session += 1;
    app.refresh_noise();
    assert_eq!(app.noise.format, Format::Next);
    assert_eq!(app.noise.rows[0], next);
    assert_eq!(noise::format(&app.noise.rows[1]).unwrap(), Format::Native);
}
