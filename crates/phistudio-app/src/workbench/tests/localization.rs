use super::*;

#[test]
fn language_selector_updates_open_ui_persists_and_preserves_chart_state() {
    fn render(
        app: &mut Workbench,
        ctx: &egui::Context,
        size: Vec2,
        events: Vec<egui::Event>,
    ) -> Vec<(egui::Pos2, String)> {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| app.ui(ctx, false),
        )
        .shapes
        .into_iter()
        .filter_map(|shape| {
            if let egui::Shape::Text(t) = shape.1 {
                let center = t.pos + t.galley.size() * 0.5;
                shape
                    .0
                    .contains(center)
                    .then(|| (center, t.galley.job.text.clone()))
            } else {
                None
            }
        })
        .collect()
    }
    fn click(app: &mut Workbench, ctx: &egui::Context, size: Vec2, at: egui::Pos2) {
        render(
            app,
            ctx,
            size,
            vec![egui::Event::PointerMoved(at), pointer(at, true)],
        );
        render(app, ctx, size, vec![pointer(at, false)]);
        render(app, ctx, size, vec![]);
    }
    fn position(texts: &[(egui::Pos2, String)], label: &str) -> egui::Pos2 {
        texts
            .iter()
            .find(|(_, text)| text == label)
            .unwrap_or_else(|| panic!("Missing {label}: {texts:?}"))
            .0
    }
    for size in [Vec2::new(873., 393.), Vec2::new(1194., 834.)] {
        for language in phistudio_core::Language::ALL
            .into_iter()
            .filter(|l| *l != phistudio_core::Language::English)
        {
            let root = tempfile::tempdir().unwrap();
            let mut app = Workbench::new(ChartDocument::empty("Settings", 120.).unwrap(), None);
            let mut writer = crate::settings::Preferences::new(Some(root.path().into()));
            writer.update(&mut app);
            app.show_settings = true;
            app.beat = 5.;
            app.playing = true;
            let original = app.editor.document().as_json().clone();
            let revision = app.editor.revision();
            let ctx = egui::Context::default();
            ctx.set_style(crate::theme::style());
            ctx.set_fonts(crate::fonts::definitions());
            for _ in 0..3 {
                render(&mut app, &ctx, size, vec![]);
            }
            let mut texts = render(&mut app, &ctx, size, vec![]);
            for _ in 0..8 {
                if texts.iter().any(|(_, text)| text == "English") {
                    break;
                }
                texts = render(
                    &mut app,
                    &ctx,
                    size,
                    vec![
                        egui::Event::PointerMoved(egui::pos2(160., 190.)),
                        egui::Event::Scroll(egui::vec2(0., 90.)),
                    ],
                );
            }
            click(&mut app, &ctx, size, position(&texts, "English"));
            let mut texts = render(&mut app, &ctx, size, vec![]);
            for _ in 0..8 {
                if texts.iter().any(|(_, text)| text == language.native_name()) {
                    break;
                }
                texts = render(
                    &mut app,
                    &ctx,
                    size,
                    vec![
                        egui::Event::PointerMoved(egui::pos2(160., 190.)),
                        egui::Event::Scroll(egui::vec2(0., -90.)),
                    ],
                );
            }
            click(
                &mut app,
                &ctx,
                size,
                position(&texts, language.native_name()),
            );
            assert_eq!(app.options.language, language);
            let texts = render(&mut app, &ctx, size, vec![]);
            {
                let _language = crate::i18n::Scope::enter(language);
                for label in ["Settings", "Language", "Preview"] {
                    position(&texts, &crate::i18n::tr(label));
                }
                position(&texts, language.native_name());
            }
            assert!(app.show_settings);
            assert_eq!(app.editor.document().as_json(), &original);
            assert_eq!(app.editor.revision(), revision);
            assert_eq!(app.beat, 5.);
            assert!(app.playing);
            writer.flush(&app).unwrap();
            let preferences = phistudio_core::load_editor_preferences(root.path()).unwrap();
            assert_eq!(preferences.options.language, language);
            let mut restored = Workbench::new(ChartDocument::empty("Another", 120.).unwrap(), None);
            restored.apply_preferences(&preferences);
            assert_eq!(restored.options.language, language);
            click(
                &mut app,
                &ctx,
                size,
                position(&texts, language.native_name()),
            );
            let mut texts = render(&mut app, &ctx, size, vec![]);
            for _ in 0..8 {
                if texts.iter().any(|(_, text)| text == "English") {
                    break;
                }
                texts = render(
                    &mut app,
                    &ctx,
                    size,
                    vec![
                        egui::Event::PointerMoved(egui::pos2(160., 190.)),
                        egui::Event::Scroll(egui::vec2(0., 90.)),
                    ],
                );
            }
            click(&mut app, &ctx, size, position(&texts, "English"));
            let texts = render(&mut app, &ctx, size, vec![]);
            position(&texts, "Language");
            assert_eq!(app.options.language, phistudio_core::Language::English);
            assert_eq!(app.editor.document().as_json(), &original);
            // Rendering one localized workbench cannot change another app's language.
            assert_eq!(crate::i18n::tr("Save"), "Save");
        }
    }
}

