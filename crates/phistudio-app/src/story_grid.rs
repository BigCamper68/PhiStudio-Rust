//! Extended RPE events share the ordinary timeline, with native easing curves.
use crate::{
    advanced_ui::{RightMode, StoryId},
    grid_input::{Gesture, Grid, Intent},
    theme::*,
    workbench::{Tool, Workbench},
};
use egui::{Align2, Color32, FontId, Pos2, Rect, Shape, Stroke, pos2, vec2};
use phistudio_core::{Beat, EventType, LineEvent, StoryEvent, StoryKind};

pub struct StoryGesture {
    pub id: Option<StoryId>,
    pub kind: StoryKind,
    pub original: StoryEvent,
    pub event: StoryEvent,
    pub origin: Pos2,
    pub grid: Grid,
    pub handle: Option<bool>,
    pub moved: bool,
    pub group: Vec<StoryId>,
    pub normal: phistudio_core::tools::Selection,
    pub earliest: phistudio_core::Beat,
}
fn column(grid: Grid, kind: StoryKind) -> Rect {
    let index = StoryKind::EDITABLE.iter().position(|k| *k == kind).unwrap();
    let w = grid.rect.width() / StoryKind::EDITABLE.len() as f32;
    Rect::from_min_max(
        pos2(grid.rect.left() + index as f32 * w, grid.rect.top()),
        pos2(
            grid.rect.left() + (index + 1) as f32 * w,
            grid.rect.bottom(),
        ),
    )
}
fn rect(grid: Grid, kind: StoryKind, event: &StoryEvent) -> Rect {
    let col = column(grid, kind).shrink2(vec2(3., 0.));
    Rect::from_min_max(
        pos2(col.left(), grid.y(event.end_time.as_f64())),
        pos2(col.right(), grid.y(event.start_time.as_f64())),
    )
}
fn kind_at(grid: Grid, pos: Pos2) -> StoryKind {
    StoryKind::EDITABLE
        [(((pos.x - grid.rect.left()) / grid.rect.width() * 6.).floor() as usize).min(5)]
}
pub fn begin(app: &mut Workbench, grid: Grid, pos: Pos2, additive: bool) -> Gesture {
    let kind = kind_at(grid, pos);
    let found = app
        .editor
        .document()
        .story_events(app.line, kind)
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .rev()
        .filter(|(_, e)| {
            pos.y >= grid.y(e.end_time.as_f64()) - 8. && pos.y <= grid.y(e.start_time.as_f64()) + 8.
        })
        .collect::<Vec<_>>();
    let found = if !additive && app.story_selection().len() == 1 && app.selection().is_empty() {
        found
            .iter()
            .find(|(index, _)| {
                app.advanced
                    .story_selected
                    .is_some_and(|id| id.line == app.line && id.kind == kind && id.index == *index)
            })
            .cloned()
    } else {
        None
    }
    .or_else(|| found.first().cloned());
    if app.tool == Tool::Event {
        let start = grid.beat(pos.y);
        let end = start
            .checked_add(Beat::new(0, 1, grid.division).unwrap())
            .unwrap_or(start);
        let event = StoryEvent::new(kind, start, end);
        return Gesture::Story(Box::new(StoryGesture {
            id: None,
            kind,
            original: event.clone(),
            event,
            origin: pos,
            grid,
            handle: Some(false),
            moved: false,
            group: vec![],
            normal: Default::default(),
            earliest: start,
        }));
    }
    if let Some((index, event)) = found {
        let id = StoryId {
            line: app.line,
            kind,
            index,
        };
        let selected = app.story_selection();
        let normal = app.selection();
        if !additive && selected.contains(&id) && matches!(app.tool, Tool::Select | Tool::BoxSelect)
        {
            let a = (pos.y - grid.y(event.start_time.as_f64())).abs();
            let b = (pos.y - grid.y(event.end_time.as_f64())).abs();
            let earliest = phistudio_core::objects::ObjectClipboard::copy(
                app.editor.document(),
                &normal,
                &selected,
            )
            .ok()
            .map(|c| c.first)
            .unwrap_or(event.start_time);
            return Gesture::Story(Box::new(StoryGesture {
                id: Some(id),
                kind,
                original: event.clone(),
                event,
                origin: pos,
                grid,
                handle: (selected.len() == 1
                    && normal.is_empty()
                    && app.tool == Tool::Select
                    && a.min(b) <= 14.)
                    .then_some(a <= b),
                moved: false,
                group: selected,
                normal,
                earliest,
            }));
        }
        return Gesture::Tap {
            origin: pos,
            grid,
            intent: Intent::SelectStory(Some(id), additive),
            moved: false,
        };
    }
    Gesture::Tap {
        origin: pos,
        grid,
        intent: Intent::SelectStory(None, additive),
        moved: false,
    }
}
pub fn box_hits(app: &Workbench, grid: Grid, area: Rect) -> Vec<StoryId> {
    let mut ids = vec![];
    for kind in StoryKind::EDITABLE {
        for (index, event) in app
            .editor
            .document()
            .story_events(app.line, kind)
            .unwrap_or_default()
            .into_iter()
            .enumerate()
        {
            if area.intersects(rect(grid, kind, &event).intersect(grid.rect)) {
                ids.push(StoryId {
                    line: app.line,
                    kind,
                    index,
                });
            }
        }
    }
    ids
}
pub fn hits(app: &Workbench, grid: Grid, pos: Pos2) -> Vec<StoryId> {
    if !grid.rect.contains(pos) {
        return vec![];
    }
    let kind = kind_at(grid, pos);
    app.editor
        .document()
        .story_events(app.line, kind)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            pos.y >= grid.y(e.end_time.as_f64()) - 8. && pos.y <= grid.y(e.start_time.as_f64()) + 8.
        })
        .map(|(index, _)| StoryId {
            line: app.line,
            kind,
            index,
        })
        .collect()
}
pub fn update(d: &mut StoryGesture, pos: Pos2) {
    d.moved |= pos.distance(d.origin) > 8.;
    if d.id.is_none() || d.moved {
        let mut interval = crate::grid_input::IntervalDrag {
            original: [d.original.start_time, d.original.end_time],
            current: [d.event.start_time, d.event.end_time],
            origin: d.grid.beat(d.origin.y),
            earliest: d.earliest,
            handle: d.handle,
        };
        interval.update(d.grid.beat(pos.y), d.grid.division);
        [d.event.start_time, d.event.end_time] = interval.current;
    }
}
pub fn finish(app: &mut Workbench, d: StoryGesture) {
    if (d.group.len() > 1 || !d.normal.is_empty()) && d.moved {
        let edit = d
            .event
            .start_time
            .checked_sub(d.original.start_time)
            .and_then(|beats| {
                phistudio_core::objects::transform(
                    app.editor.document(),
                    &d.normal,
                    &d.group,
                    phistudio_core::tools::Transform::Shift { beats, x: 0. },
                )
            });
        app.apply_object_tool(edit);
        return;
    }
    if d.id.is_none() || d.moved {
        app.apply_story(d.id, d.kind, Some(d.event));
    }
}
fn tint(kind: StoryKind) -> Color32 {
    match kind {
        StoryKind::ScaleX => tap(),
        StoryKind::ScaleY => accent(),
        StoryKind::Color => flick(),
        StoryKind::Text => hold(),
        StoryKind::Paint => drag(),
        StoryKind::Gif => Color32::from_rgb(185, 154, 243),
        StoryKind::Incline => muted(),
    }
}
fn header_label(painter: &egui::Painter, kind: StoryKind, width: f32) -> (String, FontId) {
    let (full, short, symbol) = match kind {
        StoryKind::ScaleX => ("Scale X", "SX", "X"),
        StoryKind::ScaleY => ("Scale Y", "SY", "Y"),
        StoryKind::Color => ("RGB", "RGB", "C"),
        StoryKind::Text => ("Text", "Text", "T"),
        StoryKind::Paint => ("Paint", "Paint", "P"),
        StoryKind::Gif => ("GIF", "GIF", "G"),
        StoryKind::Incline => ("Incline", "Inc", "I"),
    };
    let font = FontId::proportional(if width < 44. { 9. } else { 11. });
    for label in [if width < 65. { short } else { full }, short, symbol] {
        let text = crate::i18n::tr(label);
        if painter
            .layout_no_wrap(text.clone(), font.clone(), Color32::WHITE)
            .size()
            .x
            <= width - 4.
        {
            return (text, font);
        }
    }
    (String::new(), font)
}
pub fn paint(app: &Workbench, painter: &egui::Painter, grid: Grid) {
    let p = painter.with_clip_rect(grid.rect);
    let selected: std::collections::BTreeSet<_> = app.story_selection().into_iter().collect();
    let (moving, shift): (std::collections::BTreeSet<_>, _) = match &app.drag {
        Some(Gesture::Story(d)) if d.moved => (
            d.group.iter().copied().collect(),
            d.event.start_time.checked_sub(d.original.start_time).ok(),
        ),
        Some(Gesture::SelectionMove {
            stories,
            beats,
            moved: true,
            ..
        }) => (stories.iter().copied().collect(), Some(*beats)),
        _ => (Default::default(), None),
    };
    for kind in StoryKind::EDITABLE.iter().copied() {
        let col = column(grid, kind);
        painter.line_segment(
            [col.left_top(), col.left_bottom()],
            Stroke::new(1_f32, border()),
        );
        let (label, font) = header_label(painter, kind, col.width());
        let header = Rect::from_min_max(
            pos2(col.left() + 1., col.top() - 20.),
            pos2(col.right() - 1., col.top()),
        );
        painter
            .with_clip_rect(header.intersect(painter.clip_rect()))
            .text(
                pos2(col.center().x, col.top() - 10.),
                Align2::CENTER_CENTER,
                label,
                font,
                tint(kind),
            );
        for (index, mut event) in app
            .editor
            .document()
            .story_events(app.line, kind)
            .unwrap_or_default()
            .into_iter()
            .enumerate()
        {
            let id = StoryId {
                line: app.line,
                kind,
                index,
            };
            if let Some(Gesture::Erase { stories: ids, .. }) = &app.drag
                && ids.contains(&id)
            {
                continue;
            }
            if let Some(Gesture::Story(d)) = &app.drag
                && d.id == Some(id)
            {
                event = d.event.clone();
            } else if moving.contains(&id)
                && let Some(delta) = shift
            {
                event.start_time = event
                    .start_time
                    .checked_add(delta)
                    .unwrap_or(event.start_time);
                event.end_time = event.end_time.checked_add(delta).unwrap_or(event.end_time);
            }
            block(&p, grid, kind, &event, selected.contains(&id));
        }
    }
    if let Some(Gesture::Story(d)) = &app.drag
        && d.id.is_none()
    {
        block(&p, grid, d.kind, &d.event, true);
    }
}
fn text(v: &serde_json::Value) -> String {
    v.as_str()
        .map_or_else(|| v.to_string(), str::to_owned)
        .chars()
        .take(40)
        .collect()
}
fn block(p: &egui::Painter, grid: Grid, kind: StoryKind, event: &StoryEvent, selected: bool) {
    let r = rect(grid, kind, event);
    paint_block(p, grid.rect, r, event, selected, tint(kind), true);
}
pub(crate) fn paint_block(
    p: &egui::Painter,
    clip: Rect,
    r: Rect,
    event: &StoryEvent,
    selected: bool,
    color: Color32,
    show_curve: bool,
) {
    if !r.intersects(clip) {
        return;
    }
    p.rect_filled(
        r,
        0.,
        color.gamma_multiply(if selected { 0.24 } else { 0.13 }),
    );
    p.rect_stroke(
        r,
        0.,
        Stroke::new(
            if selected { 1.5_f32 } else { 0.7_f32 },
            color.gamma_multiply(0.8),
        ),
    );
    if show_curve {
        let curve = phistudio_prpr::EventCurve::new(
            EventType::MoveX,
            &LineEvent {
                easing_type: event.easing_type,
                easing_left: event.easing_left,
                easing_right: event.easing_right,
                bezier: event.bezier,
                bezier_points: event.bezier_points,
                ..Default::default()
            },
        );
        let points = (0..=48)
            .map(|i| {
                let t = i as f32 / 48.;
                let x = if event.start == event.end {
                    0.5
                } else {
                    curve.sample(t).clamp(-0.1, 1.1)
                };
                pos2(
                    r.left() + r.width() * (0.1 + x * 0.8),
                    r.bottom() - t * r.height(),
                )
            })
            .collect();
        p.add(Shape::line(points, Stroke::new(1_f32, color)));
    }
    let q = p.with_clip_rect(r.intersect(clip));
    for (y, align, label) in [
        (r.bottom() - 3., Align2::LEFT_BOTTOM, text(&event.start)),
        (r.top() + 3., Align2::LEFT_TOP, text(&event.end)),
    ] {
        q.text(
            pos2(r.left() + 3., y),
            align,
            label,
            FontId::monospace(10.),
            crate::theme::text(),
        );
    }
    if selected {
        for y in [r.top(), r.bottom()] {
            p.circle_filled(pos2(r.center().x, y), 3., color);
        }
    }
}
pub fn active(app: &Workbench) -> bool {
    app.advanced.right_mode == RightMode::Storyboard
}

#[cfg(test)]
mod header_tests {
    use super::*;
    #[test]
    fn narrow_storyboard_headers_fit_in_every_language() {
        for language in phistudio_core::Language::ALL {
            let _language = crate::i18n::Scope::enter(language);
            let ctx = egui::Context::default();
            ctx.set_fonts(crate::fonts::definitions());
            let _ = ctx.run(Default::default(), |ctx| {
                let p = ctx.debug_painter();
                for width in [16., 24., 42., 60., 90., 140.] {
                    for kind in StoryKind::EDITABLE {
                        let (text, font) = header_label(&p, kind, width);
                        assert!(
                            p.layout_no_wrap(text, font, Color32::WHITE).size().x <= width - 4.
                        );
                    }
                }
            });
        }
    }
}
