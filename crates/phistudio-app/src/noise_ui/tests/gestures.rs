use super::*;
fn geometry(app: &Workbench) -> crate::grid_input::Grid {
    crate::grid_input::geometry(Rect::from_min_max(pos2(0., 80.), pos2(900., 700.)), app)
}
fn drag(app: &mut Workbench, a: egui::Pos2, b: egui::Pos2) {
    input(
        app,
        &[button(a, true), Event::PointerMoved(b), button(b, false)],
        false,
    );
}
#[test]
fn outer_field_end_wins_over_tied_disable_and_resize_is_one_undo() {
    for format in [Format::Native, Format::Next] {
        let mut app = app();
        app.noise.format = format;
        app.add_noise();
        app.set_tool(Tool::Select);
        let source = app.editor.document().as_json().clone();
        let g = geometry(&app);
        let x = layout::column(&app, g, layout::View::focus(&app), 0.)
            .center()
            .x;
        let a = pos2(x, g.y(4.));
        let b = pos2(x, g.y(5.));
        input(&mut app, &[button(a, true), Event::PointerMoved(b)], false);
        assert_eq!(app.editor.document().as_json(), &source);
        let Some(crate::grid_input::Gesture::Noise(gesture)) = &app.drag else {
            panic!("Missing field resize");
        };
        let grid::NoiseGesture::Edit(edit) = &**gesture else {
            panic!("Field resize became navigation");
        };
        assert_eq!(
            noise::beat(
                &edit.current,
                "disappearTime",
                format,
                app.editor.document().tempo()
            )
            .unwrap(),
            Beat::new(5, 0, 1).unwrap()
        );
        if format == Format::Native {
            assert_eq!(edit.current["disableTime"], edit.current["disappearTime"]);
        }
        input(&mut app, &[button(b, false)], false);
        assert_eq!(app.editor.history_rows().len(), 2);
        app.editor.undo();
        app.refresh_noise();
        assert_eq!(app.editor.document().as_json(), &source);
        drag(&mut app, pos2(x, g.y(0.)), pos2(x, g.y(1.)));
        let raw = &noise::fields(app.editor.document()).unwrap()[0];
        assert_eq!(
            noise::beat(raw, "appearTime", format, app.editor.document().tempo()).unwrap(),
            Beat::ONE
        );
    }
}
#[test]
fn keyframes_begin_at_one_grid_step_and_both_ends_resize_in_every_track() {
    for format in [Format::Native, Format::Next] {
        for track in Track::all(format) {
            let mut app = app();
            app.noise.format = format;
            app.add_noise();
            if *track == Track::Active {
                let mut field = app.noise.rows[0].clone();
                field["activeIntervals"] = json!([]);
                assert!(app.noise_actions(
                    "Empty activation fixture",
                    vec![Action::Update {
                        index: 0,
                        value: field
                    }]
                ));
                app.refresh_noise();
            }
            app.noise.animation = true;
            app.noise.track = *track;
            app.beat = 1.;
            let count = noise::keys(&app.noise.rows[0], *track).unwrap().len();
            app.add_noise();
            app.set_tool(Tool::Select);
            let original = app.editor.document().as_json().clone();
            let values = noise::keys(&app.noise.rows[0], *track).unwrap();
            if track.native(format) {
                assert_eq!(
                    noise::beat(
                        &values[count + 1],
                        "time",
                        Format::Native,
                        app.editor.document().tempo()
                    )
                    .unwrap(),
                    Beat::new(1, 1, 4).unwrap()
                );
            } else {
                assert_eq!(values[count]["endTime"], json!([1, 1, 4]));
            }
            let g = geometry(&app);
            let lane = Track::all(format).iter().position(|t| t == track).unwrap();
            let x = layout::column(&app, g.events(), layout::View::focus(&app), lane as f64)
                .center()
                .x;
            drag(&mut app, pos2(x, g.y(1.25)), pos2(x, g.y(3.)));
            let values = noise::keys(&app.noise.rows[0], *track).unwrap();
            let (key, name, f) = if track.native(format) {
                (&values[count + 1], "time", Format::Native)
            } else {
                (&values[count], "endTime", Format::Next)
            };
            assert_eq!(
                noise::beat(key, name, f, app.editor.document().tempo()).unwrap(),
                Beat::new(3, 0, 1).unwrap(),
                "{format:?} {track:?}"
            );
            app.editor.undo();
            app.refresh_noise();
            assert_eq!(app.editor.document().as_json(), &original);
            drag(&mut app, pos2(x, g.y(1.)), pos2(x, g.y(0.5)));
            let values = noise::keys(&app.noise.rows[0], *track).unwrap();
            let (name, f) = if track.native(format) {
                ("time", Format::Native)
            } else {
                ("startTime", Format::Next)
            };
            assert_eq!(
                noise::beat(&values[count], name, f, app.editor.document().tempo()).unwrap(),
                Beat::new(0, 1, 2).unwrap(),
                "{format:?} {track:?}"
            );
        }
    }
}
#[test]
fn tap_and_hold_creation_stays_anchored_and_cancellation_discards_native_endpoints() {
    for format in [Format::Native, Format::Next] {
        let mut app = app();
        app.noise.format = format;
        app.add_noise();
        app.set_tool(Tool::Event);
        let g = geometry(&app);
        let track = if format == Format::Native {
            Track::Scale
        } else {
            Track::ScaleX
        };
        let lane = Track::all(format).iter().position(|t| *t == track).unwrap();
        let x = layout::column(
            &app,
            g.events(),
            layout::View { keys: true, track },
            lane as f64,
        )
        .center()
        .x;
        let a = pos2(x, g.y(1.));
        let b = pos2(x, g.y(3.));
        let before = app.editor.document().as_json().clone();
        input(&mut app, &[button(a, true)], false);
        input(&mut app, &[Event::PointerMoved(b)], false);
        assert_eq!(app.editor.document().as_json(), &before);
        input(&mut app, &[], true);
        assert_eq!(app.editor.document().as_json(), &before);
        drag(&mut app, a, b);
        let values = noise::keys(&app.noise.rows[0], track).unwrap();
        if format == Format::Native {
            assert_eq!(values.len(), 2);
            assert_eq!(
                noise::beat(&values[0], "time", format, app.editor.document().tempo()).unwrap(),
                Beat::ONE
            );
            assert_eq!(
                noise::beat(&values[1], "time", format, app.editor.document().tempo()).unwrap(),
                Beat::new(3, 0, 1).unwrap()
            );
        } else {
            assert_eq!(values[0]["startTime"], json!([1, 0, 1]));
            assert_eq!(values[0]["endTime"], json!([3, 0, 1]));
        }
        assert_eq!(app.editor.history_rows().len(), 2);
        assert!(app.tool == Tool::Event);
        app.editor.undo();
        app.refresh_noise();
        assert_eq!(app.editor.document().as_json(), &before);
        drag(&mut app, a, a);
        let values = noise::keys(&app.noise.rows[0], track).unwrap();
        if format == Format::Native {
            assert_eq!(
                noise::beat(&values[1], "time", format, app.editor.document().tempo()).unwrap(),
                Beat::new(1, 1, 4).unwrap()
            );
        } else {
            assert_eq!(values[0]["endTime"], json!([1, 1, 4]));
        }
    }
}
#[test]
fn real_toolbar_and_touch_creation_work_on_first_and_repeated_drags() {
    let mut app = app();
    app.show_context = true;
    let ctx = egui::Context::default();
    let frame = |app: &mut Workbench, events: Vec<Event>, canceled: bool| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1194., 834.))),
                events,
                ..Default::default()
            },
            |ctx| app.ui(ctx, canceled),
        )
    };
    for _ in 0..3 {
        frame(&mut app, vec![], false);
    }
    let out = frame(&mut app, vec![], false);
    let labels: Vec<_> = out
        .shapes
        .iter()
        .filter_map(|s| match &s.1 {
            egui::Shape::Text(t) => Some((t.galley.job.text.clone(), t.pos + t.galley.size() / 2.)),
            _ => None,
        })
        .collect();
    let buttons: Vec<_> = labels
        .iter()
        .filter(|(text, _)| text == "+ Field")
        .collect();
    assert_eq!(buttons.len(), 1);
    assert!(labels.iter().any(|(text, _)| text == "Add field at cursor"));
    let pos = buttons[0].1;
    frame(
        &mut app,
        vec![Event::PointerMoved(pos), button(pos, true)],
        false,
    );
    frame(&mut app, vec![button(pos, false)], false);
    assert!(app.tool == Tool::Event);
    let mut touch = crate::touch::TouchRouter::default();
    for n in 0..2 {
        frame(&mut app, vec![], false);
        let g = crate::grid_input::geometry(app.grid_area, &app);
        let col = layout::column(&app, g, layout::View::focus(&app), n as f64 * 2.);
        let a = pos2(col.center().x, g.y(1.));
        let b = pos2(col.center().x, g.y(3.));
        let (events, cancel) = touch.event(egui::TouchPhase::Start, n + 1, a);
        frame(&mut app, events, cancel);
        assert!(matches!(
            app.drag,
            Some(crate::grid_input::Gesture::Noise(_))
        ));
        let (events, cancel) = touch.event(egui::TouchPhase::Move, n + 1, b);
        frame(&mut app, events, cancel);
        assert_eq!(
            noise::fields(app.editor.document()).unwrap().len(),
            n as usize
        );
        let (events, cancel) = touch.event(egui::TouchPhase::End, n + 1, b);
        frame(&mut app, events, cancel);
        let fields = noise::fields(app.editor.document()).unwrap();
        assert_eq!(fields.len(), n as usize + 1);
        assert_eq!(
            noise::beat(
                &fields[n as usize],
                "appearTime",
                Format::Native,
                app.editor.document().tempo()
            )
            .unwrap(),
            Beat::ONE
        );
        assert_eq!(
            noise::beat(
                &fields[n as usize],
                "disappearTime",
                Format::Native,
                app.editor.document().tempo()
            )
            .unwrap(),
            Beat::new(3, 0, 1).unwrap()
        );
        assert!(app.tool == Tool::Event);
    }
    assert_eq!(app.editor.history_rows().len(), 2);
}

