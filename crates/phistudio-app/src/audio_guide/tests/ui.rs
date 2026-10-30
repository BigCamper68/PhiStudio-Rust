use super::*;
use crate::workbench::Workbench;
use egui::{FullOutput, Pos2, Rect, Vec2, vec2};
use phistudio_core::{ChartDocument, Language};

fn frame(
    app: &mut Workbench,
    ctx: &egui::Context,
    size: Vec2,
    events: Vec<egui::Event>,
) -> FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ctx| app.ui(ctx, false),
    )
}

fn text_rect(
    out: &FullOutput,
    ctx: &egui::Context,
    text: &str,
    order: egui::Order,
) -> Option<Rect> {
    // Later shapes belong to the popup; its range names also occur in the
    // colored legend underneath it.
    out.shapes.iter().rev().find_map(|clipped| {
        let (clip, shape) = (&clipped.0, &clipped.1);
        if let egui::Shape::Text(t) = shape {
            let rect = Rect::from_min_size(t.pos, t.galley.size());
            (t.galley.job.text == text
                && clip.contains(rect.center())
                && ctx
                    .layer_id_at(rect.center())
                    .is_some_and(|layer| layer.order == order))
            .then_some(rect)
        } else {
            None
        }
    })
}

#[test]
fn guide_window_fits_all_languages_and_frequency_popup_accepts_touch_above_it() {
    for language in Language::ALL {
        let _language = crate::i18n::Scope::enter(language);
        for size in [vec2(873., 393.), vec2(582., 262.)] {
            let ctx = egui::Context::default();
            ctx.set_style(crate::theme::style());
            ctx.set_fonts(crate::fonts::definitions());
            let mut app = Workbench::new(ChartDocument::empty("Audio UI", 120.).unwrap(), None);
            app.options.language = language;
            app.audio_guide.show = true;
            let original = app.editor.document().as_json().clone();
            for _ in 0..4 {
                frame(&mut app, &ctx, size, vec![]);
            }
            let window = crate::window::visible_rect(&ctx, egui::Id::new("Audio guide")).unwrap();
            assert!(window.width() >= 400., "{language:?}/{size:?}: {window:?}");
            assert!(window.width() < size.x, "{language:?}/{size:?}: {window:?}");
            let zones = ctx.data_mut(|d| {
                d.get_temp::<Vec<crate::scroll::Zone>>(egui::Id::new("phistudio-scroll-zones"))
                    .unwrap_or_default()
            });
            let zone = zones.into_iter().find(|zone| {
                window.contains_rect(zone.rect) && zone.layer.order == egui::Order::Middle
            });
            if let Some(zone) = zone {
                let mut state = egui::scroll_area::State::load(&ctx, zone.id).unwrap();
                assert_eq!(state.offset.x, 0.);
                state.offset.y = zone.max_offset;
                state.store(&ctx, zone.id);
            }
            let mut out = frame(&mut app, &ctx, size, vec![]);
            // Fully visible rows must fit horizontally even in Russian/Spanish.
            for clipped in &out.shapes {
                let (clip, shape) = (&clipped.0, &clipped.1);
                if let egui::Shape::Text(t) = shape {
                    let rect = Rect::from_min_size(t.pos, t.galley.size());
                    if window.contains_rect(*clip)
                        && clip.width() >= window.width() - 40.
                        && clip.contains(rect.center())
                    {
                        assert!(
                            rect.left() >= clip.left() - 1. && rect.right() <= clip.right() + 1.,
                            "{language:?}/{size:?}: clipped text {}: {rect:?}, {clip:?}",
                            t.galley.job.text
                        );
                    }
                }
            }
            // A compact viewport can need a little upward scrolling to show
            // the frequency row above the footer/reset controls.
            for _ in 0..8 {
                if text_rect(
                    &out,
                    &ctx,
                    &crate::i18n::tr("All frequencies"),
                    egui::Order::Middle,
                )
                .is_some()
                {
                    break;
                }
                let zone = zone.expect("Compact controls register vertical scrolling");
                let mut state = egui::scroll_area::State::load(&ctx, zone.id).unwrap();
                state.offset.y = (state.offset.y - 25.).max(0.);
                state.store(&ctx, zone.id);
                out = frame(&mut app, &ctx, size, vec![]);
            }
            let all = text_rect(
                &out,
                &ctx,
                &crate::i18n::tr("All frequencies"),
                egui::Order::Middle,
            )
            .unwrap();
            let mut router = crate::touch::TouchRouter::default();
            let mut tap = |pos, time| {
                for (phase, time) in [
                    (egui::TouchPhase::Start, time),
                    (egui::TouchPhase::End, time + 0.02),
                ] {
                    let (events, canceled) = router.event_with_context(&ctx, phase, 1, pos, time);
                    assert!(!canceled);
                    frame(&mut app, &ctx, size, events);
                }
            };
            tap(all.center(), 1.);
            for _ in 0..3 {
                frame(&mut app, &ctx, size, vec![]);
            }
            let out = frame(&mut app, &ctx, size, vec![]);
            let low = text_rect(
                &out,
                &ctx,
                &crate::i18n::tr("Low (<250 Hz)"),
                egui::Order::Foreground,
            )
            .expect("Frequency popup must be visible above its parent window");
            for (phase, time) in [(egui::TouchPhase::Start, 2.), (egui::TouchPhase::End, 2.02)] {
                let (events, _) = router.event_with_context(&ctx, phase, 1, low.center(), time);
                frame(&mut app, &ctx, size, events);
            }
            assert_eq!(app.options.audio_guide.band, 1, "{language:?}/{size:?}");
            app.options.audio_guide.waveform = true;
            app.options.audio_guide.attacks = true;
            app.options.audio_guide.gain = 3.;
            app.options.audio_guide.sensitivity = 0.8;
            if let Some(zone) = zone {
                let mut state = egui::scroll_area::State::load(&ctx, zone.id).unwrap();
                state.offset.y = zone.max_offset;
                state.store(&ctx, zone.id);
            }
            let out = frame(&mut app, &ctx, size, vec![]);
            let reset = text_rect(
                &out,
                &ctx,
                &crate::i18n::tr("Reset audio guide settings"),
                egui::Order::Middle,
            )
            .unwrap();
            for (phase, time) in [(egui::TouchPhase::Start, 3.), (egui::TouchPhase::End, 3.02)] {
                let (events, _) = router.event_with_context(&ctx, phase, 1, reset.center(), time);
                frame(&mut app, &ctx, size, events);
            }
            assert_eq!(
                app.options.audio_guide,
                phistudio_core::AudioGuideOptions::default()
            );
            assert!(app.audio_guide.show);
            assert_eq!(app.editor.document().as_json(), &original);
            assert!(!app.editor.is_dirty());
            app.back();
            assert!(!app.audio_guide.show && !app.close_requested);
        }
    }
}

