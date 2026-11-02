//! Noise blocks use Storyboard's interval frame and Events' curve conventions.
use super::*;
use egui::{Align2, FontId, Shape, Stroke};
use phistudio_core::{LineEvent, StoryEvent};
pub(crate) fn paint(app: &Workbench, painter: &egui::Painter, grid: Grid) {
    paint_surface(
        app,
        painter,
        grid,
        View {
            keys: false,
            track: app.noise.track,
        },
    );
    let keys_grid = grid.events();
    if app.noise.selected.is_some() {
        for track in Track::all(app.noise_format()) {
            paint_surface(
                app,
                painter,
                keys_grid,
                View {
                    keys: true,
                    track: *track,
                },
            );
        }
    } else {
        let p = painter.with_clip_rect(grid.event_rect);
        let galley = p.layout(
            crate::i18n::tr("Select field"),
            FontId::proportional(12.),
            muted(),
            grid.event_rect.width() - 20.,
        );
        p.galley(
            grid.event_rect.center_top() + vec2(-galley.size().x / 2., 16.),
            galley,
        );
    }
    if let Some(Gesture::Noise(gesture)) = &app.timeline.drag {
        match &**gesture {
            NoiseGesture::Box {
                origin,
                current,
                grid,
                ..
            } => {
                let p = painter.with_clip_rect(grid.rect);
                let r = Rect::from_two_pos(*origin, *current);
                p.rect_filled(r, 0., accent().gamma_multiply(0.15));
                p.rect_stroke(r, 0., Stroke::new(1_f32, accent()));
            }
            NoiseGesture::Erase { last, grid, .. } => {
                painter.with_clip_rect(grid.rect).circle_stroke(
                    *last,
                    14.,
                    Stroke::new(1_f32, flick()),
                );
            }
            _ => {}
        }
    }
}
pub(super) fn header_label(p: &egui::Painter, track: Track, width: f32) -> String {
    let full = crate::i18n::tr(track.label());
    if p.layout_no_wrap(
        full.clone(),
        FontId::proportional(12.),
        egui::Color32::WHITE,
    )
    .size()
    .x <= width - 4.
    {
        return full;
    }
    let short = match track {
        Track::Move => "XY",
        Track::MoveX => "X",
        Track::MoveY => "Y",
        Track::Scale => "SXY",
        Track::ScaleX => "SX",
        Track::ScaleY => "SY",
        Track::Rotate => "°",
        Track::Color => "RGB",
        Track::Active => "✓",
    };
    let short = crate::i18n::tr(short);
    if p.layout_no_wrap(
        short.clone(),
        FontId::proportional(12.),
        egui::Color32::WHITE,
    )
    .size()
    .x <= width - 4.
    {
        short
    } else {
        let symbol = match track {
            Track::Scale | Track::ScaleX | Track::MoveX => "X",
            Track::ScaleY | Track::MoveY => "Y",
            Track::Move => "+",
            Track::Rotate => "°",
            Track::Color => "C",
            Track::Active => "✓",
        };
        if p.layout_no_wrap(
            symbol.into(),
            FontId::proportional(12.),
            egui::Color32::WHITE,
        )
        .size()
        .x <= width - 4.
        {
            symbol.into()
        } else {
            String::new()
        }
    }
}
fn tint(track: Track) -> egui::Color32 {
    match track {
        Track::Move | Track::MoveX => tap(),
        Track::MoveY | Track::Scale | Track::ScaleX | Track::ScaleY => hold(),
        Track::Rotate => flick(),
        Track::Color => drag(),
        Track::Active => accent(),
    }
}
fn paint_surface(app: &Workbench, painter: &egui::Painter, grid: Grid, view: View) {
    let active = View::focus(app) == view || (!view.keys && !app.noise.animation);
    if !view.keys {
        let p = painter.with_clip_rect(grid.rect);
        for x in layout::grid_lines(grid, app.noise.offset) {
            p.line_segment(
                [pos2(x, grid.rect.top()), pos2(x, grid.rect.bottom())],
                Stroke::new(0.5_f32, border().gamma_multiply(0.55)),
            );
        }
    }
    let w = layout::width(app, grid, view.keys);
    let first = if view.keys {
        0
    } else {
        app.noise.offset.floor() as i64
    };
    let last = first + (grid.rect.width() / w).ceil() as i64 + 1;
    let columns = if view.keys {
        let i = Track::all(app.noise_format())
            .iter()
            .position(|t| *t == view.track)
            .unwrap_or(0) as i64;
        i..i + 1
    } else {
        first..last
    };
    let header = Rect::from_min_max(
        grid.rect.min - vec2(0., 22.),
        pos2(grid.rect.right(), grid.rect.top()),
    );
    for lane in columns {
        let col = layout::column(app, grid, view, lane as f64);
        let p = painter.with_clip_rect(grid.rect);
        p.line_segment(
            [col.left_top(), col.left_bottom()],
            Stroke::new(1_f32, border()),
        );
        p.line_segment(
            [col.right_top(), col.right_bottom()],
            Stroke::new(1_f32, border()),
        );
        let label = if view.keys {
            header_label(painter, view.track, col.width())
        } else {
            format!("X {}", lane + 1)
        };
        let column_header = Rect::from_min_max(
            pos2(col.left(), header.top()),
            pos2(col.right(), header.bottom()),
        );
        let p = painter.with_clip_rect(
            column_header
                .intersect(header)
                .intersect(painter.clip_rect()),
        );
        p.text(
            pos2(col.center().x, grid.rect.top() - 11.),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(if view.keys { 12. } else { 10. }),
            if view.keys { tint(view.track) } else { muted() },
        );
    }
    let mut values: Vec<_> = rows_in(app, view).iter().collect();
    if view.keys && view.track == app.noise.track {
        if let (Some(i), Some(d)) = (app.noise.key_selected, &app.noise.key_draft)
            && let Some(raw) = values.get_mut(i)
        {
            *raw = &d.raw;
        }
    } else if !view.keys
        && let (Some(i), Some(d)) = (app.noise.selected, &app.noise.draft)
        && let Some(raw) = values.get_mut(i)
    {
        *raw = &d.raw;
    }
    if active
        && let Some(Gesture::Noise(gesture)) = &app.timeline.drag
        && let NoiseGesture::Edit(d) = &**gesture
    {
        for (index, _, value) in &d.companions {
            if let Some(raw) = values.get_mut(*index) {
                *raw = value;
            }
        }
        if d.creating {
            values.push(&d.current);
        } else if let Some(raw) = values.get_mut(d.index) {
            *raw = &d.current;
        }
        if let Some(end) = &d.end_key {
            if let Some(index) = end.index {
                if let Some(raw) = values.get_mut(index) {
                    *raw = &end.current;
                }
            } else {
                values.push(&end.current);
            }
        }
    }
    let spans = spans(app, &values, view);
    for (i, raw) in values.iter().enumerate() {
        if active
            && let Some(Gesture::Noise(g)) = &app.timeline.drag
            && let NoiseGesture::Erase { ids, .. } = &**g
            && ids.contains(&i)
        {
            continue;
        }
        let Some(span) = &spans[i] else {
            continue;
        };
        let rect = rect_span(app, grid, raw, i, view, span.range);
        if !rect.intersects(grid.rect) {
            continue;
        }
        let column = if view.keys {
            Track::all(app.noise_format())
                .iter()
                .position(|t| *t == view.track)
                .unwrap_or(0) as f64
        } else {
            layout::lane(raw, i)
        };
        let col = layout::column(app, grid, view, column).intersect(grid.rect);
        if !col.is_positive() {
            continue;
        }
        let p = painter.with_clip_rect(col);
        let selected = if view.keys {
            active && (app.noise.selection.contains(&i) || app.noise.key_selected == Some(i))
        } else {
            app.noise.selected == Some(i)
                || (!app.noise.animation && app.noise.selection.contains(&i))
        };
        let c = if view.keys {
            tint(view.track)
        } else {
            let [r, g, b] = noise::color(raw);
            egui::Color32::from_rgb(r, g, b)
        };
        let (start, end) = if view.keys {
            labels(raw, span.next.map(|n| values[n]), view, app.noise_format())
        } else {
            (
                json!(if noise::name(raw).is_empty() {
                    format!("#{}", i + 1)
                } else {
                    noise::name(raw).to_owned()
                }),
                json!(if raw["isSubtract"] == true || raw["isInvert"] == true {
                    crate::i18n::tr("Subtract")
                } else {
                    String::new()
                }),
            )
        };
        let event = StoryEvent {
            start_time: span.range[0],
            end_time: span.range[1],
            start: if view.keys { json!("") } else { start.clone() },
            end: if view.keys { json!("") } else { end.clone() },
            ..Default::default()
        };
        crate::story_grid::paint_block(&p, grid.rect, rect, &event, selected, c, false);
        if view.keys {
            curves(app, &p, grid, rect, raw, span.next.map(|n| values[n]), view);
            crate::event_grid::paint_interval_labels(
                &p.with_clip_rect(rect.intersect(p.clip_rect())),
                rect,
                &value_label(&start),
                &value_label(&end),
            );
        } else if noise::format(raw).ok() == Some(Format::Native) {
            for phase in [1, 2] {
                if let Ok(beat) = noise::beat(
                    raw,
                    noise::PHASES[phase],
                    Format::Native,
                    app.editor.document().tempo(),
                ) {
                    let y = grid.y(beat.as_f64());
                    p.line_segment(
                        [pos2(rect.left(), y), pos2(rect.right(), y)],
                        Stroke::new(1_f32, c.gamma_multiply(0.8)),
                    );
                    if selected {
                        p.circle_filled(pos2(rect.center().x, y), 3., c);
                    }
                }
            }
        }
    }
}
fn value_label(value: &Value) -> String {
    match value {
        Value::Number(n) => n
            .as_f64()
            .map(crate::event_grid::number_label)
            .unwrap_or_default(),
        Value::Array(values) => format!(
            "[{}]",
            values.iter().map(value_label).collect::<Vec<_>>().join(",")
        ),
        Value::String(s) => s.clone(),
        _ => String::new(),
    }
}
fn native_value(raw: &Value, track: Track) -> Value {
    let value = &raw[match track {
        Track::Move => "endPosition",
        Track::Scale => "scale",
        _ => "rotation",
    }];
    if track == Track::Rotate {
        value.clone()
    } else {
        json!([value["x"], value["y"]])
    }
}
fn labels(raw: &Value, next: Option<&Value>, view: View, format: Format) -> (Value, Value) {
    if view.track.native(format) {
        let a = native_value(raw, view.track);
        (a.clone(), next.map_or(a, |v| native_value(v, view.track)))
    } else if view.track == Track::Active {
        (json!(""), json!(""))
    } else {
        (raw["start"].clone(), raw["end"].clone())
    }
}
fn curves(
    app: &Workbench,
    p: &egui::Painter,
    grid: Grid,
    r: Rect,
    raw: &Value,
    next: Option<&Value>,
    view: View,
) {
    if view.track == Track::Active {
        return;
    }
    let native = view.track.native(app.noise_format());
    let (a, b) = labels(raw, next, view, app.noise_format());
    let values = |value: &Value| -> Vec<f64> {
        if let Some(a) = value.as_array() {
            a.iter().filter_map(Value::as_f64).collect()
        } else {
            value.as_f64().into_iter().collect()
        }
    };
    let (a, b) = (values(&a), values(&b));
    if a.is_empty() || a.len() != b.len() {
        return;
    }
    let mut curve_value = raw.clone();
    curve_value["start"] = json!(0.);
    curve_value["end"] = json!(1.);
    curve_value["bezier"] = json!(u8::from(noise::flag(&raw["bezier"])));
    let curve = serde_json::from_value::<LineEvent>(curve_value).ok();
    let visible = r.intersect(grid.rect);
    let steps = (visible.height() / 4.).ceil().clamp(2., 160.) as usize;
    let lo = a.iter().chain(&b).copied().fold(0., f64::min);
    let hi = a.iter().chain(&b).copied().fold(1., f64::max);
    let start = if native {
        raw["time"].as_f64().unwrap_or(0.)
    } else {
        noise::time(
            raw,
            "startTime",
            Format::Next,
            app.editor.document().tempo(),
        )
        .unwrap_or(0.)
    };
    let end = if native {
        next.and_then(|v| v["time"].as_f64()).unwrap_or(start)
    } else {
        noise::time(raw, "endTime", Format::Next, app.editor.document().tempo()).unwrap_or(start)
    };
    for channel in 0..a.len() {
        let points = (0..=steps)
            .filter_map(|i| {
                let y = visible.bottom() - visible.height() * i as f32 / steps as f32;
                let time = app.editor.document().tempo().seconds_at(grid.raw_beat(y));
                let t = if end > start {
                    ((time - start) / (end - start)).clamp(0., 1.)
                } else {
                    1.
                };
                let progress = if native {
                    let key = if view.track == Track::Rotate {
                        "easeType"
                    } else if channel == 0 {
                        "easeTypeX"
                    } else {
                        "easeTypeY"
                    };
                    noise::native::ease(raw[key].as_i64().unwrap_or(0) as i32, t as f32) as f64
                } else {
                    curve.as_ref().map_or(t, |event| noise::curve(event, t))
                };
                let value = a[channel] + (b[channel] - a[channel]) * progress;
                value.is_finite().then(|| {
                    pos2(
                        r.left()
                            + (0.1 + ((value - lo) / (hi - lo)).clamp(0., 1.) as f32 * 0.8)
                                * r.width(),
                        y,
                    )
                })
            })
            .collect();
        p.add(Shape::line(
            points,
            Stroke::new(
                1.5_f32,
                if channel == 1 {
                    hold()
                } else if channel == 2 {
                    flick()
                } else {
                    tint(view.track)
                },
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn translated_track_headers_fit_fixed_tracks_on_narrow_screens() {
        for language in phistudio_core::Language::ALL {
            let _language = crate::i18n::Scope::enter(language);
            let ctx = egui::Context::default();
            ctx.set_fonts(crate::fonts::definitions());
            let _ = ctx.run(Default::default(), |ctx| {
                for format in [Format::Native, Format::Next] {
                    for width in [16., 22., 32., 48., 64., 90.] {
                        for track in Track::all(format) {
                            let p = ctx.debug_painter();
                            let label = header_label(&p, *track, width);
                            assert!(
                                p.layout_no_wrap(
                                    label,
                                    FontId::proportional(12.),
                                    egui::Color32::WHITE
                                )
                                .size()
                                .x <= width - 4.
                            );
                        }
                    }
                }
            });
        }
    }
}
