use super::*;

pub(crate) fn paint(app: &Workbench, painter: &egui::Painter, grid: Grid) {
    let p = painter.with_clip_rect(grid.rect);
    if let Some(e) = &app.shaders.error {
        p.text(
            grid.rect.left_top() + vec2(6., 6.),
            Align2::LEFT_TOP,
            crate::i18n::tr(e),
            FontId::proportional(12.),
            flick(),
        );
        return;
    }
    let lanes = lanes(app);
    let keyframe = app.shaders.animation_track().is_some();
    let count = if keyframe { 1 } else { lanes.len() };
    for (i, lane) in lanes.iter().enumerate().take(count) {
        let col = column(grid, i, count);
        painter.line_segment(
            [col.left_top(), col.left_bottom()],
            Stroke::new(1_f32, border()),
        );
        let label = if let Some(name) = app.shaders.animation_track() {
            name.to_owned()
        } else {
            format!(
                "{} · {}",
                if lane.1 { "G" } else { "L" },
                lane.0.trim_start_matches('/')
            )
        };
        let header = Rect::from_min_max(
            pos2(col.left(), col.top() - 20.),
            pos2(col.right(), col.top()),
        );
        painter
            .with_clip_rect(header.intersect(painter.clip_rect()))
            .text(
                pos2(col.center().x, col.top() - 10.),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(if col.width() < 80. { 9. } else { 11. }),
                accent(),
            );
    }
    let mut rows = rows(app);
    if rows.is_empty() {
        let hint = if keyframe {
            "Use + Keyframe to draw an interval."
        } else {
            "Select + Shader, choose a preset, then draw upwards."
        };
        p.text(
            grid.rect.center_top() + vec2(0., 22.),
            Align2::CENTER_TOP,
            crate::i18n::tr(hint),
            FontId::proportional(11.),
            muted(),
        );
    }
    if let Some(a) = &app.shaders.animation
        && keyframe
        && let Some(row) = rows.get_mut(a.index)
    {
        *row = a.raw.clone();
    }
    if let Some(Gesture::Shader(d)) = &app.drag
        && let ShaderGesture::Edit(d) = &**d
    {
        let index = d.keyframe.as_ref().map_or(d.effect, |(_, index)| *index);
        if d.creating {
            rows.push(d.current.clone());
        } else if let Some(row) = rows.get_mut(index) {
            *row = d.current.clone();
        }
    }
    for (index, raw) in rows.iter().enumerate() {
        if let Some(Gesture::Shader(d)) = &app.drag
            && let ShaderGesture::Erase { ids, .. } = &**d
            && ids.contains(&index)
        {
            continue;
        }
        let col = if keyframe {
            grid.rect
        } else {
            let Some(lane) = lanes.iter().position(|(name, global)| {
                raw["shader"].as_str() == Some(name)
                    && raw["global"].as_bool().unwrap_or(false) == *global
            }) else {
                continue;
            };
            column(grid, lane, lanes.len())
        };
        let Some(r) = interval(app, grid, col, raw, keyframe) else {
            continue;
        };
        let selected = if keyframe {
            app.shaders
                .animation
                .as_ref()
                .is_some_and(|a| a.index == index)
        } else {
            app.shaders.selected == Some(index)
        };
        let mut event = StoryEvent::new(StoryKind::ScaleX, Beat::ZERO, Beat::ONE);
        if keyframe {
            event.start = raw["start"].clone();
            event.end = raw["end"].clone();
            event.easing_type = raw["easingType"].as_u64().unwrap_or(1) as i32;
            event.easing_left = raw["easingLeft"].as_f64().unwrap_or(0.);
            event.easing_right = raw["easingRight"].as_f64().unwrap_or(1.);
        } else {
            event.start = json!(format!("#{}", index + 1));
            event.end = event.start.clone();
        }
        crate::story_grid::paint_block(&p, grid.rect, r, &event, selected, accent(), keyframe);
    }
    if let Some(Gesture::Shader(d)) = &app.drag
        && let ShaderGesture::Erase { last, .. } = &**d
    {
        p.circle_stroke(*last, 14., Stroke::new(1_f32, flick()));
    }
}
