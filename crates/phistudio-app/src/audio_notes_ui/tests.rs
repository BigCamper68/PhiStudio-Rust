use super::*;
use phistudio_core::{ChartDocument, Command, Language};
fn app() -> Workbench {
    let mut app = Workbench::new(ChartDocument::empty("Generator", 120.).unwrap(), None);
    app.project_options.music = None;
    app.audio_guide.seed_generation_test(
        app.session,
        app.asset_revision,
        None,
        &app.options.audio_guide,
    );
    app.open_audio_notes();
    app
}
fn wait(app: &mut Workbench) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while app.audio_notes.pending.is_some() && std::time::Instant::now() < deadline {
        app.poll_audio_notes();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(app.audio_notes.pending.is_none());
    assert!(
        app.audio_notes.error.is_none(),
        "{:?}",
        app.audio_notes.error
    );
}
#[test]
fn worker_preview_is_passive_insert_is_one_undo_and_repetition_skips_existing_notes() {
    let mut app = app();
    let original = app.editor.document().as_json().clone();
    app.editor.mark_saved();
    assert!(!app.options.audio_guide.waveform && !app.options.audio_guide.attacks);
    app.build_audio_note_preview();
    wait(&mut app);
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(!app.editor.is_dirty());
    assert_eq!(
        app.audio_notes.preview.as_ref().unwrap().1.plan.notes.len(),
        3
    );
    app.multi_line.enabled = true;
    let ctx = egui::Context::default();
    for _ in 0..3 {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(582., 262.),
                )),
                ..Default::default()
            },
            |ctx| app.ui(ctx, false),
        );
    }
    app.focus_audio_note_preview();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(582., 262.),
            )),
            ..Default::default()
        },
        |ctx| app.ui(ctx, false),
    );
    let g = app.multi_line.base.unwrap();
    let first = app.audio_notes.preview.as_ref().unwrap().1.plan.notes[0]
        .note
        .start_time
        .as_f64();
    assert!(
        g.rect.contains(egui::pos2(g.rect.center().x, g.y(first))),
        "Compact focus must reveal the first ghost"
    );
    app.apply_audio_note_preview();
    assert_eq!(app.editor.document().note_count(), 3);
    assert!(!app.audio_notes.show);
    assert!(app.editor.undo());
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(!app.editor.is_dirty());
    assert!(app.editor.redo());
    app.open_audio_notes();
    app.build_audio_note_preview();
    wait(&mut app);
    assert!(
        app.audio_notes
            .preview
            .as_ref()
            .unwrap()
            .1
            .plan
            .notes
            .is_empty()
    );
}
#[test]
fn stale_doc_offset_options_and_source_never_commit_old_previews_and_back_cancels() {
    let mut app = app();
    for what in 0..4 {
        app.build_audio_note_preview();
        wait(&mut app);
        match what {
            0 => app.execute(Command::RenameChart("Changed".into())),
            1 => app.project_options.manifest_offset_ms += 100,
            2 => app.options.audio_notes.x += 100.,
            _ => {
                app.audio_guide.seed_generation_test(
                    app.session,
                    app.asset_revision,
                    None,
                    &app.options.audio_guide,
                );
            }
        }
        app.apply_audio_note_preview();
        assert_eq!(app.editor.document().note_count(), 0);
        assert!(app.audio_notes.error.is_some());
    }
    app.build_audio_note_preview();
    let cancel = Arc::clone(&app.audio_notes.pending.as_ref().unwrap().cancel);
    app.back();
    assert!(cancel.load(Ordering::Acquire));
    assert!(!app.audio_notes.show);
    assert_eq!(app.editor.document().note_count(), 0);
    assert!(app.audio_notes.worker.is_some());
    app.audio_notes.start = "99".into();
    app.audio_notes.hide_ghosts = true;
    app.replace_document(ChartDocument::empty("Other", 130.).unwrap(), None, None);
    assert!(
        app.audio_notes.start.is_empty()
            && app.audio_notes.end.is_empty()
            && !app.audio_notes.hide_ghosts
    );
    assert!(
        app.audio_notes.worker.is_some()
            && !app.audio_notes.show
            && app.audio_notes.preview.is_none()
    );
}
#[test]
fn generation_window_actions_fit_phone_in_all_languages_and_scales_without_edits() {
    let mut app = app();
    let source = app.editor.document().as_json().clone();
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::fonts::definitions());
    ctx.set_style(crate::theme::style());
    for lang in Language::ALL {
        app.options.language = lang;
        for scale in [1., 1.5] {
            let size = egui::vec2(873. / scale, 393. / scale);
            for _ in 0..4 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| app.ui(ctx, false),
                );
            }
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| app.ui(ctx, false),
            );
            let rect = crate::window::visible_rect(&ctx, egui::Id::new("Notes from audio markers"))
                .unwrap();
            assert!(
                rect.left() >= -1.
                    && rect.right() <= size.x + 1.
                    && rect.top() >= -1.
                    && rect.bottom() <= size.y + 1.,
                "{lang:?}/{scale}: {rect:?}/{size:?}"
            );
            let _scope = crate::i18n::Scope::enter(lang);
            for text in [
                crate::i18n::tr("Build note preview"),
                crate::i18n::format("Insert {0} notes", &["0".into()]),
            ] {
                let found=output.shapes.iter().any(|s|matches!(&s.1,egui::Shape::Text(t) if t.galley.job.text==text && t.pos.y>=0. && t.pos.y+t.galley.size().y<=size.y));
                assert!(found, "Missing reachable action {lang:?}/{scale}: {text}");
            }
        }
    }
    assert_eq!(app.editor.document().as_json(), &source);
    assert!(!app.editor.is_dirty());
}

