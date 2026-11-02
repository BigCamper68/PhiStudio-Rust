//! Editor columns are separate from the field's playable rectangle.
use super::*;
use crate::grid_input::Grid;
use egui::{Pos2, Rect, pos2};
pub const FIELD_WIDTH: f32 = 52.;
const POSITION: &str = "editorPositionX";
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct View {
    pub keys: bool,
    pub track: Track,
}
impl View {
    pub fn focus(app: &Workbench) -> Self {
        Self {
            keys: app.noise.animation,
            track: app.noise.track,
        }
    }
}
pub(super) fn lane(raw: &Value, index: usize) -> f64 {
    raw[POSITION]
        .as_f64()
        .filter(|x| x.is_finite())
        .map_or(index as f64, |x| (x + 540.) / 216.)
}
pub(super) fn place(raw: &mut Value, lane: f64) {
    raw[POSITION] = json!(lane * 216. - 540.);
}
pub(super) fn width(app: &Workbench, grid: Grid, keys: bool) -> f32 {
    if keys {
        grid.rect.width() / Track::all(app.noise_format()).len() as f32
    } else {
        FIELD_WIDTH
    }
}
pub(super) fn column(app: &Workbench, grid: Grid, view: View, index: f64) -> Rect {
    let w = width(app, grid, view.keys);
    let x = grid.rect.left()
        + ((index - if view.keys { 0. } else { app.noise.offset }) * f64::from(w)) as f32;
    Rect::from_min_max(pos2(x, grid.rect.top()), pos2(x + w, grid.rect.bottom()))
}
pub(super) fn centered_offset(grid: Grid, lane: f64) -> f64 {
    lane + 0.5 - f64::from(grid.rect.width()) / f64::from(FIELD_WIDTH) / 2.
}
pub(super) fn grid_step(grid: Grid) -> f32 {
    grid.rect.width() / (grid.lanes.max(2) - 1) as f32
}
pub(super) fn grid_lines(grid: Grid, offset: f64) -> impl Iterator<Item = f32> {
    let step = f64::from(grid_step(grid) / FIELD_WIDTH);
    let first = ((offset - 0.5) / step).floor();
    (0..grid.lanes + 2).map(move |n| {
        grid.rect.left()
            + (((first + n as f64) * step + 0.5 - offset) * f64::from(FIELD_WIDTH)) as f32
    })
}
pub(super) fn snap(grid: Grid, lane: f64) -> f64 {
    if grid.snap_x_enabled {
        let step = f64::from(grid_step(grid) / FIELD_WIDTH);
        (lane / step).round() * step
    } else {
        lane
    }
}
pub(super) fn at(app: &Workbench, grid: Grid, keys: bool, pos: Pos2) -> f64 {
    let lane = f64::from(pos.x - grid.rect.left()) / f64::from(width(app, grid, keys));
    if keys {
        lane.floor()
    } else {
        snap(grid, lane + app.noise.offset - 0.5)
    }
}
pub(super) fn shifted(app: &Workbench, grid: Grid, lane: f64, pixels: f32) -> f64 {
    snap(
        grid,
        lane + f64::from(pixels) / f64::from(width(app, grid, false)),
    )
}
pub(crate) fn pan(app: &mut Workbench, grid: Grid, pixels: f64) {
    let value = app.noise.offset + pixels / f64::from(width(app, grid, false));
    if value.is_finite() {
        app.noise.offset = value;
    }
}
impl Workbench {
    pub(super) fn focus_noise(&mut self, keys: bool, track: Track) -> bool {
        if self.noise.animation == keys && (!keys || self.noise.track == track) {
            return true;
        }
        if !self.flush_noise_draft() {
            return false;
        }
        self.noise.animation = keys;
        self.noise.track = track;
        self.noise.selection.clear();
        self.noise.key_selected = None;
        self.noise.key_draft = None;
        self.noise.cycle = None;
        true
    }
}