#[test]
fn line_state_popup_localizes_every_item_and_only_changes_the_filter() {
    fn render(
        app: &mut Workbench,
        ctx: &egui::Context,
        size: Vec2,
        events: Vec<egui::Event>,
    ) -> Vec<(egui::Pos2, String)> {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| app.ui(ctx, false),
        )
        .shapes
        .into_iter()
        .filter_map(|shape| {
            if let egui::Shape::Text(t) = shape.1 {
                let center = t.pos + t.galley.size() * 0.5;
                shape
                    .0
                    .contains(center)
                    .then(|| (center, t.galley.job.text.clone()))
            } else {
                None
            }
        })
        .collect()
    }
    fn click(app: &mut Workbench, ctx: &egui::Context, size: Vec2, at: egui::Pos2) {
        render(app, ctx, size, vec![egui::Event::PointerMoved(at)]);
        render(app, ctx, size, vec![pointer(at, true)]);
        render(app, ctx, size, vec![pointer(at, false)]);
        render(app, ctx, size, vec![]);
    }
    fn position(texts: &[(egui::Pos2, String)], label: &str) -> egui::Pos2 {
        texts
            .iter()
            .rev()
            .find(|(_, text)| text == label)
            .unwrap_or_else(|| panic!("Missing {label}: {texts:?}"))
            .0
    }
    for size in [Vec2::new(873., 393.), Vec2::new(1194., 834.)] {
        for language in phistudio_core::Language::ALL {
            let mut app = Workbench::new(ChartDocument::empty("Raw title", 120.).unwrap(), None);
            app.options.language = language;
            app.beat = 5.;
            app.playing = true;
            let original = app.editor.document().as_json().clone();
            let revision = app.editor.revision();
            let ctx = egui::Context::default();
            ctx.set_style(crate::theme::style());
            ctx.set_fonts(crate::fonts::definitions());
            let labels = {
                let _language = crate::i18n::Scope::enter(language);
                [
                    "All line states",
                    "Visible lines",
                    "Visible notes",
                    "Active events",
                ]
                .map(crate::i18n::tr)
            };
            for _ in 0..3 {
                render(&mut app, &ctx, size, vec![]);
            }
            for (filter, label) in labels.iter().enumerate() {
                let texts = render(&mut app, &ctx, size, vec![]);
                let at = position(&texts, &labels[app.advanced.line_filter]);
                click(&mut app, &ctx, size, at);
                for _ in 0..3 {
                    render(&mut app, &ctx, size, vec![]);
                }
                let texts = render(&mut app, &ctx, size, vec![]);
                for expected in &labels {
                    position(&texts, expected);
                }
                click(&mut app, &ctx, size, position(&texts, label));
                assert_eq!(
                    app.advanced.line_filter,
                    filter,
                    "{} {size:?} choosing {label}: {texts:?}",
                    language.code()
                );
                assert_eq!(app.editor.document().as_json(), &original);
                assert_eq!(app.editor.revision(), revision);
                assert_eq!(app.beat, 5.);
                assert!(app.playing);
            }
        }
    }
}

