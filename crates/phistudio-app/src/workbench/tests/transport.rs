use super::*;

#[test]
fn event_grid_pan_retains_playback_and_reserved_layers_remain_read_only() {
    for layer in [0, 4] {
        let mut app = Workbench::new(ChartDocument::empty("Events", 120.).unwrap(), None);
        app.events.layer = layer;
        app.tool = Tool::Select;
        app.beat = 8.;
        app.playing = true;
        let original = app.editor.document().as_json().clone();
        let grid = test_grid(&app).events();
        let p = egui::pos2(grid.rect.center().x, grid.y(9.));
        grid_events(
            &mut app,
            &[pointer(p, true), pointer(p + egui::vec2(0., 96.), false)],
            false,
        );
        assert!(app.playing);
        assert_eq!(app.beat, 9.);
        assert!(!app.grid_input.is_scrubbing());
        if layer == 4 {
            app.tool = Tool::Event;
            grid_events(&mut app, &[pointer(p, true), pointer(p, false)], false);
        }
        assert_eq!(app.editor.document().as_json(), &original);
    }
}

#[test]
fn touches_scroll_sidebar_and_floating_properties_without_scrubbing_the_chart() {
    let mut app = Workbench::new(ChartDocument::empty("Panel input", 120.).unwrap(), None);
    app.compact_properties = true;
    app.playing = true;
    app.select_event(Some(phistudio_core::EventId {
        line: 0,
        layer: 0,
        kind: phistudio_core::EventType::MoveX,
        index: 0,
    }));
    let original = app.editor.document().as_json().clone();
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    let size = Vec2::new(873., 393.);
    let mut router = crate::touch::TouchRouter::default();
    for _ in 0..5 {
        frame(&mut app, &ctx, size, Vec::new(), false);
    }
    for start in [egui::pos2(750., 250.), egui::pos2(190., 155.)] {
        let zone = crate::scroll::at(&ctx, start).expect("Properties scroll viewport");
        let before = egui::scroll_area::State::load(&ctx, zone.id)
            .unwrap()
            .offset
            .y;
        for (i, (phase, delta)) in [
            (egui::TouchPhase::Start, 0.),
            (egui::TouchPhase::Move, 30.),
            (egui::TouchPhase::Move, 65.),
            (egui::TouchPhase::End, 80.),
        ]
        .into_iter()
        .enumerate()
        {
            let (events, canceled) = router.event_with_context(
                &ctx,
                phase,
                1,
                start - egui::vec2(0., delta),
                i as f64 * 0.02,
            );
            frame(&mut app, &ctx, size, events, canceled);
        }
        let after = egui::scroll_area::State::load(&ctx, zone.id)
            .unwrap()
            .offset
            .y;
        assert!(
            after > before + 35.,
            "Panel at {start:?} blocked its touch: {before} -> {after}"
        );
        assert_eq!(app.editor.document().as_json(), &original);
        assert!(app.playing);
        assert_eq!(app.beat, 0.);
        assert!(!app.grid_input.is_scrubbing());
        router.cancel();
    }
}

#[test]
fn playback_swipes_resume_at_the_final_release_and_paused_swipes_stay_paused() {
    for tool in [
        Tool::Select,
        Tool::Place(NoteKind::Tap),
        Tool::Place(NoteKind::Flick),
        Tool::Place(NoteKind::Drag),
    ] {
        for was_playing in [false, true] {
            for direction in [-1., 1.] {
                let mut app = Workbench::new(ChartDocument::empty("Scrub", 120.).unwrap(), None);
                app.tool = tool;
                app.beat = 8.;
                app.playing = was_playing;
                let original = app.editor.document().as_json().clone();
                let grid = test_grid(&app);
                let down = egui::pos2(grid.x(0.), 180.);
                grid_events(&mut app, &[pointer(down, true)], false);
                assert!(app.grid_input.is_scrubbing());
                assert_eq!(app.playing, was_playing);
                grid_events(
                    &mut app,
                    &[egui::Event::PointerMoved(
                        down + egui::vec2(0., 96. * direction),
                    )],
                    false,
                );
                assert!(app.grid_input.is_scrubbing());
                // The release has a newer coordinate than the last move.
                grid_events(
                    &mut app,
                    &[pointer(down + egui::vec2(0., 144. * direction), false)],
                    false,
                );
                assert!(!app.grid_input.is_scrubbing());
                assert_eq!(app.playing, was_playing);
                assert!((app.beat - (8. + 1.5 * direction as f64)).abs() < 1e-9);
                assert_eq!(app.editor.document().as_json(), &original);
                assert!(!app.editor.can_undo());
            }
        }
    }
}