#[test]
fn attacks_remain_above_grid_strokes_including_the_current_beat() {
    let mut app = Workbench::new(ChartDocument::empty("Marker layer", 120.).unwrap(), None);
    app.options.audio_guide.attacks = true;
    app.audio_guide.key = Some(Key {
        session: app.session,
        assets: app.asset_revision,
        pcm: 0,
        frames: 0,
        sr: 8000,
    });
    app.audio_guide.markers = vec![Onset {
        seconds: 0.,
        strength: 1.,
        band: 2,
    }];
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&mut app, &ctx, vec2(873., 393.), vec![]);
    }
    let out = frame(&mut app, &ctx, vec2(873., 393.), vec![]);
    let grid = crate::grid_input::geometry(app.grid_area, &app);
    let marker_index = out.shapes.iter().position(|shape| matches!(&shape.1,
        egui::Shape::Path(path) if path.points.len() == 4 && path.fill == egui::Color32::from_rgb(100,207,230)
    )).unwrap();
    let mut strokes = 0;
    for (index, shape) in out.shapes.iter().enumerate() {
        if let egui::Shape::LineSegment { points, .. } = &shape.1
            && points.iter().all(|p| (p.y - grid.baseline()).abs() < 0.001)
            && points[0].x <= grid.rect.right()
            && points[1].x >= grid.rect.left()
        {
            assert!(
                index < marker_index,
                "A grid/current-beat stroke covers the marker"
            );
            strokes += 1;
        }
    }
    assert!(
        strokes >= 3,
        "Test must include a beat, current beat and attack tick"
    );
}

