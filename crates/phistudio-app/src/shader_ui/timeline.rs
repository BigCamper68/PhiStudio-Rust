//! Shader effects and all selected-shader uniforms share one paired timeline.
use super::*;
use egui::pos2;
pub(crate) fn show(app: &mut Workbench, ui: &mut egui::Ui, canceled: bool, resizing: bool) {
    let grid = crate::grid_input::geometry(ui.available_rect_before_wrap(), app);
    let header_height = if layout::max_offset(app, grid.events()) > 0. {
        80.
    } else {
        64.
    };
    crate::timeline::paired::show(
        app,
        ui,
        canceled,
        resizing,
        header_height,
        header,
        super::grid::paint,
    );
}
fn header(app: &mut Workbench, ui: &mut egui::Ui, grid: crate::grid_input::Grid, frame: Rect) {
    for keys in [false, true] {
        let surface = if keys { grid.event_rect } else { grid.rect };
        let header = Rect::from_min_max(
            pos2(surface.left(), frame.top()),
            pos2(surface.right(), frame.top() + 64.),
        );
        ui.allocate_ui_at_rect(header, |ui| {
            ui.set_clip_rect(header.intersect(ui.clip_rect()));
            if keys {
                app.shader_navigation(ui);
            } else {
                app.shader_left_navigation(ui);
            }
        });
    }
    let max_offset = layout::max_offset(app, grid.events());
    app.shaders.offsets[1] = app.shaders.offsets[1].clamp(0., max_offset);
    if max_offset > 0. {
        let scrollbar = Rect::from_min_max(
            pos2(grid.event_rect.left(), frame.top() + 64.),
            pos2(grid.event_rect.right(), frame.top() + 80.),
        );
        ui.allocate_ui_at_rect(scrollbar, |ui| {
            ui.set_clip_rect(scrollbar.intersect(ui.clip_rect()));
            ui.spacing_mut().interact_size.y = 14.;
            ui.spacing_mut().slider_width = scrollbar.width();
            if ui
                .add(
                    egui::Slider::new(&mut app.shaders.offsets[1], 0. ..=max_offset)
                        .show_value(false),
                )
                .changed()
            {
                app.drag = None;
                app.grid_input.cancel();
            }
        });
    }
}
