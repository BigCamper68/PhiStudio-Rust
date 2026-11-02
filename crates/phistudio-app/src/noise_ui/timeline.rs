//! Noise Fields share the paired timeline's ruler, divider and touch navigation.
use super::*;
use egui::{Rect, pos2};
pub(crate) fn show(app: &mut Workbench, ui: &mut egui::Ui, canceled: bool, resizing: bool) {
    crate::timeline::paired::show(app, ui, canceled, resizing, 32., header, super::grid::paint);
}
fn header(app: &mut Workbench, ui: &mut egui::Ui, grid: crate::grid_input::Grid, frame: Rect) {
    for keys in [false, true] {
        let surface = if keys { grid.event_rect } else { grid.rect };
        let header = Rect::from_min_max(
            pos2(surface.left(), frame.top()),
            pos2(surface.right(), frame.top() + 32.),
        );
        ui.allocate_ui_at_rect(header, |ui| {
            ui.set_clip_rect(header.intersect(ui.clip_rect()));
            app.noise_navigation(ui, keys, grid);
        });
    }
}