#[test]
fn attacks_draw_once_per_surface_regardless_of_overlaid_line_count() {
    let mut raw = ChartDocument::empty("Overlay markers", 120.)
        .unwrap()
        .as_json()
        .clone();
    let line = raw["judgeLineList"][0].clone();
    raw["judgeLineList"] = serde_json::Value::Array(vec![line; 8]);
    let mut app = Workbench::new(ChartDocument::from_value(raw.clone()).unwrap(), None);
    app.options.audio_guide.attacks = true;
    app.audio_guide.key = Some(Key {
        session: app.session,
        assets: app.asset_revision,
        pcm: 0,
        frames: 0,
        sr: 8000,
    });
    app.audio_guide.markers = vec![Onset {
        seconds: 0.5,
        strength: 0.4,
        band: 2,
    }];
    app.multi_line.enabled = true;
    app.multi_line.widths[0] = 150.;
    let ctx = egui::Context::default();
    let size = vec2(1280., 800.);
    let color = egui::Color32::from_rgb(100, 207, 230);
    let tick_color = color.gamma_multiply(0.10 + 0.16 * 0.4);
    for merged in [true, false] {
        app.multi_line.merged = merged;
        for count in [1, 2, 8] {
            app.multi_line.lines = (0..count).collect();
            for _ in 0..3 {
                frame(&mut app, &ctx, size, vec![]);
            }
            let out = frame(&mut app, &ctx, size, vec![]);
            let grid = crate::grid_input::geometry(app.grid_area, &app);
            let expected = if merged {
                1
            } else {
                app.line_panels(grid, false).len()
            };
            let diamonds = out
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.1,
                        egui::Shape::Path(path) if path.points.len() == 4 && path.fill == color
                    )
                })
                .count();
            let ticks = out
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.1,
                        egui::Shape::LineSegment { stroke, .. } if stroke.color == tick_color
                    )
                })
                .count();
            assert_eq!(diamonds, expected, "overlay={merged}, lines={count}");
            assert_eq!(ticks, expected, "overlay={merged}, lines={count}");
        }
    }
    assert_eq!(app.editor.document().as_json(), &raw);
    assert!(!app.editor.is_dirty());
}

#[test]
fn windows_menu_toggles_multi_line_and_audio_guide_in_both_directions() {
    let mut app = Workbench::new(
        ChartDocument::empty("Window visibility", 120.).unwrap(),
        None,
    );
    let raw = app.editor.document().as_json().clone();
    let ctx = egui::Context::default();
    let size = vec2(1280., 800.);
    for label in ["Multiple lines", "Audio guide"] {
        for open in [true, false] {
            for text in ["Windows", label] {
                for _ in 0..3 {
                    frame(&mut app, &ctx, size, vec![]);
                }
                let out = frame(&mut app, &ctx, size, vec![]);
                let rect = out
                    .shapes
                    .iter()
                    .rev()
                    .find_map(|shape| match &shape.1 {
                        egui::Shape::Text(t) if t.galley.job.text == text => {
                            let rect = Rect::from_min_size(t.pos, t.galley.size());
                            (shape.0.contains(rect.center())
                                && (text == "Windows"
                                    || ctx.layer_id_at(rect.center()).is_some_and(|layer| {
                                        layer.order == egui::Order::Foreground
                                    })))
                            .then_some(rect)
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("Visible menu item: {text}, open={open}"));
                for pressed in [true, false] {
                    frame(
                        &mut app,
                        &ctx,
                        size,
                        vec![
                            egui::Event::PointerMoved(rect.center()),
                            egui::Event::PointerButton {
                                pos: rect.center(),
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: Default::default(),
                            },
                        ],
                    );
                }
            }
            assert_eq!(
                if label == "Multiple lines" {
                    app.multi_line.show
                } else {
                    app.audio_guide.show
                },
                open,
                "{label}"
            );
        }
    }
    assert_eq!(app.editor.document().as_json(), &raw);
    assert!(!app.editor.is_dirty());
}
