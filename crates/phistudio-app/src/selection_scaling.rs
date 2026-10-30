//! Direct X scaling shares the grid's touch router and commits only on release.
use crate::{
    grid_input::{Gesture, Grid},
    theme::*,
    workbench::{Tool, Workbench},
};
use egui::{Pos2, Rect, pos2, vec2};
use phistudio_core::tools::Selection;
#[derive(Clone, Copy, Default, serde::Serialize, serde::Deserialize, PartialEq, Debug)]
pub(crate) enum Anchor {
    First,
    #[default]
    Center,
    Last,
}
pub(crate) struct Frame {
    pub bounds: Rect,
    pub handles: Vec<Pos2>,
    pub anchor: f64,
    pub selection: Selection,
}
pub(crate) fn frame(app: &mut Workbench, grid: Grid) -> Option<Frame> {
    if app.tool != Tool::Select || app.advanced.right_mode == crate::advanced_ui::RightMode::Shaders
    {
        return None;
    }
    let selection = app.selection();
    if selection.notes.len() < 2
        || (!app.multi_active(false) && selection.notes.iter().any(|n| n.line != app.line))
    {
        return None;
    }
    let selected: Vec<_> = selection
        .notes
        .iter()
        .filter_map(|id| {
            app.editor
                .document()
                .note(id.line, id.index)
                .ok()
                .map(|note| (id.index, note))
        })
        .collect();
    if selected.len() < 2 {
        return None;
    }
    let lo = selected
        .iter()
        .map(|(_, n)| n.x)
        .fold(f64::INFINITY, f64::min);
    let hi = selected
        .iter()
        .map(|(_, n)| n.x)
        .fold(f64::NEG_INFINITY, f64::max);
    if hi - lo < 1e-5 {
        return None;
    }
    let first = selected.iter().min_by_key(|(i, n)| (n.start_time, *i))?.1.x;
    let last = selected.iter().max_by_key(|(i, n)| (n.start_time, *i))?.1.x;
    let anchor = match app.tools.scale_anchor {
        Anchor::First => first,
        Anchor::Center => (lo + hi) / 2.,
        Anchor::Last => last,
    };
    let scale = if let Some(Gesture::SelectionScale { anchor, factor, .. }) = &app.drag {
        Some((*anchor, *factor))
    } else {
        None
    };
    let (a, b) = scale.map_or((lo, hi), |(anchor, factor)| {
        (
            anchor + (lo - anchor) * factor,
            anchor + (hi - anchor) * factor,
        )
    });
    let top = selected
        .iter()
        .map(|(_, n)| grid.y(n.end_time.as_f64().max(n.start_time.as_f64())))
        .fold(f32::INFINITY, f32::min);
    let bottom = selected
        .iter()
        .map(|(_, n)| grid.y(n.start_time.as_f64()))
        .fold(f32::NEG_INFINITY, f32::max);
    let bounds = Rect::from_min_max(pos2(grid.x(a.min(b)), top), pos2(grid.x(a.max(b)), bottom))
        .expand2(vec2(0., 8.));
    if !bounds.intersects(grid.rect) {
        return None;
    }
    let y = (top - 24.).clamp(grid.rect.top() + 14., grid.rect.bottom() - 14.);
    let handles = [a, b]
        .into_iter()
        .filter(|v| (*v - anchor).abs() > 1e-5)
        .map(|v| pos2(grid.x(v), y))
        .filter(|p| grid.rect.contains(*p))
        .collect();
    Some(Frame {
        bounds,
        handles,
        anchor,
        selection,
    })
}
pub(crate) fn begin(app: &mut Workbench, grid: Grid, pos: Pos2) -> Option<Gesture> {
    let frame = frame(app, grid)?;
    frame
        .handles
        .iter()
        .any(|p| Rect::from_center_size(*p, vec2(26., 26.)).contains(pos))
        .then_some(Gesture::SelectionScale {
            selection: frame.selection,
            grid,
            origin: pos,
            anchor: frame.anchor,
            factor: 1.,
            moved: false,
        })
}
pub(crate) fn paint(app: &mut Workbench, p: &egui::Painter, grid: Grid) {
    if let Some(frame) = frame(app, grid) {
        let p = p.with_clip_rect(grid.rect);
        p.rect_stroke(
            frame.bounds,
            2.,
            egui::Stroke::new(1_f32, accent().gamma_multiply(0.6)),
        );
        for handle in frame.handles {
            p.rect_filled(Rect::from_center_size(handle, vec2(12., 14.)), 2., accent());
            p.line_segment(
                [handle - vec2(2., 4.), handle + vec2(-2., 4.)],
                egui::Stroke::new(1_f32, base()),
            );
            p.line_segment(
                [handle - vec2(-2., 4.), handle + vec2(2., 4.)],
                egui::Stroke::new(1_f32, base()),
            );
        }
        let at = grid.x(frame.anchor);
        if at >= grid.rect.left() && at <= grid.rect.right() {
            p.circle_stroke(
                pos2(at, frame.bounds.center().y),
                4.,
                egui::Stroke::new(1_f32, accent()),
            );
        }
    }
}
