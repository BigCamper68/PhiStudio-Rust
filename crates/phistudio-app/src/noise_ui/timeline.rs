//! Two time-synchronized surfaces use the existing grid router and divider.
use super::*;
use crate::grid_input;
use egui::{Align2, FontId, Rect, Sense, Stroke, pos2, vec2};
pub(crate) fn show(app: &mut Workbench, ui: &mut egui::Ui, canceled: bool, resizing: bool) {
    let size = ui.available_size();
    if size.x < 80. || size.y < 90. {
        return;
    }
    let (frame, _) = ui.allocate_exact_size(size, Sense::hover());
    let area = Rect::from_min_max(frame.min + vec2(0., 32.), frame.max);
    ui.painter().rect_filled(frame, 0., base());
    let initial = grid_input::geometry(area, app);
    let splitter = Rect::from_min_max(
        pos2(initial.rect.right(), frame.top()),
        pos2(initial.event_rect.left(), frame.bottom()),
    );
    let response = ui
        .interact(splitter, ui.id().with("noise-divider"), Sense::drag())
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    if !app.modal_open()
        && response.dragged()
        && let Some(pos) = response.interact_pointer_pos()
    {
        app.timeline_split =
            ((pos.x - initial.full_rect().left()) / initial.full_rect().width()).clamp(0.35, 0.7);
        app.drag = None;
        app.grid_input.cancel();
    }
    let grid = grid_input::geometry(area, app);
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
    app.grid_area = area;
    app.grid_hover = ui.input(|i| i.pointer.hover_pos());
    let mut events = ui.input(|i| i.events.clone());
    if ui.input(|i| i.modifiers.shift) {
        for event in &mut events {
            if let egui::Event::Scroll(d) = event
                && d.x.abs() < d.y.abs()
            {
                *d = vec2(d.y, 0.);
            }
        }
    }
    let input = Rect::from_min_max(pos2(frame.left(), grid.rect.top()), area.max);
    if !app.modal_open() {
        grid_input::handle(
            app,
            &events,
            grid,
            input,
            |pos| {
                !resizing && !response.dragged() && ui.ctx().layer_id_at(pos) == Some(ui.layer_id())
            },
            canceled,
        );
    } else {
        app.drag = None;
        app.grid_input.cancel();
    }
    let grid = grid_input::geometry(area, app);
    let p = ui.painter_at(frame);
    if app.options.preview_behind_grid {
        app.paint_grid_preview(&p, grid.full_rect());
    }
    let first = (grid.raw_beat(grid.rect.bottom() + 12.) * app.division as f64).floor() as i64;
    let count = (((grid.rect.height() + 24.) / grid.zoom) * app.division as f32).ceil() as i64 + 2;
    let clip = Rect::from_min_max(
        pos2(frame.left(), grid.rect.top()),
        pos2(grid.event_rect.right(), grid.rect.bottom()),
    );
    let q = p.with_clip_rect(clip);
    for tick in first..first + count {
        let beat = tick as f64 / app.division as f64;
        let y = grid.y(beat);
        let main = tick.rem_euclid(app.division as i64) == 0;
        if !main && !app.options.minor_grid {
            continue;
        }
        for rect in [grid.rect, grid.event_rect] {
            q.line_segment(
                [pos2(rect.left(), y), pos2(rect.right(), y)],
                Stroke::new(
                    if main { 1_f32 } else { 0.5_f32 },
                    if main { border() } else { raised() },
                ),
            );
        }
        if main && app.options.beat_labels {
            q.text(
                pos2(grid.rect.left() - 12., y),
                Align2::RIGHT_CENTER,
                format!("{beat:.0}"),
                FontId::monospace(12.),
                muted(),
            );
        }
    }
    super::grid::paint(app, &p, grid);
    p.rect_filled(
        splitter,
        0.,
        if response.hovered() || response.dragged() {
            raised()
        } else {
            base()
        },
    );
    p.line_segment(
        [splitter.center_top(), splitter.center_bottom()],
        Stroke::new(1_f32, border()),
    );
    let y = grid.baseline();
    for rect in [grid.rect, grid.event_rect] {
        q.line_segment(
            [pos2(rect.left(), y), pos2(rect.right(), y)],
            Stroke::new(1.5_f32, accent()),
        );
    }
    q.rect_filled(
        Rect::from_center_size(pos2(frame.left() + 22., y), vec2(42., 22.)),
        4.,
        accent(),
    );
    q.text(
        pos2(frame.left() + 22., y),
        Align2::CENTER_CENTER,
        format!("{:.1}", app.grid_beat()),
        FontId::monospace(11.),
        base(),
    );
}