#[test]
fn note_sidebar_stays_bounded_in_every_language_at_large_interface_scales() {
    for language in phistudio_core::Language::ALL {
        for scale in [1., 1.25, 1.5] {
            let size = Vec2::new(873. / scale, 393. / scale);
            let ctx = egui::Context::default();
            ctx.set_fonts(crate::fonts::definitions());
            let mut app = Workbench::new(ChartDocument::empty("Pane", 120.).unwrap(), None);
            app.options.language = language;
            app.execute(Command::AddNote {
                line: 0,
                note: Note::default(),
            });
            app.select(Some(NoteId { line: 0, index: 0 }));
            for width in [190., 260., 400., 190.] {
                ctx.data_mut(|d| {
                    d.insert_persisted(
                        egui::Id::new("context"),
                        egui::containers::panel::PanelState {
                            rect: egui::Rect::from_min_size(
                                egui::pos2(size.x - width, 40.),
                                Vec2::new(width, size.y - 104.),
                            ),
                        },
                    )
                });
                for _ in 0..4 {
                    frame(&mut app, &ctx, size, vec![], false);
                }
                let panel =
                    egui::containers::panel::PanelState::load(&ctx, egui::Id::new("context"))
                        .unwrap()
                        .rect;
                assert!(
                    panel.right() <= size.x + 0.1 && panel.left() >= 0.,
                    "{language:?} scale {scale} width {width}: {panel:?}, screen {size:?}"
                );
                assert!(
                    app.grid_area.width() >= 200.,
                    "No chart left at {language:?}/{scale}: {:?}",
                    app.grid_area
                );
            }
        }
    }
}

#[test]
fn windows_remain_inside_phone_at_100_to_150_percent_in_every_language() {
    let mut failures = vec![];
    for language in phistudio_core::Language::ALL {
        for scale in [1., 1.25, 1.5] {
            let size = Vec2::new(873. / scale, 393. / scale);
            for view in 0..15 {
                let ctx = egui::Context::default();
                ctx.set_fonts(crate::fonts::definitions());
                let mut app = Workbench::new(ChartDocument::empty("Windows", 120.).unwrap(), None);
                app.options.language = language;
                app.execute(Command::AddNote {
                    line: 0,
                    note: Note::default(),
                });
                app.select(Some(NoteId { line: 0, index: 0 }));
                match view {
                    0 => app.show_settings = true,
                    1 => app.show_edit = true,
                    2..=6 => {
                        app.tools.show = true;
                        app.tools.page = match view {
                            2 => crate::toolkit::Page::Selection,
                            3 => crate::toolkit::Page::Motion,
                            4 => crate::toolkit::Page::Text,
                            5 => crate::toolkit::Page::Events,
                            _ => crate::toolkit::Page::Values,
                        };
                    }
                    7 => app.show_line_properties = true,
                    8 => app.show_bpm = true,
                    9 => app.show_grid_settings = true,
                    10 => app.compact_properties = true,
                    11 => app.advanced.history = true,
                    12 => app.open_chart_properties(),
                    13 => app.playtest.setup = true,
                    _ => app.open_video(),
                }
                for _ in 0..6 {
                    frame(&mut app, &ctx, size, vec![], false);
                }
                let rect = ctx.used_rect();
                if rect.left() < -0.5
                    || rect.top() < -0.5
                    || rect.right() > size.x + 0.5
                    || rect.bottom() > size.y + 0.5
                {
                    failures.push(format!(
                        "{language:?}/{scale}, window {view}: {rect:?}, screen {size:?}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