#[test]
fn ruler_seeks_and_batched_swipes_keep_playback_but_note_edits_stop_it() {
    let mut app = Workbench::new(ChartDocument::empty("Scrub", 120.).unwrap(), None);
    app.beat = 8.;
    app.playing = true;
    let grid = test_grid(&app);
    let ruler = egui::pos2(grid.rect.left() - 5., grid.y(10.));
    grid_events(
        &mut app,
        &[pointer(ruler, true), pointer(ruler, false)],
        false,
    );
    assert_eq!(app.beat, 10.);
    assert!(app.playing && !app.grid_input.is_scrubbing());
    let down = egui::pos2(300., 180.);
    let up = down + egui::vec2(0., 96.);
    grid_events(&mut app, &[pointer(down, true), pointer(up, false)], false);
    assert_eq!(app.beat, 11.);
    assert!(app.playing && !app.grid_input.is_scrubbing());
    app.tool = Tool::Place(NoteKind::Tap);
    grid_events(
        &mut app,
        &[pointer(down, true), pointer(down, false)],
        false,
    );
    assert!(!app.playing && !app.grid_input.is_scrubbing());
    assert_eq!(app.editor.document().note_count(), 1);
}

#[test]
fn pinch_during_playback_resumes_after_the_last_finger_without_committing_a_hold() {
    let mut app = Workbench::new(ChartDocument::empty("Pinch", 120.).unwrap(), None);
    app.beat = 8.;
    app.playing = true;
    app.tool = Tool::Place(NoteKind::Hold);
    let mut router = crate::touch::TouchRouter::default();
    for (phase, id, pos) in [
        (egui::TouchPhase::Start, 1, egui::pos2(220., 180.)),
        (egui::TouchPhase::Start, 2, egui::pos2(320., 180.)),
        (egui::TouchPhase::Move, 2, egui::pos2(420., 220.)),
        (egui::TouchPhase::End, 1, egui::pos2(220., 180.)),
    ] {
        let (events, canceled) = router.event(phase, id, pos);
        grid_events(&mut app, &events, canceled);
        assert!(app.playing && app.grid_input.is_scrubbing());
    }
    let (events, canceled) = router.event(egui::TouchPhase::End, 2, egui::pos2(420., 220.));
    grid_events(&mut app, &events, canceled);
    assert!(app.playing && !app.grid_input.is_scrubbing());
    assert!(!app.editor.can_undo());
    assert_eq!(app.editor.document().note_count(), 0);
}

#[test]
fn canceling_or_explicitly_pausing_a_scrub_does_not_restart_playback() {
    for cancel in [false, true] {
        let mut app = Workbench::new(ChartDocument::empty("Cancel", 120.).unwrap(), None);
        app.playing = true;
        let down = egui::pos2(300., 180.);
        grid_events(&mut app, &[pointer(down, true)], false);
        assert!(app.grid_input.is_scrubbing());
        if cancel {
            grid_events(&mut app, &[pointer(down, false)], true);
        } else {
            app.playing = false; // An explicit pause while the finger is down.
            grid_events(
                &mut app,
                &[pointer(down + egui::vec2(0., 96.), false)],
                false,
            );
        }
        assert!(!app.playing && !app.grid_input.is_scrubbing());
        assert!(!app.editor.can_undo());
    }
}

#[test]
fn releasing_after_a_tool_clears_the_draft_never_leaves_transport_blocked() {
    let mut app = Workbench::new(ChartDocument::empty("Tool change", 120.).unwrap(), None);
    app.playing = true;
    let down = egui::pos2(300., 180.);
    grid_events(&mut app, &[pointer(down, true)], false);
    assert!(app.grid_input.is_scrubbing());
    app.drag = None;
    grid_events(&mut app, &[pointer(down, false)], false);
    assert!(!app.grid_input.is_scrubbing() && app.playing);
}

