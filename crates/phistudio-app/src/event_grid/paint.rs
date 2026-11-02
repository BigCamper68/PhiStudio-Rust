use super::*;

pub(super) fn header_label(painter: &egui::Painter, kind: EventType, width: f32) -> String {
    let label = if width < 65. {
        match kind {
            EventType::MoveX => "X",
            EventType::MoveY => "Y",
            EventType::Rotate if width < 44. => "Rot",
            EventType::Alpha if width < 44. => "α",
            EventType::Speed if width < 44. => "Spd",
            _ => kind.label(),
        }
    } else {
        kind.label()
    };
    let text = crate::i18n::tr(label);
    if painter
        .layout_no_wrap(text.clone(), FontId::proportional(12.), Color32::WHITE)
        .size()
        .x
        <= width - 4.
    {
        text
    } else {
        match kind {
            EventType::MoveX => "X",
            EventType::MoveY => "Y",
            EventType::Rotate => "°",
            EventType::Alpha => "α",
            EventType::Speed => "v",
        }
        .into()
    }
}

pub fn paint(app: &mut Workbench, painter: &egui::Painter, grid: Grid) {
    let clipped = painter.with_clip_rect(grid.rect.intersect(painter.clip_rect()));
    let separator = if app.multi_active(true) {
        Stroke::new(1.5_f32, accent().linear_multiply(0.6))
    } else {
        Stroke::new(1_f32, border())
    };
    for (i, kind) in EventType::ALL.into_iter().enumerate() {
        let col = column(grid, kind);
        painter.line_segment(
            [col.left_top(), col.left_bottom()],
            if i == 0 {
                separator
            } else {
                Stroke::new(1_f32, border())
            },
        );
        painter.text(
            pos2(col.center().x, grid.rect.top() - 11.),
            Align2::CENTER_CENTER,
            header_label(painter, kind, col.width()),
            FontId::proportional(12.),
            color(kind),
        );
        if i == 4 {
            painter.line_segment([col.right_top(), col.right_bottom()], separator);
        }
    }
    for (id, event) in app.event_rows().iter() {
        if let Some(Gesture::Erase { selection, .. }) = &app.drag
            && selection.events.binary_search(id).is_ok()
        {
            continue;
        }
        let mut group_preview = match &app.drag {
            Some(Gesture::SelectionMove {
                selection, beats, ..
            }) if selection.events.binary_search(id).is_ok() => {
                let mut event = event.clone();
                if let (Ok(start), Ok(end)) = (
                    event.start_time.checked_add(*beats),
                    event.end_time.checked_add(*beats),
                ) {
                    event.start_time = start;
                    event.end_time = end;
                }
                Some(event)
            }
            _ => None,
        };
        if app.options.xy_link
            && let Some(Gesture::Event(EventGesture::Edit {
                id: drag_id,
                current,
                ..
            })) = &app.drag
            && *drag_id != *id
            && app.editor.document().xy_pair(*drag_id).ok() == Some(*id)
        {
            let mut paired = event.clone();
            paired.start_time = current.start_time;
            paired.end_time = current.end_time;
            group_preview = Some(paired);
        }
        let visual = match &app.drag {
            Some(Gesture::Event(EventGesture::Edit {
                id: drag_id,
                current,
                ..
            })) if *drag_id == *id => current,
            _ => group_preview.as_ref().unwrap_or(event),
        };
        draw(
            &clipped,
            grid,
            *id,
            visual,
            app.events.selected == Some(*id)
                || app.tools.selection.events.binary_search(id).is_ok(),
            false,
        );
    }
    if let Some(Gesture::Event(EventGesture::Create {
        line, kind, event, ..
    })) = &app.drag
        && *line == app.line
    {
        draw(
            &clipped,
            grid,
            EventId {
                line: app.line,
                layer: app.events.layer,
                kind: *kind,
                index: usize::MAX,
            },
            event,
            true,
            true,
        );
    }
}

pub(super) fn color(kind: EventType) -> Color32 {
    match kind {
        EventType::MoveX => tap(),
        EventType::MoveY => hold(),
        EventType::Rotate => flick(),
        EventType::Alpha => drag(),
        EventType::Speed => accent(),
    }
}

pub(super) fn draw(
    p: &egui::Painter,
    grid: Grid,
    id: EventId,
    event: &LineEvent,
    selected: bool,
    ghost: bool,
) {
    let col = column(grid, id.kind).shrink2(vec2(4., 0.));
    let start = grid.y(event.start_time.as_f64());
    let end = grid.y(event.end_time.as_f64());
    let rect = Rect::from_min_max(pos2(col.left(), end), pos2(col.right(), start));
    if !rect.intersects(grid.rect) {
        return;
    }
    let clipped = p.with_clip_rect(col.intersect(p.clip_rect()));
    let p = &clipped;
    let c = color(id.kind);
    p.rect_filled(rect, 2., c.gamma_multiply(if ghost { 0.1 } else { 0.17 }));
    p.rect_stroke(
        rect,
        2.,
        Stroke::new(
            if selected { 1.8_f32 } else { 0.8_f32 },
            if selected {
                accent()
            } else {
                c.gamma_multiply(0.7)
            },
        ),
    );
    let curve = phistudio_prpr::EventCurve::new(id.kind, event);
    let (lo, hi) = id.kind.range();
    // Bound work by visible pixels, even for events lasting thousands of beats.
    let visible = rect.intersect(grid.rect);
    let samples = (visible.height() / 4.).ceil().clamp(2., 160.) as usize;
    let points: Vec<_> = (0..=samples)
        .filter_map(|i| {
            let y = visible.bottom() - visible.height() * i as f32 / samples as f32;
            let t = ((start - y) / (start - end)).clamp(0., 1.);
            let v = event.start + (event.end - event.start) * curve.sample(t) as f64;
            v.is_finite().then(|| {
                pos2(
                    col.left() + ((v - lo) / (hi - lo)).clamp(0., 1.) as f32 * col.width(),
                    y,
                )
            })
        })
        .collect();
    p.add(Shape::line(points, Stroke::new(1.5_f32, c)));
    paint_interval_labels(
        p,
        rect,
        &number_label(event.start),
        &number_label(event.end),
    );
    if selected {
        for y in [start, end] {
            p.circle_filled(pos2(col.center().x, y), 4.5, accent());
        }
    }
}

/// Shared endpoint captions stay legible in dense animation timelines.
pub(crate) fn number_label(number: f64) -> String {
    if number.fract() == 0. {
        format!("{number:.0}")
    } else {
        format!("{number:.2}")
    }
}
pub(crate) fn paint_interval_labels(p: &egui::Painter, rect: Rect, start: &str, end: &str) {
    if start.is_empty() && end.is_empty() {
        return;
    }
    // Labels retain endpoint anchors while the caller supplies the viewport scissor.
    if rect.width() < 65. && rect.height() > 44. {
        p.text(
            rect.left_top() + vec2(4., 4.),
            Align2::LEFT_TOP,
            end,
            FontId::monospace(10.),
            text(),
        );
        p.text(
            rect.left_bottom() + vec2(4., -4.),
            Align2::LEFT_BOTTOM,
            start,
            FontId::monospace(10.),
            text(),
        );
    } else if rect.height() > 28. {
        p.text(
            rect.left_top() + vec2(4., 4.),
            Align2::LEFT_TOP,
            format!("{start} → {end}"),
            FontId::monospace(10.),
            text(),
        );
    }
}