#[test]
fn expanded_generator_controls_keep_custom_ranges_and_large_seeds_inside_compact_forms() {
    let mut app = app();
    app.audio_notes.range = Range::Custom;
    app.audio_notes.start = "0:123/1920".into();
    app.audio_notes.end = "500:123/1920".into();
    app.options.audio_notes.per_band = true;
    app.options.audio_notes.band_kinds[1] = phistudio_core::NoteKind::Hold;
    app.options.audio_notes.position = phistudio_core::tools::AudioPosition::Random;
    app.options.audio_notes.seed = u32::MAX as u64;
    for lang in [Language::English, Language::Russian, Language::Japanese] {
        app.options.language = lang;
        let _scope = crate::i18n::Scope::enter(lang);
        for width in [300., 500.] {
            let ctx = egui::Context::default();
            ctx.set_fonts(crate::fonts::definitions());
            ctx.set_style(crate::theme::style());
            let mut measured = 0.;
            for _ in 0..3 {
                let _=ctx.run(egui::RawInput {screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(800.,1600.))),..Default::default()},|ctx|{
                    egui::CentralPanel::default().show(ctx,|ui|{
                        ui.allocate_ui_with_layout(egui::vec2(width,1500.),egui::Layout::top_down(egui::Align::Min),|ui|{
                            ui.set_max_width(width);ui.style_mut().wrap=Some(true);
                            for label in ["Attack selection","Density and timing","Note types and properties","Horizontal placement","Existing notes and overlaps"] {
                                let mut state=egui::collapsing_header::CollapsingState::load_with_default_open(ctx,ui.make_persistent_id(crate::i18n::tr(label)),true);state.set_open(true);state.store(ctx);
                            }
                            app.audio_note_controls(ui);measured=ui.min_rect().width();
                        });
                    });
                });
            }
            assert!(
                measured <= width + 1.,
                "Expanded form overflow: {lang:?}/{width}: {measured}"
            );
        }
    }
}