#[test]
fn header_seek_preserves_playback_intent_and_uses_the_release_position() {
    for playing in [false, true] {
        let mut app = Workbench::new(
            ChartDocument::parse(include_str!(
                "../../../../../assets/charts/phistudio-demo.rpe.json"
            ))
            .unwrap(),
            None,
        );
        app.playing = playing;
        let ctx = egui::Context::default();
        let render = |app: &mut Workbench, events| {
            let mut rect = egui::Rect::NOTHING;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(400., 200.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        rect = egui::Rect::from_min_size(ui.cursor().min, Vec2::new(200., 32.))
                            .shrink2(egui::vec2(7., 0.));
                        app.seek_bar(ui, 200.);
                    });
                },
            );
            rect
        };
        let rect = render(&mut app, vec![]);
        let point = |fraction| egui::pos2(rect.left() + rect.width() * fraction, rect.center().y);
        render(
            &mut app,
            vec![
                egui::Event::PointerMoved(point(0.2)),
                seek_pointer_button(point(0.2), true),
            ],
        );
        assert!(app.is_scrubbing());
        assert_eq!(app.playing, playing);
        render(&mut app, vec![egui::Event::PointerMoved(point(0.5))]);
        assert!((app.beat - 32.).abs() < 0.001);
        render(
            &mut app,
            vec![
                egui::Event::PointerMoved(point(0.8)),
                seek_pointer_button(point(0.8), false),
                egui::Event::PointerGone,
            ],
        );
        assert!(!app.is_scrubbing());
        assert_eq!(app.playing, playing);
        assert!(
            (app.beat - 51.2).abs() < 0.001,
            "Final release position lost: {}",
            app.beat
        );
    }
}