#[test]
fn native_terminal_point_can_grow_a_new_segment_without_extending_the_field() {
    let mut app = app();
    app.add_noise();
    app.noise.animation = true;
    app.noise.track = Track::Move;
    app.beat = 1.;
    app.add_noise();
    app.select_noise(Some(1), false);
    app.set_tool(Tool::Select);
    let source = app.editor.document().as_json().clone();
    let g = geometry(&app);
    let x = layout::column(&app, g.events(), layout::View::focus(&app), 0.)
        .center()
        .x;
    let a = pos2(x, g.y(1.25) - 2.);
    let b = pos2(x, g.y(3.));
    input(&mut app, &[button(a, true), Event::PointerMoved(b)], false);
    assert_eq!(app.editor.document().as_json(), &source);
    input(&mut app, &[], true);
    assert_eq!(app.editor.document().as_json(), &source);
    drag(&mut app, a, b);
    let field = &noise::fields(app.editor.document()).unwrap()[0];
    assert_eq!(
        field["disappearTime"],
        source[noise::LIST][0]["disappearTime"]
    );
    let keys = noise::keys(field, Track::Move).unwrap();
    assert_eq!(keys.len(), 3);
    assert_eq!(
        noise::beat(
            &keys[2],
            "time",
            Format::Native,
            app.editor.document().tempo()
        )
        .unwrap(),
        Beat::new(3, 0, 1).unwrap()
    );
    assert_eq!(app.editor.history_rows().len(), 3);
    app.editor.undo();
    app.refresh_noise();
    assert_eq!(app.editor.document().as_json(), &source);
}