#[test]
fn generator_launch_from_settings_replaces_settings_and_preserves_the_document() {
    let mut app = app();
    app.audio_notes.close();
    app.windows.settings = true;
    let before = app.editor.document().as_json().clone();
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    fn frame(
        app: &mut Workbench,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(873., 393.),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.ui(ctx, false),
        )
    }
    fn find(app: &mut Workbench, ctx: &egui::Context, text: &str) -> egui::Pos2 {
        for part in 0..=20 {
            for _ in 0..3 {
                let _ = frame(app, ctx, vec![]);
            }
            let rect = crate::window::visible_rect(ctx, egui::Id::new("Settings")).unwrap();
            let zone = ctx
                .data_mut(|d| {
                    d.get_temp::<Vec<crate::scroll::Zone>>(egui::Id::new("phistudio-scroll-zones"))
                })
                .unwrap()
                .into_iter()
                .find(|z| z.axis == 1 && z.layer != egui::LayerId::background())
                .unwrap();
            let mut state = egui::scroll_area::State::load(ctx, zone.id).unwrap();
            state.offset.y = zone.max_offset * part as f32 / 20.;
            state.store(ctx, zone.id);
            let out = frame(app, ctx, vec![]);
            for s in out.shapes {
                if let egui::Shape::Text(t) = s.1
                    && t.galley.job.text == text
                {
                    let p = t.pos + t.galley.size() * 0.5;
                    if rect.contains(p) && p.y > rect.top() + 36. && s.0.contains(p) {
                        return p;
                    }
                }
            }
        }
        panic!("Settings action not found: {text}");
    }
    fn click(app: &mut Workbench, ctx: &egui::Context, p: egui::Pos2) {
        for pressed in [true, false] {
            let _ = frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(p),
                    egui::Event::PointerButton {
                        pos: p,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
    }
    for _ in 0..4 {
        let _ = frame(&mut app, &ctx, vec![]);
    }
    let p = find(&mut app, &ctx, "Audio guide");
    click(&mut app, &ctx, p);
    let p = find(&mut app, &ctx, "Generate notes from markers…");
    click(&mut app, &ctx, p);
    assert!(app.audio_notes.show && !app.windows.settings && !app.audio_guide.show);
    assert_eq!(app.editor.document().as_json(), &before);
}

fn input_frame(
    app: &mut Workbench,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(873., 393.),
            )),
            events,
            ..Default::default()
        },
        |ctx| app.ui(ctx, false),
    )
}
fn text_position(output: &egui::FullOutput, text: &str) -> egui::Pos2 {
    output
        .shapes
        .iter()
        .find_map(|s| {
            if let egui::Shape::Text(t) = &s.1
                && t.galley.job.text == text
            {
                Some(t.pos + t.galley.size() * 0.5)
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("Missing text {text}"))
}
fn input_click(app: &mut Workbench, ctx: &egui::Context, p: egui::Pos2) {
    for pressed in [true, false] {
        input_frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(p),
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
}
#[test]
fn typing_partial_custom_beats_keeps_the_same_field_focused() {
    let mut app = app();
    app.audio_notes.range = Range::Custom;
    app.audio_notes.start = "123".into();
    app.audio_notes.end = "987".into();
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    for _ in 0..5 {
        input_frame(&mut app, &ctx, vec![]);
    }
    let out = input_frame(&mut app, &ctx, vec![]);
    let pos = text_position(&out, "123");
    input_click(&mut app, &ctx, pos);
    input_frame(&mut app, &ctx, vec![]);
    let focused = ctx
        .memory(|m| m.focus())
        .expect("Start beat must receive focus");
    let mut state = egui::TextEdit::load_state(&ctx, focused).unwrap();
    state.set_ccursor_range(Some(egui::text::CCursorRange::two(
        egui::text::CCursor::new(0),
        egui::text::CCursor::new(100),
    )));
    state.store(&ctx, focused);
    for s in ["-", "1", "/", "2"] {
        input_frame(&mut app, &ctx, vec![egui::Event::Text(s.into())]);
        input_frame(&mut app, &ctx, vec![]);
        assert_eq!(
            ctx.memory(|m| m.focus()),
            Some(focused),
            "Focus changed after {s}"
        );
        assert!(ctx.wants_keyboard_input(), "Keyboard dismissed after {s}");
    }
    assert_eq!(app.audio_notes.start, "-1/2");
    assert_eq!(app.audio_notes.end, "987");
}

#[test]
fn dragging_generator_scrollbar_does_not_move_or_resize_the_window() {
    let mut app = app();
    let ctx = egui::Context::default();
    let mut style = crate::theme::style();
    style.animation_time = 0.;
    ctx.set_style(style);
    for _ in 0..5 {
        input_frame(&mut app, &ctx, vec![]);
    }
    let out = input_frame(&mut app, &ctx, vec![]);
    input_click(&mut app, &ctx, text_position(&out, "Attack selection"));
    for _ in 0..5 {
        input_frame(&mut app, &ctx, vec![]);
    }
    let before =
        crate::window::visible_rect(&ctx, egui::Id::new("Notes from audio markers")).unwrap();
    let zone = ctx
        .data_mut(|d| {
            d.get_temp::<Vec<crate::scroll::Zone>>(egui::Id::new("phistudio-scroll-zones"))
        })
        .unwrap()
        .into_iter()
        .find(|z| z.layer != egui::LayerId::background() && z.axis == 1)
        .unwrap();
    let p = egui::pos2(
        zone.rect.right()
            + ctx.style().spacing.scroll_bar_inner_margin
            + ctx.style().spacing.scroll_bar_width * 0.5,
        zone.rect.top() + 8.,
    );
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        pressed,
        button: egui::PointerButton::Primary,
        modifiers: Default::default(),
    };
    input_frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(p), button(p, true)],
    );
    for d in [10., 30., 50., 80., 100., 80., 30., 0.] {
        input_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(p + egui::vec2(0., d))],
        );
        let now =
            crate::window::visible_rect(&ctx, egui::Id::new("Notes from audio markers")).unwrap();
        assert!(
            (now.min - before.min).length() < 1. && (now.size() - before.size()).length() < 1.,
            "Scrollbar changed window: {before:?} -> {now:?}"
        );
    }
    input_frame(&mut app, &ctx, vec![button(p, false)]);
    assert!(!ctx.wants_keyboard_input());
}

