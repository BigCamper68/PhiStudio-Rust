use super::*;

pub(crate) fn paint(app: &Workbench, painter: &egui::Painter, grid: Grid) {
    for keys in [false, true] {
        let surface = if keys {
            grid.events()
        } else {
            Grid {
                event_rect: grid.rect,
                ..grid
            }
        };
        let p = painter.with_clip_rect(surface.rect);
        if keys {
            for (index, parameter) in app.shaders.parameters.iter().enumerate() {
                let col = layout::column(app, surface, true, index as f64);
                heading(painter, surface, col, &parameter.name);
            }
            for parameter in &app.shaders.parameters {
                paint_rows(app, &p, surface, Some(&parameter.name));
            }
        } else {
            let step = f64::from(
                surface.rect.width() / (surface.lanes.max(2) - 1) as f32 / layout::EFFECT_WIDTH,
            );
            let offset = app.shaders.offsets[0];
            let first = ((offset - 0.5) / step).floor();
            if app.options.minor_grid {
                for n in 0..surface.lanes + 2 {
                    let x = surface.rect.left()
                        + (((first + n as f64) * step + 0.5 - offset)
                            * f64::from(layout::EFFECT_WIDTH)) as f32;
                    p.line_segment(
                        [pos2(x, surface.rect.top()), pos2(x, surface.rect.bottom())],
                        Stroke::new(0.5_f32, raised()),
                    );
                }
            }
            let first = offset.floor();
            let count = (surface.rect.width() / layout::EFFECT_WIDTH).ceil() as usize + 2;
            for n in 0..count {
                let i = first + n as f64;
                let col = layout::column(app, surface, false, i);
                heading(painter, surface, col, &format!("X {:.0}", i + 1.));
            }
            paint_rows(app, &p, surface, None);
            if let Some(error) = &app.shaders.error {
                p.text(
                    surface.rect.left_top() + vec2(6., 6.),
                    Align2::LEFT_TOP,
                    crate::i18n::tr(error),
                    FontId::proportional(12.),
                    flick(),
                );
            }
        }
    }
}
fn heading(p: &egui::Painter, grid: Grid, col: Rect, label: &str) {
    let q = p.with_clip_rect(grid.rect.intersect(p.clip_rect()));
    q.line_segment(
        [col.center_top(), col.center_bottom()],
        Stroke::new(1_f32, border()),
    );
    let header = Rect::from_min_max(pos2(col.left(), col.top() - 20.), col.right_top());
    let clip = Rect::from_min_max(pos2(grid.rect.left(), header.top()), grid.rect.right_top());
    p.with_clip_rect(header.intersect(clip).intersect(p.clip_rect()))
        .text(
            pos2(col.center().x, col.top() - 10.),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(11.),
            accent(),
        );
}
fn paint_rows(app: &Workbench, p: &egui::Painter, grid: Grid, name: Option<&str>) {
    let keys = name.is_some();
    if let Some(name) = name
        && layout::parameter_column(app, grid, name).is_none_or(|col| !col.intersects(grid.rect))
    {
        return;
    }
    for (index, row) in view::Rows::new(app, name).iter() {
        let raw = row.as_ref();
        if let Some(Gesture::Shader(d)) = &app.timeline.drag
            && let ShaderGesture::Erase {
                ids,
                keys: erase_keys,
                ..
            } = &**d
            && *erase_keys == keys
            && ids.contains(&Item::new(name, index))
        {
            continue;
        }
        let Some(col) = row_column(app, grid, name, index, raw) else {
            continue;
        };
        let Some(r) = interval(app, grid, col, raw, keys) else {
            continue;
        };
        if !r.intersects(grid.rect) {
            continue;
        }
        let selected = app.shaders.selection.contains(&Item::new(name, index))
            || if keys {
                app.shaders
                    .animation
                    .as_ref()
                    .is_some_and(|a| Some(a.name.as_str()) == name && a.index == index)
            } else {
                app.shaders.selected == Some(index)
            };
        let mut event = StoryEvent::new(StoryKind::ScaleX, Beat::ZERO, Beat::ONE);
        if keys {
            event.start = raw["start"].clone();
            event.end = raw["end"].clone();
            event.easing_type = raw["easingType"].as_i64().unwrap_or(1) as i32;
            event.easing_left = raw["easingLeft"].as_f64().unwrap_or(0.);
            event.easing_right = raw["easingRight"].as_f64().unwrap_or(1.);
        }
        crate::story_grid::paint_block_shape(p, grid.rect, r, &event, selected, accent(), keys);
        let q = p.with_clip_rect(r.intersect(grid.rect));
        if keys {
            crate::event_grid::paint_interval_labels(
                &q,
                r,
                &raw["start"].to_string(),
                &raw["end"].to_string(),
            );
        } else if r.height() > 12. {
            q.text(
                r.left_bottom() + vec2(3., -3.),
                Align2::LEFT_BOTTOM,
                format!("#{}", index + 1),
                FontId::monospace(10.),
                text(),
            );
        }
    }
    if let Some(Gesture::Shader(d)) = &app.timeline.drag
        && let ShaderGesture::Box {
            origin,
            current,
            keys: box_keys,
            ..
        } = &**d
        && *box_keys == keys
    {
        p.rect_stroke(
            Rect::from_two_pos(*origin, *current),
            0.,
            Stroke::new(1_f32, accent()),
        );
    }
    if let Some(Gesture::Shader(d)) = &app.timeline.drag
        && let ShaderGesture::Erase { last, .. } = &**d
        && app.shaders.animation_track() == name
    {
        p.circle_stroke(*last, 14., Stroke::new(1_f32, flick()));
    }
}