#[test]
fn animation_body_moves_both_ends_for_short_and_tall_blocks_in_every_track() {
    for format in [Format::Native, Format::Next] {
        for track in Track::all(format) {
            for end in [1.25, 3.] {
                let mut app = app();
                app.noise.format = format;
                app.add_noise();
                if *track == Track::Active {
                    let mut raw = app.noise.rows[0].clone();
                    raw[track.key()] = json!([]);
                    assert!(app.noise_actions(
                        "Clear intervals",
                        vec![Action::Update {
                            index: 0,
                            value: raw
                        }]
                    ));
                    app.refresh_noise();
                }
                app.noise.animation = true;
                app.noise.track = *track;
                app.beat = 1.;
                app.add_noise();
                let index = app.noise.key_selected.unwrap();
                let mut raw = app.noise.rows[0].clone();
                let native = track.native(format);
                let row = if native { index + 1 } else { index };
                let name = if native { "time" } else { "endTime" };
                noise::put_beat(
                    &mut raw[track.key()][row],
                    name,
                    Beat::from_grid(end, 4).unwrap(),
                    if native { Format::Native } else { Format::Next },
                    app.editor.document().tempo(),
                )
                .unwrap();
                assert!(app.noise_actions(
                    "Set interval",
                    vec![Action::Update {
                        index: 0,
                        value: raw
                    }]
                ));
                app.refresh_noise();
                app.set_tool(Tool::Select);
                let source = app.editor.document().as_json().clone();
                let g = geometry(&app);
                let lane = Track::all(format).iter().position(|t| t == track).unwrap();
                let x = layout::column(&app, g.events(), layout::View::focus(&app), lane as f64)
                    .center()
                    .x;
                let mid = (1. + end) / 2.;
                let a = pos2(x, g.y(mid));
                let b = pos2(x, g.y(mid + 1.));
                input(&mut app, &[button(a, true), Event::PointerMoved(b)], false);
                assert_eq!(app.editor.document().as_json(), &source);
                input(&mut app, &[button(b, false)], true);
                assert_eq!(app.editor.document().as_json(), &source);
                drag(&mut app, a, b);
                let values = noise::keys(&app.noise.rows[0], *track).unwrap();
                let start = if native { "time" } else { "startTime" };
                assert_eq!(
                    noise::beat(
                        &values[index],
                        start,
                        if native { Format::Native } else { Format::Next },
                        app.editor.document().tempo()
                    )
                    .unwrap(),
                    Beat::new(2, 0, 1).unwrap(),
                    "{format:?} {track:?} {end}"
                );
                assert_eq!(
                    noise::beat(
                        &values[row],
                        name,
                        if native { Format::Native } else { Format::Next },
                        app.editor.document().tempo()
                    )
                    .unwrap(),
                    Beat::from_grid(end + 1., 4).unwrap(),
                    "{format:?} {track:?} {end}"
                );
                assert_eq!(app.noise.selection, BTreeSet::from([index]));
                assert!(app.editor.undo());
                app.refresh_noise();
                assert_eq!(app.editor.document().as_json(), &source);
            }
        }
    }
}

