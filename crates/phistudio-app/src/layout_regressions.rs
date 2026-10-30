//! Check the contents of the actual workbench, including glyph layout and controls.
use crate::workbench::Workbench;
use egui::{Context, Event, FullOutput, Pos2, Rect, Shape, Vec2, pos2, vec2};
use phistudio_core::{ChartDocument, EventId, EventType, Language, ProjectLibrary, ProjectSummary};

fn draw(app: &mut Workbench, ctx: &Context, size: Vec2, events: Vec<Event>) -> FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ctx| app.ui(ctx, false),
    )
}
fn context() -> Context {
    let ctx = Context::default();
    ctx.set_fonts(crate::fonts::definitions());
    ctx.set_style(crate::theme::style());
    ctx
}
fn texts(output: &FullOutput) -> impl Iterator<Item = (&Rect, &egui::epaint::TextShape)> {
    output.shapes.iter().filter_map(|s| {
        if let Shape::Text(t) = &s.1 {
            Some((&s.0, t))
        } else {
            None
        }
    })
}
fn button(output: &FullOutput, caption: &str) -> Rect {
    let (_, text) = texts(output)
        .find(|(_, t)| t.galley.job.text == caption)
        .unwrap_or_else(|| panic!("Missing {caption}"));
    let center = text.pos + text.galley.size() * 0.5;
    output
        .shapes
        .iter()
        .filter_map(|s| match &s.1 {
            Shape::Rect(r) if r.rect.contains(center) && r.fill == crate::theme::raised() => {
                Some(r.rect)
            }
            _ => None,
        })
        .min_by(|a, b| a.area().total_cmp(&b.area()))
        .unwrap_or_else(|| panic!("No button behind {caption}"))
}
fn panel(ctx: &Context) -> Rect {
    egui::containers::panel::PanelState::load(ctx, egui::Id::new("context"))
        .unwrap()
        .rect
}
fn width(ctx: &Context, size: Vec2, width: f32) {
    ctx.data_mut(|d| {
        d.insert_persisted(
            egui::Id::new("context"),
            egui::containers::panel::PanelState {
                rect: Rect::from_min_size(pos2(size.x - width, 48.), vec2(width, size.y - 112.)),
            },
        )
    });
}
fn scroll(ctx: &Context, pane: Rect, y: f32) {
    let zone = crate::scroll::at(ctx, pane.center()).expect("Inspector scroll zone");
    let mut state = egui::scroll_area::State::load(ctx, zone.id).unwrap();
    state.offset.y = y.min(zone.max_offset);
    state.store(ctx, zone.id);
}

#[test]
fn docked_event_labels_remain_readable_in_all_languages_scales_and_widths() {
    for language in Language::ALL {
        let mut app = Workbench::new(
            ChartDocument::from_value(
                serde_json::from_str(include_str!(
                    "../../../assets/charts/phistudio-demo.rpe.json"
                ))
                .unwrap(),
            )
            .unwrap(),
            None,
        );
        app.options.language = language;
        for scale in [0.75, 0.9, 1., 1.1, 1.25, 1.5] {
            let size = vec2(2048. / 2.75 / scale, 920. / 2.75 / scale);
            let ctx = context();
            for w in [190., 260., 350., 400.] {
                width(&ctx, size, w);
                app.select_event(Some(EventId {
                    line: 0,
                    layer: 0,
                    kind: EventType::MoveX,
                    index: 0,
                }));
                for _ in 0..4 {
                    draw(&mut app, &ctx, size, vec![]);
                }
                let pane = panel(&ctx);
                let mut checked = 0;
                for offset in [None, Some(500.), Some(750.)] {
                    if let Some(y) = offset {
                        scroll(&ctx, pane, y);
                    }
                    let output = draw(&mut app, &ctx, size, vec![]);
                    let _scope = crate::i18n::Scope::enter(language);
                    let labels = [
                        "Start beat",
                        "End beat",
                        "Start value",
                        "End value",
                        "Constant value",
                        "Easing",
                        "Bezier",
                        "Easing left",
                        "Easing right",
                        "Link group",
                    ]
                    .map(crate::i18n::tr);
                    for (clip, t) in
                        texts(&output).filter(|(_, t)| labels.contains(&t.galley.job.text))
                    {
                        checked += 1;
                        assert!(
                            t.galley.rows.len() <= 2,
                            "{language:?}/{scale}/{w}: fragmented {:?}, {} rows",
                            t.galley.job.text,
                            t.galley.rows.len()
                        );
                        assert!(!t.galley.job.justify, "Justified field label");
                        assert!(
                            t.pos.x >= clip.left() - 0.5
                                && t.pos.x + t.galley.size().x <= clip.right() + 0.5,
                            "{language:?}/{scale}/{w}: clipped label {:?}",
                            t.galley.job.text
                        );
                    }
                }
                assert!(
                    checked >= 2,
                    "No real property labels checked: {language:?}/{scale}/{w}"
                );
            }
        }
    }
}

