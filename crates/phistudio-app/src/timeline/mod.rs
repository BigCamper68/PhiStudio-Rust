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
    let shaders = app.advanced.right_mode.dedicated();
    let area = Rect::from_min_max(
        frame.min
            + vec2(
                0.,
                if shaders {
                    64.
                } else if app.multi_line.enabled {
                    72.
                } else {
                    32.
                },
            ),
        frame.max,
    );
    let painter = ui.painter_at(frame);
    painter.rect_filled(frame, 0., base());
    let mut divider_active = false;
    if !shaders {
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
            app.timeline_split =
                ((pos.x - initial.rect.left()) / initial.full_rect().width()).clamp(0.35, 0.7);
        }
        divider_active = splitter.hovered() || splitter.dragged();
    }
    let grid = grid_input::geometry(area, app);
    if app.options.preview_behind_grid {
        // Fit the complete image inside the data surface. Navigation, coordinate
        // labels and track titles live above this rect and must remain opaque.
        app.paint_grid_preview(&painter, grid.full_rect());
    }
    if shaders {
        let header = Rect::from_min_max(
            pos2(grid.rect.left(), frame.top()),
            pos2(grid.rect.right(), frame.top() + 64.),
        );
        ui.allocate_ui_at_rect(header, |ui| {
            ui.set_clip_rect(header.intersect(ui.clip_rect()));
            app.shader_navigation(ui);
        });
    } else {
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
                            let __i18n_arg_0 = &(app.camera_x);
                            crate::i18n::format("X {0}", &[format!("{:.0}", __i18n_arg_0)])
                        }),
                    )
                    .on_hover_text(crate::i18n::tr(
                        "Reset horizontal camera; ±675 is the standard playfield",
                    ))
                    .clicked()
                {
                    app.camera_x = 0.;
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
                    app.drag = None;
                    app.grid_input.cancel();
                    app.navigation_axis = None;
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
    }
    app.multi_line.base = Some(grid);
    app.multi_scrollbar(ui, grid, false);
    app.multi_scrollbar(ui, grid, true);
    app.grid_area = area;
    app.grid_hover = ui.input(|i| i.pointer.hover_pos());
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
                    && (!shaders || pos.x <= grid.event_rect.right())
                    && (pos.x <= grid.rect.right() || pos.x >= grid.event_rect.left())
            },
            canceled,
        );
    } else {
        app.drag = None;
        app.grid_input.cancel();
    }
    let grid = grid_input::geometry(area, app);
    // Keep whole glyphs in the draw list until they leave the viewport. Scissoring
    // hides only the part beyond an edge, including half-pixel grid strokes.
    let viewport = Rect::from_min_max(
        pos2(frame.left(), grid.rect.top()),
        pos2(grid.event_rect.right(), grid.rect.bottom()),
    );
    let grid_painter = painter.with_clip_rect(viewport.intersect(painter.clip_rect()));
    let show_audio_guide = !shaders
        && app.audio_guide.current(
            app.session,
            app.asset_revision,
            app.project_options.music.as_deref(),
        );
    let first = (grid.raw_beat(grid.rect.bottom() + 12.) * app.division as f64).floor() as i64;
    let rows = (((grid.rect.height() + 24.) / grid.zoom) * app.division as f32).ceil() as i64 + 2;
    for tick in first..first + rows {
        let beat = tick as f64 / app.division as f64;
        let y = grid.y(beat);
        let main = tick.rem_euclid(app.division as i64) == 0;
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
    if !shaders {
        let original_line = app.line;
        let base_grid = grid;
        // Paint through column gutters before each pane draws its notes.
        if app.multi_active(false) && !app.multi_line.merged {
            let y = grid.baseline();
            grid_painter.line_segment(
                [pos2(grid.rect.left(), y), pos2(grid.rect.right(), y)],
                Stroke::new(1.5_f32, accent()),
            );
        }
        for (line, grid) in app.line_panels(base_grid, false) {
            app.line = line;
            let pane_clip = Rect::from_min_max(
                pos2(
                    grid.rect.left().max(base_grid.rect.left()),
                    frame.top() + 32.,
                ),
                pos2(
                    grid.rect.right().min(base_grid.rect.right()),
                    frame.bottom(),
                ),
            );
            let painter = painter.with_clip_rect(pane_clip);
            let grid_painter = grid_painter.with_clip_rect(grid.rect.intersect(base_grid.rect));
            if !app.multi_active(false)
                || !app.multi_line.merged
                || Some(&line) == app.multi_line.lines.first()
            {
                let note_background =
                    grid_painter.with_clip_rect(grid.rect.intersect(base_grid.rect));
                for (left, right) in [
                    (grid.rect.left(), grid.x(-675.).min(grid.rect.right())),
                    (grid.x(675.).max(grid.rect.left()), grid.rect.right()),
                ] {
                    if right > left {
                        note_background.rect_filled(
                            Rect::from_min_max(
                                pos2(left, grid.rect.top()),
                                pos2(right, grid.rect.bottom()),
                            ),
                            0.,
                            base().gamma_multiply(0.85),
                        );
                    }
                }
                let step = 1350. / (grid.lanes.max(2) - 1) as f64;
                let first = ((grid.camera_x - 675.) / step).floor() as i64;
                for lane in first..first + grid.lanes as i64 + 2 {
                    let world = lane as f64 * step;
                    let x = grid.x(world);
                    if x >= grid.rect.left() && x <= grid.rect.right() {
                        grid_painter.line_segment(
                            [pos2(x, grid.rect.top()), pos2(x, grid.rect.bottom())],
                            Stroke::new(
                                0.5_f32,
                                if world.abs() < 0.01 {
                                    border()
                                } else {
                                    border().gamma_multiply(0.55)
                                },
                            ),
                        );
                    }
                }
                let note_clip = grid_painter.with_clip_rect(grid.rect.intersect(base_grid.rect));
                for world in [-675., 675.] {
                    let x = grid.x(world);
                    note_clip.line_segment(
                        [pos2(x, grid.rect.top()), pos2(x, grid.rect.bottom())],
                        Stroke::new(1.5_f32, accent().linear_multiply(0.6)),
                    );
                }
                let y = grid.baseline();
                if grid.rect.contains(pos2(grid.rect.center().x, y)) {
                    grid_painter.line_segment(
                        [pos2(area.left(), y), pos2(grid.rect.right(), y)],
                        Stroke::new(1.5_f32, accent()),
                    );
                }
                if show_audio_guide {
                    app.audio_guide.paint_waveform(
                        &grid_painter,
                        grid,
                        app.editor.document().tempo(),
                        app.project_options
                            .music_offset_seconds(app.editor.document()),
                        &app.options.audio_guide,
                        app.pixels_per_point,
                    );
                }
            }
            let notes = app.notes();
            let clipped = painter.with_clip_rect(grid.rect.intersect(base_grid.rect));
            let padding = note_margin(app.skin.as_ref(), grid.note_width);
            let mut candidates = app.note_candidates(
                grid.raw_beat(grid.rect.bottom() + padding),
                grid.raw_beat(grid.rect.top() - padding),
            );
            // A draft can enter the viewport from anywhere. Keep these raw IDs in
            // the same paint order as indexed stationary notes, including overlaps.
            match &app.drag {
                Some(Gesture::Move { id, .. } | Gesture::Resize { id, .. }) if id.line == line => {
                    candidates.push(id.index)
                }
                Some(
                    Gesture::SelectionMove { selection, .. }
                    | Gesture::SelectionScale { selection, .. },
                ) => candidates.extend(
                    selection
                        .notes
                        .iter()
                        .filter(|id| id.line == app.line)
                        .map(|id| id.index),
                ),
                Some(Gesture::Story(d)) if d.moved => candidates.extend(
                    d.normal
                        .notes
                        .iter()
                        .filter(|id| id.line == app.line)
                        .map(|id| id.index),
                ),
                _ => {}
            }
            if app.drag.is_some() {
                candidates.sort_unstable();
                candidates.dedup();
            }
            let skin = app
                .skin
                .as_ref()
                .map(|s| s.grid.as_ref().map_or(&s.normal, |g| &g.normal));
            let mut batch = NoteBatch::new(&clipped);
            for candidate in candidates {
                let Some((index, note)) = notes.get(candidate) else {
                    continue;
                };
                let id = NoteId {
                    line: app.line,
                    index: *index,
                };
                let skin = if app.options.preview_double_hint && app.note_highlights.contains(&id) {
                    app.skin
                        .as_ref()
                        .map(|s| s.grid.as_ref().map_or(&s.highlighted, |g| &g.highlighted))
                } else {
                    skin
                };
                if let Some(Gesture::Erase { selection, .. }) = &app.drag
                    && selection.notes.binary_search(&id).is_ok()
                {
                    continue;
                }
                let group_preview = match &app.drag {
                    Some(Gesture::SelectionScale {
                        selection,
                        anchor,
                        factor,
                        ..
                    }) if selection.notes.binary_search(&id).is_ok() => {
                        let mut note = note.clone();
                        note.x = anchor + (note.x - anchor) * factor;
                        Some(note)
                    }
                    Some(Gesture::SelectionMove {
                        selection,
                        beats,
                        x,
                        ..
                    }) if selection.notes.binary_search(&id).is_ok() => {
                        let mut note = note.clone();
                        if let (Ok(start), Ok(end)) = (
                            note.start_time.checked_add(*beats),
                            note.end_time.checked_add(*beats),
                        ) {
                            note.start_time = start;
                            note.end_time = end;
                            note.x += x;
                        }
                        Some(note)
                    }
                    Some(Gesture::Story(d))
                        if d.normal.notes.binary_search(&id).is_ok() && d.moved =>
                    {
                        let mut note = note.clone();
                        if let Ok(delta) = d.event.start_time.checked_sub(d.original.start_time) {
                            note.start_time = note
                                .start_time
                                .checked_add(delta)
                                .unwrap_or(note.start_time);
                            note.end_time =
                                note.end_time.checked_add(delta).unwrap_or(note.end_time);
                        }
                        Some(note)
                    }
                    _ => None,
                };
                let visual = match &app.drag {
                    Some(
                        Gesture::Move {
                            id: drag_id,
                            current,
                            ..
                        }
                        | Gesture::Resize {
                            id: drag_id,
                            current,
                            ..
                        },
                    ) if *drag_id == id => current,
                    _ => group_preview.as_ref().unwrap_or(note),
                };
                draw_note(
                    &mut batch,
                    &grid,
                    visual,
                    app.selected == Some(id)
                        || app.tools.selection.notes.binary_search(&id).is_ok(),
                    false,
                    skin,
                );
            }
            if line == original_line
                && let Some(Gesture::Create {
                    start,
                    end,
                    x,
                    kind,
                    ..
                }) = &app.drag
            {
                draw_note(
                    &mut batch,
                    &grid,
                    &Note {
                        kind: *kind,
                        start_time: *start,
                        end_time: if *kind == NoteKind::Hold {
                            (*end).max(*start)
                        } else {
                            *start
                        },
                        x: *x,
                        ..Note::default()
                    },
                    true,
                    true,
                    skin,
                );
            }
            batch.flush();
            app.paint_audio_note_preview(&clipped, line, grid);
            if !app.multi_active(false) {
                for (value, align) in [
                    (-675., Align2::LEFT_CENTER),
                    (0., Align2::CENTER_CENTER),
                    (675., Align2::RIGHT_CENTER),
                ] {
                    painter
                        .with_clip_rect(Rect::from_min_max(
                            pos2(grid.rect.left() - 1., grid.rect.top() - 22.),
                            pos2(grid.rect.right() + 1., grid.rect.top()),
                        ))
                        .text(
                            pos2(grid.x(value), grid.rect.top() - 11.),
                            align,
                            format!("{value:.0}"),
                            FontId::monospace(10.),
                            muted(),
                        );
                }
            }
            if app.multi_active(false) && (!app.multi_line.merged || line == original_line) {
                painter.text(
                    pos2(grid.rect.left() + 5., grid.rect.top() - 11.),
                    Align2::LEFT_CENTER,
                    format!(
                        "L{} · {}",
                        line,
                        app.line_summaries()
                            .get(line)
                            .map_or(String::new(), |l| l.name.clone())
                    ),
                    FontId::proportional(11.),
                    if line == original_line {
                        accent()
                    } else {
                        muted()
                    },
                );
            }
        }
        app.line = original_line;
    }
    if app.advanced.right_mode == crate::advanced_ui::RightMode::Shaders {
        crate::shader_ui::grid::paint(app, &painter, grid.events());
    } else {
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
    }
    if !shaders {
        let line = app.line;
        if let Some((_, panel)) = app
            .line_panels(grid, false)
            .into_iter()
            .find(|(i, _)| *i == line)
        {
            crate::selection_scaling::paint(app, &painter.with_clip_rect(grid.rect), panel);
        }
    }
    if let Some(Gesture::Box {
        origin, current, ..
    }) = &app.drag
    {
        let p = painter.with_clip_rect(viewport);
        let rect = Rect::from_two_pos(*origin, *current);
        p.rect_filled(rect, 0., accent().gamma_multiply(0.12));
        p.rect_stroke(rect, 0., Stroke::new(1_f32, accent()));
    }
    if let Some(Gesture::Erase { last, .. }) = &app.drag {
        painter
            .with_clip_rect(viewport)
            .circle_stroke(*last, 14., Stroke::new(1_f32, flick()));
    }
    if !shaders {
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
    }
    let y = grid.baseline();
    if grid.rect.contains(pos2(grid.rect.center().x, y)) {
        painter.line_segment(
            [
                pos2(
                    if shaders {
                        area.left()
                    } else {
                        grid.rect.right()
                    },
                    y,
                ),
                pos2(grid.event_rect.right(), y),
            ],
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