#[test]
fn moving_adjacent_native_segments_shifts_shared_endpoints_only_once() {
    let mut app = app();
    app.add_noise();
    app.noise.animation = true;
    app.noise.track = Track::Move;
    app.beat = 1.;
    app.add_noise();
    let mut field = app.noise.rows[0].clone();
    field["moveEvents"] = json!([
        {"time":0.5,"endPosition":{"x":0.2,"y":0.3},"future":1},
        {"time":1.5,"endPosition":{"x":0.4,"y":0.5},"future":2},
        {"time":2.5,"endPosition":{"x":0.6,"y":0.7},"future":3}
    ]);
    assert!(app.noise_actions(
        "Native segment fixture",
        vec![Action::Update {
            index: 0,
            value: field
        }]
    ));
    app.refresh_noise();
    app.noise.selection = BTreeSet::from([0, 1]);
    app.set_tool(Tool::Select);
    let source = app.editor.document().as_json().clone();
    let g = geometry(&app);
    let x = layout::column(&app, g.events(), layout::View::focus(&app), 0.)
        .center()
        .x;
    drag(&mut app, pos2(x, g.y(2.)), pos2(x, g.y(3.)));
    for (i, raw) in noise::keys(&app.noise.rows[0], Track::Move)
        .unwrap()
        .iter()
        .enumerate()
    {
        assert_eq!(raw["time"], json!([1., 2., 3.][i]));
        assert_eq!(raw["future"], json!(i + 1));
    }
    assert_eq!(app.noise.selection, BTreeSet::from([0, 1]));
    assert!(app.editor.undo());
    assert_eq!(app.editor.document().as_json(), &source);
}