#[test]
fn sidebar_preview_fills_width_and_the_add_line_button_remains_centered_and_visible() {
    for language in Language::ALL {
        let mut app = Workbench::new(ChartDocument::empty("Sidebar", 120.).unwrap(), None);
        app.options.language = language;
        for scale in [0.75, 0.9, 1., 1.1, 1.25, 1.5] {
            let size = vec2(2048. / 2.75 / scale, 920. / 2.75 / scale);
            let ctx = context();
            for w in [190., 350., 400.] {
                width(&ctx, size, w);
                for _ in 0..4 {
                    draw(&mut app, &ctx, size, vec![]);
                }
                let pane = panel(&ctx);
                scroll(&ctx, pane, 0.);
                draw(&mut app, &ctx, size, vec![]);
                let gutter = ctx.style().spacing.scroll_bar_width
                    + ctx.style().spacing.scroll_bar_inner_margin
                    + ctx.style().spacing.scroll_bar_outer_margin;
                assert!(
                    (app.preview.rect.width() - (pane.width() - 16. - gutter)).abs() < 1.,
                    "{language:?}/{scale}/{w}: unused Preview space"
                );
                assert!((app.preview.rect.right() - (pane.right() - 8. - gutter)).abs() < 1.);
                assert!(
                    (app.preview.rect.height() * app.preview_aspect() - app.preview.rect.width())
                        .abs()
                        < 1.
                );
                scroll(&ctx, pane, app.preview.rect.height() + 50.);
                let output = draw(&mut app, &ctx, size, vec![]);
                let add = button(&output, "+");
                assert!(
                    (add.right() - app.preview.rect.right()).abs() < 1.,
                    "{language:?}/{scale}/{w}: + does not fit beside selector: {add:?}"
                );
                assert!((add.width() - add.height()).abs() < 0.5 && add.width() >= 31.);
                let (clip, t) = texts(&output)
                    .find(|(_, t)| t.galley.job.text == "+")
                    .unwrap();
                assert!(clip.contains(t.pos + t.galley.size() * 0.5));
                let zone = crate::scroll::at(&ctx, pane.center()).unwrap();
                assert_eq!(
                    egui::scroll_area::State::load(&ctx, zone.id)
                        .unwrap()
                        .offset
                        .x,
                    0.
                );
            }
        }
    }
}

