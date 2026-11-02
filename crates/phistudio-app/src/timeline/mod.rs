mod note_surface;
mod notes;
pub(crate) mod paired;
pub use crate::grid_input::Gesture;
use crate::{
    grid_input::{self, Grid},
    theme::*,
    workbench::Workbench,
};
use egui::{Align2, Color32, FontId, Rect, Sense, Shape, Stroke, pos2, vec2};
pub(crate) use notes::note_margin;
#[cfg(test)]
use notes::repeat_span;
pub(crate) use notes::{NoteBatch, draw_note};
use phistudio_core::{Note, NoteId, NoteKind};

pub fn show(app: &mut Workbench, ui: &mut egui::Ui, canceled: bool, resizing_sidebar: bool) {
    if app.advanced.right_mode == crate::advanced_ui::RightMode::Shaders {
        app.refresh_shaders();
        if app.shader_source_view(ui) {
            return;
        }
        crate::shader_ui::timeline::show(app, ui, canceled, resizing_sidebar);
        return;
    }
    if app.advanced.right_mode == crate::advanced_ui::RightMode::NoiseFields {
        app.refresh_noise();
        crate::noise_ui::timeline::show(app, ui, canceled, resizing_sidebar);
        return;
    }
    app.multi_line
        .lines
        .retain(|i| *i < app.editor.document().line_count());
    if app.multi_line.enabled && app.multi_line.lines.is_empty() {
        app.multi_line
            .lines
            .push(app.line.min(app.editor.document().line_count() - 1));
    }
    let size = ui.available_size().max(vec2(1., 1.));
    if size.x < 80. || size.y < 90. {
        return;
    }
    let (frame, _) = ui.allocate_exact_size(size, Sense::hover());
    let area = Rect::from_min_max(
        frame.min + vec2(0., if app.multi_line.enabled { 72. } else { 32. }),
        frame.max,
    );
    let painter = ui.painter_at(frame);
    painter.rect_filled(frame, 0., base());

    let initial = grid_input::geometry(area, app);
    let split_rect = Rect::from_min_max(
        pos2(initial.rect.right(), frame.top()),
        pos2(initial.event_rect.left(), frame.bottom()),
    );
    let splitter = ui
        .interact(split_rect, ui.id().with("timeline-divider"), Sense::drag())
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    if !app.modal_open()
        && splitter.dragged()
        && let Some(pos) = splitter.interact_pointer_pos()
    {
        app.timeline.split =
            ((pos.x - initial.rect.left()) / initial.full_rect().width()).clamp(0.35, 0.7);
    }
    let divider_active = splitter.hovered() || splitter.dragged();

    let grid = grid_input::geometry(area, app);
    if app.options.preview_behind_grid {
        // Fit the complete image inside the data surface. Navigation, coordinate
        // labels and track titles live above this rect and must remain opaque.
        app.paint_grid_preview(&painter, grid.full_rect());
    }

    let header_h = 32.;
    let notes_header = Rect::from_min_max(
        pos2(grid.rect.left(), frame.top()),
        pos2(grid.rect.right(), frame.top() + header_h),
    );
    ui.allocate_ui_at_rect(notes_header, |ui| {
        ui.set_clip_rect(notes_header.intersect(ui.clip_rect()));
        ui.set_enabled(!app.modal_open());
        ui.spacing_mut().interact_size.y = 24.;
        navigation_row(ui, "notes-navigation-scroll", |ui| {
            ui.label(
                egui::RichText::new(crate::i18n::tr("Notes"))
                    .size(12.)
                    .color(muted()),
            );
            if ui
                .add_sized(
                    [60., 24.],
                    egui::Button::new({
                        let __i18n_arg_0 = &(app.timeline.camera_x);
                        crate::i18n::format("X {0}", &[format!("{:.0}", __i18n_arg_0)])
                    }),
                )
                .on_hover_text(crate::i18n::tr(
                    "Reset horizontal camera; ±675 is the standard playfield",
                ))
                .clicked()
            {
                app.timeline.camera_x = 0.;
            }
            let locked = app.options.lock_horizontal;
            if ui
                .selectable_label(
                    locked,
                    crate::i18n::tr(if locked { "X locked" } else { "Lock X" }),
                )
                .on_hover_text(crate::i18n::tr(
                    "Lock horizontal note-grid navigation; keep vertical scrubbing",
                ))
                .clicked()
            {
                app.options.lock_horizontal = !locked;
                app.timeline.cancel_gesture();
                app.timeline.navigation_axis = None;
            }
            app.audio_guide_button(ui);
            if ui.button(crate::i18n::tr("Multiple lines")).clicked() {
                app.open_multi_line();
            }
        });
    });
    let header = Rect::from_min_max(
        pos2(grid.event_rect.left(), frame.top()),
        pos2(grid.event_rect.right(), frame.top() + header_h),
    );
    ui.allocate_ui_at_rect(header, |ui| {
        ui.set_clip_rect(header.intersect(ui.clip_rect()));
        ui.set_enabled(!app.modal_open());
        ui.spacing_mut().interact_size.y = 24.;
        navigation_row(ui, "events-navigation-scroll", |ui| {
            use crate::advanced_ui::RightMode;
            app.workspace_selector(ui);
            if app.advanced.right_mode == RightMode::Events {
                app.event_layer_picker(ui);
            }
        });
    });

    app.multi_line.base = Some(grid);
    app.multi_scrollbar(ui, grid, false);
    app.multi_scrollbar(ui, grid, true);
    app.timeline.area = area;
    app.timeline.hover = ui.input(|i| i.pointer.hover_pos());
    let events = ui.input(|i| i.events.clone());
    let input_area = Rect::from_min_max(pos2(area.left(), grid.rect.top()), area.max);
    if !app.modal_open() {
        grid_input::handle(
            app,
            &events,
            grid,
            input_area,
            |pos| {
                !resizing_sidebar
                    && ui.ctx().layer_id_at(pos) == Some(ui.layer_id())
                    && (pos.x <= grid.rect.right() || pos.x >= grid.event_rect.left())
            },
            canceled,
        );
    } else {
        app.timeline.cancel_gesture();
    }
    let grid = grid_input::geometry(area, app);
    // Keep whole glyphs in the draw list until they leave the viewport. Scissoring
    // hides only the part beyond an edge, including half-pixel grid strokes.
    let viewport = Rect::from_min_max(
        pos2(frame.left(), grid.rect.top()),
        pos2(grid.event_rect.right(), grid.rect.bottom()),
    );
    let grid_painter = painter.with_clip_rect(viewport.intersect(painter.clip_rect()));
    let show_audio_guide = app.audio_guide.current(
        app.session,
        app.asset_revision,
        app.project_options.music.as_deref(),
    );
    let first =
        (grid.raw_beat(grid.rect.bottom() + 12.) * app.timeline.division as f64).floor() as i64;
    let rows =
        (((grid.rect.height() + 24.) / grid.zoom) * app.timeline.division as f32).ceil() as i64 + 2;
    for tick in first..first + rows {
        let beat = tick as f64 / app.timeline.division as f64;
        let y = grid.y(beat);
        let main = tick.rem_euclid(app.timeline.division as i64) == 0;
        if !main && !app.options.minor_grid {
            continue;
        }
        grid_painter.line_segment(
            [pos2(grid.rect.left(), y), pos2(grid.event_rect.right(), y)],
            Stroke::new(
                if main { 1_f32 } else { 0.5_f32 },
                if main { border() } else { raised() },
            ),
        );
        if main && app.options.beat_labels {
            grid_painter.text(
                pos2(grid.rect.left() - 12., y),
                Align2::RIGHT_CENTER,
                format!("{beat:.0}"),
                FontId::monospace(12.),
                muted(),
            );
        }
    }

    note_surface::paint(app, &painter, &grid_painter, grid, frame, show_audio_guide);

    let original_line = app.line;
    for (line, panel) in app.line_panels(grid, true) {
        app.line = line;
        let clip = Rect::from_min_max(
            pos2(
                panel.event_rect.left().max(grid.event_rect.left()),
                grid.event_rect.top() - 40.,
            ),
            pos2(
                panel.event_rect.right().min(grid.event_rect.right()),
                grid.event_rect.bottom(),
            ),
        );
        if crate::story_grid::active(app) {
            crate::story_grid::paint(app, &painter.with_clip_rect(clip), panel.events());
        } else {
            crate::event_grid::paint(app, &painter.with_clip_rect(clip), panel.events());
        }
        if app.multi_active(true) {
            painter.with_clip_rect(clip).text(
                pos2(panel.event_rect.left() + 4., panel.event_rect.top() - 30.),
                Align2::LEFT_CENTER,
                format!("L{line}"),
                FontId::monospace(11.),
                accent(),
            );
        }
    }
    app.line = original_line;

    let line = app.line;
    if let Some((_, panel)) = app
        .line_panels(grid, false)
        .into_iter()
        .find(|(i, _)| *i == line)
    {
        crate::selection_scaling::paint(app, &painter.with_clip_rect(grid.rect), panel);
    }

    if let Some(Gesture::Box {
        origin, current, ..
    }) = &app.timeline.drag
    {
        let p = painter.with_clip_rect(viewport);
        let rect = Rect::from_two_pos(*origin, *current);
        p.rect_filled(rect, 0., accent().gamma_multiply(0.12));
        p.rect_stroke(rect, 0., Stroke::new(1_f32, accent()));
    }
    if let Some(Gesture::Erase { last, .. }) = &app.timeline.drag {
        painter
            .with_clip_rect(viewport)
            .circle_stroke(*last, 14., Stroke::new(1_f32, flick()));
    }

    let split_rect = Rect::from_min_max(
        pos2(grid.rect.right(), frame.top()),
        pos2(grid.event_rect.left(), frame.bottom()),
    );
    painter.rect_filled(
        split_rect,
        0.,
        if divider_active { raised() } else { base() },
    );
    painter.line_segment(
        [split_rect.center_top(), split_rect.center_bottom()],
        Stroke::new(1_f32, border()),
    );

    let y = grid.baseline();
    if grid.rect.contains(pos2(grid.rect.center().x, y)) {
        painter.line_segment(
            [pos2(grid.rect.right(), y), pos2(grid.event_rect.right(), y)],
            Stroke::new(1.5_f32, accent()),
        );
        painter.rect_filled(
            Rect::from_center_size(pos2(area.left() + 22., y), vec2(42., 22.)),
            4.,
            accent(),
        );
        painter.text(
            pos2(area.left() + 22., y),
            Align2::CENTER_CENTER,
            format!("{:.1}", app.grid_beat()),
            FontId::monospace(11.),
            base(),
        );
    }
    if show_audio_guide {
        // An overlay is one audio surface regardless of how many lines share it.
        let surfaces = if app.multi_active(false) && app.multi_line.merged {
            1
        } else {
            usize::MAX
        };
        for (_, panel) in app.line_panels(grid, false).into_iter().take(surfaces) {
            // Passive attack ticks/diamonds stay above every grid stroke, including
            // the current-beat line. Waveform is above grid strokes, below notes.
            app.audio_guide.paint_markers(
                &grid_painter.with_clip_rect(grid.rect),
                panel,
                app.editor.document().tempo(),
                app.project_options
                    .music_offset_seconds(app.editor.document()),
                &app.options.audio_guide,
                app.pixels_per_point,
            );
        }
    }
}

pub(crate) fn navigation_row(ui: &mut egui::Ui, id: &str, content: impl FnOnce(&mut egui::Ui)) {
    use crate::scroll::TouchScroll as _;
    ui.spacing_mut().scroll_bar_width = 3.;
    ui.spacing_mut().scroll_bar_inner_margin = 1.;
    egui::ScrollArea::horizontal()
        .id_source(id)
        .touch_show_x(ui, |ui| {
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 28.),
                egui::Layout::left_to_right(egui::Align::Center),
                content,
            );
        });
}

// Work is proportional to the visible portion, even for a Hold spanning hours.
// Tiny repeat periods retain the native sampler instead of expanding geometry.

#[cfg(test)]
mod tests;