#[test]
fn changing_attack_slider_during_background_selection_keeps_the_control_in_place() {
    let mut app = app();
    let clip = app.audio_guide.seed_generation_test(
        app.session,
        app.asset_revision,
        None,
        &app.options.audio_guide,
    );
    app.audio_guide.generation_requested = true;
    let ctx = egui::Context::default();
    let mut style = crate::theme::style();
    style.animation_time = 0.;
    ctx.set_style(style);
    for _ in 0..5 {
        input_frame(&mut app, &ctx, vec![]);
    }
    let out = input_frame(&mut app, &ctx, vec![]);
    input_click(&mut app, &ctx, text_position(&out, "Attack selection"));
    for _ in 0..5 {
        input_frame(&mut app, &ctx, vec![]);
    }
    let out = input_frame(&mut app, &ctx, vec![]);
    let label = out
        .shapes
        .iter()
        .find_map(|s| match &s.1 {
            egui::Shape::Text(t) if t.galley.job.text == "Attack sensitivity" => {
                Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
            }
            _ => None,
        })
        .unwrap();
    let p = egui::pos2(
        label.left() + 40.,
        label.bottom()
            + ctx.style().spacing.item_spacing.y
            + ctx.style().spacing.interact_size.y * 0.5,
    );
    let before = app.options.audio_guide.sensitivity;
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        pressed,
        button: egui::PointerButton::Primary,
        modifiers: Default::default(),
    };
    input_frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(p), button(p, true)],
    );
    for dx in [5., 15., 25., 35., 45.] {
        app.audio_guide.update(
            app.session,
            app.asset_revision,
            None,
            Some(clip.clone()),
            &app.options.audio_guide,
        );
        let out = input_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(p + egui::vec2(dx, 0.))],
        );
        let now = text_position(&out, "Attack sensitivity");
        assert!(
            (now - label.center()).length() < 1.,
            "Background selection moved slider: {label:?} -> {now:?}"
        );
    }
    input_frame(&mut app, &ctx, vec![button(p + egui::vec2(45., 0.), false)]);
    assert_ne!(
        app.options.audio_guide.sensitivity, before,
        "The test must exercise the actual slider"
    );
}