#[test]
fn projects_actions_separator_and_rows_share_both_edges_with_scrollbar_outside() {
    for language in Language::ALL {
        let mut app = Workbench::new(ChartDocument::empty("Projects", 120.).unwrap(), None);
        app.options.language = language;
        app.show_projects = true;
        app.file = Some("unused/chart.json".into());
        app.library = Some(ProjectLibrary {
            projects: (0..8)
                .map(|i| ProjectSummary {
                    workspace: format!("unused/{i}").into(),
                    name: format!("Project {i}"),
                    source_name: "Chart.pez".into(),
                    notes: 1000,
                    last_opened_ms: 0,
                    cover: None,
                })
                .collect(),
            issues: vec![],
        });
        for scale in [0.75, 0.9, 1., 1.1, 1.25, 1.5] {
            let size = vec2(2048. / 2.75 / scale, 920. / 2.75 / scale);
            let ctx = context();
            for _ in 0..4 {
                draw(&mut app, &ctx, size, vec![]);
            }
            let output = draw(&mut app, &ctx, size, vec![]);
            let _scope = crate::i18n::Scope::enter(language);
            let create = button(&output, &crate::i18n::tr("+ Create chart"));
            let export = button(
                &output,
                &crate::i18n::tr(
                    if texts(&output)
                        .any(|(_, t)| t.galley.job.text == crate::i18n::tr("Export project…"))
                    {
                        "Export project…"
                    } else {
                        "Export…"
                    },
                ),
            );
            let delete = button(&output, &crate::i18n::tr("Delete"));
            let line = output
                .shapes
                .iter()
                .find_map(|s| match &s.1 {
                    Shape::LineSegment { points: p, .. }
                        if (p[0].y - p[1].y).abs() < 0.1
                            && (p[1].x - p[0].x).abs() > size.x * 0.6
                            && p[0].y > export.bottom()
                            && p[0].y < delete.top() =>
                    {
                        Some(Rect::from_two_pos(p[0], p[1]))
                    }
                    _ => None,
                })
                .expect("Project separator");
            assert!(
                (create.left() - line.left()).abs() < 1.
                    && (delete.right() - line.right()).abs() < 1.
                    && (export.right() - line.right()).abs() < 1.,
                "{language:?}/{scale}: actions {create:?}/{export:?}, line {line:?}, rows {delete:?}"
            );
            assert!(
                delete.top() < size.y - 32.,
                "No list viewport left at {language:?}/{scale}"
            );
            assert!(line.right() < size.x - 24., "Scrollbar gutter lost");
        }
    }
}

#[test]
fn compact_windows_menu_does_not_justify_playback_speed_letters() {
    for language in Language::ALL {
        for scale in [1.1, 1.25, 1.5] {
            let mut app = Workbench::new(ChartDocument::empty("Speed", 120.).unwrap(), None);
            app.options.language = language;
            let size = vec2(2048. / 2.75 / scale, 920. / 2.75 / scale);
            let ctx = context();
            for _ in 0..4 {
                draw(&mut app, &ctx, size, vec![]);
            }
            let output = draw(&mut app, &ctx, size, vec![]);
            let _scope = crate::i18n::Scope::enter(language);
            let (_, t) = texts(&output)
                .find(|(_, t)| t.galley.job.text == crate::i18n::tr("Windows"))
                .unwrap();
            let at = t.pos + t.galley.size() * 0.5;
            let event = |pressed| Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            draw(
                &mut app,
                &ctx,
                size,
                vec![Event::PointerMoved(at), event(true)],
            );
            draw(&mut app, &ctx, size, vec![event(false)]);
            let output = draw(&mut app, &ctx, size, vec![]);
            let (_, t) = texts(&output)
                .find(|(_, t)| t.galley.job.text == crate::i18n::tr("Sidebar"))
                .unwrap();
            let zone = crate::scroll::at(&ctx, t.pos + t.galley.size() * 0.5).unwrap();
            let mut state = egui::scroll_area::State::load(&ctx, zone.id).unwrap();
            state.offset.y = zone.max_offset;
            state.store(&ctx, zone.id);
            let output = draw(&mut app, &ctx, size, vec![]);
            let (_, t) = texts(&output)
                .find(|(_, t)| t.galley.job.text == crate::i18n::tr("Playback speed"))
                .expect("Playback speed caption after scrolling");
            assert!(
                !t.galley.job.justify && t.galley.job.halign == egui::Align::Min,
                "{language:?}/{scale}: spaced-out speed caption"
            );
        }
    }
}
