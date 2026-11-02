//! Timeline coordinates are editor metadata, independent of shader uniforms.
use super::*;
use crate::grid_input::Grid;
use egui::{Pos2, pos2};

pub(super) const EFFECT_WIDTH: f32 = 52.;
const POSITION: &str = "phiStudioEditorX";

pub(super) fn lane(raw: &Value, index: usize) -> f64 {
    raw[POSITION]
        .as_f64()
        .filter(|x| x.is_finite())
        .unwrap_or(index as f64)
}
pub(super) fn place(raw: &mut Value, lane: f64) {
    raw[POSITION] = json!(lane);
}
pub(super) fn width(app: &Workbench, grid: Grid, keys: bool) -> f32 {
    if keys {
        (grid.rect.width() / app.shaders.parameters.len().max(1) as f32).max(76.)
    } else {
        EFFECT_WIDTH
    }
}
pub(super) fn column(app: &Workbench, grid: Grid, keys: bool, lane: f64) -> Rect {
    let w = width(app, grid, keys);
    let x =
        grid.rect.left() + ((lane - app.shaders.offsets[usize::from(keys)]) * f64::from(w)) as f32;
    Rect::from_min_max(pos2(x, grid.rect.top()), pos2(x + w, grid.rect.bottom()))
}
pub(super) fn at(app: &Workbench, grid: Grid, keys: bool, pos: Pos2) -> f64 {
    let x = f64::from((pos.x - grid.rect.left()) / width(app, grid, keys))
        + app.shaders.offsets[usize::from(keys)];
    if keys { x.floor() } else { snap(grid, x - 0.5) }
}
pub(super) fn snap(grid: Grid, lane: f64) -> f64 {
    if grid.snap_x_enabled {
        let step = f64::from(grid.rect.width() / (grid.lanes.max(2) - 1) as f32 / EFFECT_WIDTH);
        (lane / step).round() * step
    } else {
        lane
    }
}
pub(super) fn parameter_at(app: &Workbench, grid: Grid, pos: Pos2) -> Option<String> {
    let index = at(app, grid, true, pos);
    (index >= 0.)
        .then(|| app.shaders.parameters.get(index as usize))
        .flatten()
        .map(|p| p.name.clone())
}
pub(super) fn parameter_column(app: &Workbench, grid: Grid, name: &str) -> Option<Rect> {
    let index = app.shaders.parameters.iter().position(|p| p.name == name)?;
    Some(column(app, grid, true, index as f64))
}
pub(crate) fn pan(app: &mut Workbench, grid: Grid, keys: bool, pixels: f64) {
    let grid = if keys { grid.events() } else { grid };
    let index = usize::from(keys);
    let offset = app.shaders.offsets[index] + pixels / f64::from(width(app, grid, keys));
    if offset.is_finite() {
        app.shaders.offsets[index] = if keys {
            offset.clamp(0., max_offset(app, grid))
        } else {
            offset
        };
    }
}
pub(crate) fn navigate(app: &mut Workbench, grid: Grid, pixels: f32) {
    let keys = app.shaders.animation_track().is_some();
    app.shaders.offsets[usize::from(keys)] = grid.camera_x;
    pan(app, grid, keys, -f64::from(pixels));
}
pub(super) fn max_offset(app: &Workbench, grid: Grid) -> f64 {
    (app.shaders.parameters.len() as f64 - f64::from(grid.rect.width() / width(app, grid, true)))
        .max(0.)
}
impl Workbench {
    pub(super) fn focus_shader_grid(&mut self, parameter: Option<String>) -> bool {
        let page = if parameter.is_some() {
            ShaderPage::Animation
        } else {
            ShaderPage::Effect
        };
        if self.shaders.page == page && (parameter.is_none() || self.shaders.track == parameter) {
            return true;
        }
        if !self.flush_shader_draft() {
            return false;
        }
        self.refresh_shaders();
        self.shaders.page = page;
        self.shaders.track = parameter;
        self.shaders.animation = None;
        self.timeline.input.cycle = None;
        true
    }
    pub(super) fn jump_shader(&mut self, index: usize, grid: Grid) {
        self.select_shader(index);
        if self.shaders.selected != Some(index) {
            return;
        }
        let lane = lane(&self.shaders.source["effects"][index], index);
        self.shaders.offsets[0] = lane + 0.5 - f64::from(grid.rect.width() / EFFECT_WIDTH) / 2.;
        self.shaders.offsets[1] = 0.;
        self.set_grid_beat((self.shaders.effects[index].start.as_f64() - 1.).max(0.));
        self.transport.playing = false;
        self.timeline.cancel_gesture();
    }
}