fn find_scrolled_text(app: &mut Workbench, ctx: &egui::Context, text: &str) -> egui::Pos2 {
    for part in 0..=30 {
        let zones = ctx
            .data_mut(|d| {
                d.get_temp::<Vec<crate::scroll::Zone>>(egui::Id::new("phistudio-scroll-zones"))
            })
            .unwrap();
        let zone = zones
            .into_iter()
            .find(|z| z.layer != egui::LayerId::background() && z.axis == 1)
            .unwrap();
        let mut state = egui::scroll_area::State::load(ctx, zone.id).unwrap();
        state.offset.y = zone.max_offset * part as f32 / 30.;
        state.store(ctx, zone.id);
        let out = input_frame(app, ctx, vec![]);
        for s in out.shapes {
            if let egui::Shape::Text(t) = s.1
                && t.galley.job.text == text
            {
                let p = t.pos + t.galley.size() * 0.5;
                if s.0.contains(p) && zone.rect.shrink(12.).contains(p) {
                    return p;
                }
            }
        }
    }
    panic!("Missing visible generator control: {text}");
}
#[test]
fn numeric_entry_survives_analysis_changes_without_switching_fields() {
    let mut app = app();
    let clip = app.audio_guide.seed_generation_test(
        app.session,
        app.asset_revision,
        None,
        &app.options.audio_guide,
    );
    app.audio_guide.generation_requested = true;
    let ctx = egui::Context::default();
    let mut style = crate::theme::style();
    style.animation_time = 0.;
    ctx.set_style(style);
    for _ in 0..5 {
        input_frame(&mut app, &ctx, vec![]);
    }
    let p = find_scrolled_text(&mut app, &ctx, "Density and timing");
    input_click(&mut app, &ctx, p);
    for _ in 0..5 {
        input_frame(&mut app, &ctx, vec![]);
    }
    let p = find_scrolled_text(&mut app, &ctx, "20000");
    input_click(&mut app, &ctx, p);
    input_frame(&mut app, &ctx, vec![]);
    let id = ctx.memory(|m| m.focus()).expect("Numeric field focus");
    let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
    state.set_ccursor_range(Some(egui::text::CCursorRange::two(
        egui::text::CCursor::new(0),
        egui::text::CCursor::new(100),
    )));
    state.store(&ctx, id);
    for c in ["1", "2", "3"] {
        app.options.audio_guide.sensitivity += 0.01;
        app.audio_guide.update(
            app.session,
            app.asset_revision,
            None,
            Some(clip.clone()),
            &app.options.audio_guide,
        );
        input_frame(&mut app, &ctx, vec![egui::Event::Text(c.into())]);
        input_frame(&mut app, &ctx, vec![]);
        assert_eq!(
            ctx.memory(|m| m.focus()),
            Some(id),
            "Numeric focus changed after {c}"
        );
        assert!(ctx.wants_keyboard_input());
        assert_eq!(
            app.options.audio_notes.max_notes, 20000,
            "Commit on Enter, not per character"
        );
    }
    input_frame(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
    );
    assert_eq!(app.options.audio_notes.max_notes, 123);
    assert_eq!(app.options.audio_notes.every, 1);
    assert_eq!(app.options.audio_notes.minimum_gap_beats, 0.);
}

#[test]
fn preview_audio_is_passive_and_stops_on_stale_input_close_or_fake_notes() {
    let mut app = app();
    app.options.audio_notes.per_band = true;
    app.build_audio_note_preview();
    wait(&mut app);
    let before = app.editor.document().as_json().clone();
    let mut heard = Vec::new();
    let play = |kind| {
        heard.push(kind);
        Ok(())
    };
    assert_eq!(app.audition_audio_notes(0., false, play).unwrap(), 0);
    app.transport.playing = true;
    assert_eq!(
        app.audition_audio_notes(0.3, false, |k| {
            heard.push(k);
            Ok(())
        })
        .unwrap(),
        1
    );
    app.audio_notes.hide_ghosts = true;
    assert_eq!(
        app.audition_audio_notes(0.8, false, |k| {
            heard.push(k);
            Ok(())
        })
        .unwrap(),
        1
    );
    assert_eq!(
        heard,
        [
            phistudio_core::NoteKind::Tap,
            phistudio_core::NoteKind::Drag
        ]
    );
    app.project_options.manifest_offset_ms += 100;
    assert_eq!(
        app.audition_audio_notes(1.5, false, |_| panic!("Stale preview played"))
            .unwrap(),
        0
    );
    app.audio_notes.close();
    assert_eq!(
        app.audition_audio_notes(2., false, |_| panic!("Closed preview played"))
            .unwrap(),
        0
    );
    assert_eq!(app.editor.document().as_json(), &before);
    assert!(!app.editor.is_dirty());
    app.open_audio_notes();
    app.options.audio_notes.fake = true;
    app.options.audio_notes.kind = phistudio_core::NoteKind::Hold;
    app.options.audio_notes.per_band = false;
    app.build_audio_note_preview();
    wait(&mut app);
    assert!(
        !app.audio_notes
            .preview
            .as_ref()
            .unwrap()
            .1
            .plan
            .notes
            .is_empty()
    );
    for t in [0., 0.3, 0.8, 1.5, 2.] {
        assert_eq!(
            app.audition_audio_notes(t, false, |_| panic!("Fake preview played"))
                .unwrap(),
            0
        );
    }
    assert_eq!(app.editor.document().as_json(), &before);
}