#[test]
fn header_notes_button_switches_full_music_and_note_ranges_without_editing() {
    for size in [Vec2::new(873., 393.), Vec2::new(1194., 834.)] {
        let mut raw = ChartDocument::empty("Seek range", 120.)
            .unwrap()
            .as_json()
            .clone();
        raw["BPMList"] = serde_json::json!([
            {"startTime":[0,0,1],"bpm":120.},
            {"startTime":[16,0,1],"bpm":60.}
        ]);
        let mut app = Workbench::new(ChartDocument::from_value(raw).unwrap(), None);
        app.execute(Command::AddNote {
            line: 0,
            note: Note {
                start_time: Beat::new(8, 0, 1).unwrap(),
                end_time: Beat::new(8, 0, 1).unwrap(),
                ..Default::default()
            },
        });
        app.editor.mark_saved();
        app.music_duration = Some(180.);
        app.beat = 150.; // Toggling the range must preserve an outro cursor.
        app.playing = true;
        assert!(!app.options.seek_notes_only);
        assert_eq!(app.seek_max(), 188.);
        let original = app.editor.document().as_json().clone();
        let history = app.editor.history_rows().len();
        let ctx = egui::Context::default();
        ctx.set_style(crate::theme::style());
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
        let p = out
            .shapes
            .iter()
            .find_map(|s| match &s.1 {
                egui::Shape::Text(t) if t.galley.job.text == "Notes" && t.pos.y < 32. => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("Notes toggle must be reachable in the header");
        for enabled in [true, false] {
            frame(
                &mut app,
                &ctx,
                size,
                vec![egui::Event::PointerMoved(p), seek_pointer_button(p, true)],
                false,
            );
            frame(
                &mut app,
                &ctx,
                size,
                vec![seek_pointer_button(p, false)],
                false,
            );
            assert_eq!(app.options.seek_notes_only, enabled);
            assert_eq!(app.seek_max(), if enabled { 64. } else { 188. });
            assert_eq!(app.beat, 150.);
            assert!(app.playing);
        }
        app.options.seek_notes_only = true;
        let saved = app.preferences();
        let mut next = Workbench::new(app.editor.document().clone(), None);
        next.apply_preferences(&saved);
        assert!(next.options.seek_notes_only);
        assert_eq!(app.editor.document().as_json(), &original);
        assert_eq!(app.editor.history_rows().len(), history);
        assert!(!app.editor.is_dirty());
    }
}

#[test]
fn vertical_navigation_ignores_horizontal_jitter_and_horizontal_navigation_snaps() {
    let mut app = Workbench::new(ChartDocument::empty("Camera", 120.).unwrap(), None);
    app.tool = Tool::Select;
    app.beat = 10.;
    let grid = test_grid(&app);
    let start = egui::pos2(grid.rect.center().x, 200.);
    for direction in [-1., 1.] {
        let before = app.beat;
        grid_events(&mut app, &[pointer(start, true)], false);
        for i in 1..=15 {
            let pos =
                start + egui::vec2(if i % 2 == 0 { 7. } else { -7. }, i as f32 * direction * 7.);
            grid_events(&mut app, &[egui::Event::PointerMoved(pos)], false);
        }
        grid_events(
            &mut app,
            &[pointer(start + egui::vec2(7., direction * 110.), false)],
            false,
        );
        assert_eq!(app.camera_x, 0.);
        assert_ne!(app.beat, before);
    }
    let beat = app.beat;
    grid_events(
        &mut app,
        &[
            pointer(start, true),
            egui::Event::PointerMoved(start + egui::vec2(24., 1.)),
            egui::Event::PointerMoved(start + egui::vec2(80., 60.)),
            pointer(start + egui::vec2(85., 62.), false),
        ],
        false,
    );
    assert_eq!(app.beat, beat);
    let step = 1350. / (app.lanes - 1) as f64;
    assert!((app.camera_x / step - (app.camera_x / step).round()).abs() < 1e-8);
    assert_ne!(app.camera_x, 0.);
    let camera = app.camera_x;
    let start = test_grid(&app).event_rect.center();
    grid_events(
        &mut app,
        &[
            pointer(start, true),
            egui::Event::PointerMoved(start + egui::vec2(55., 1.)),
            pointer(start + egui::vec2(65., 2.), false),
        ],
        false,
    );
    assert_eq!(
        app.camera_x, camera,
        "Event swipes must not move the note camera"
    );
}

#[test]
fn preview_note_navigation_finds_offscreen_source_and_selects_correct_line() {
    let mut app = Workbench::new(ChartDocument::empty("Find", 120.).unwrap(), None);
    app.execute(Command::AddLine {
        name: "Target".into(),
    });
    app.execute(Command::AddNote {
        line: 1,
        note: Note {
            x: 1800.,
            start_time: Beat::new(80, 0, 1).unwrap(),
            ..Default::default()
        },
    });
    app.preview.fullscreen = true;
    app.go_to_note(NoteId { line: 1, index: 0 });
    assert!(!app.preview.fullscreen);
    assert_eq!(app.line, 1);
    assert_eq!(app.selected, Some(NoteId { line: 1, index: 0 }));
    assert_eq!(app.camera_x, 1800.);
    assert_eq!(app.beat, 79.);
}

#[test]
fn horizontal_lock_keeps_camera_and_vertical_navigation_available() {
    let mut app = Workbench::new(ChartDocument::empty("Lock", 120.).unwrap(), None);
    app.options.lock_horizontal = true;
    app.camera_x = 405.;
    app.beat = 10.;
    let start = test_grid(&app).rect.center();
    grid_events(
        &mut app,
        &[
            pointer(start, true),
            egui::Event::PointerMoved(start + egui::vec2(100., 2.)),
            pointer(start + egui::vec2(110., 3.), false),
        ],
        false,
    );
    assert_eq!(app.camera_x, 405.);
    assert_eq!(app.beat, 10.);
    grid_events(
        &mut app,
        &[
            pointer(start, true),
            egui::Event::PointerMoved(start + egui::vec2(5., 80.)),
            pointer(start + egui::vec2(5., 90.), false),
        ],
        false,
    );
    assert_eq!(app.camera_x, 405.);
    assert!(app.beat > 10.);
}
