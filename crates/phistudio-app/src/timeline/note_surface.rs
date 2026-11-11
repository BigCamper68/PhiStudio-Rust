//! Note surface painting, including multiline panes and in-progress gestures.
use super::*;
pub(super) fn paint(
    app: &mut Workbench,
    painter: &egui::Painter,
    grid_painter: &egui::Painter,
    grid: Grid,
    frame: Rect,
    show_audio_guide: bool,
) {
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
            let note_background = grid_painter.with_clip_rect(grid.rect.intersect(base_grid.rect));
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
                    [pos2(frame.left(), y), pos2(grid.rect.right(), y)],
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
        match &app.timeline.drag {
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
        if app.timeline.drag.is_some() {
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
            if let Some(Gesture::Erase { selection, .. }) = &app.timeline.drag
                && selection.notes.binary_search(&id).is_ok()
            {
                continue;
            }
            let group_preview = match &app.timeline.drag {
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
                Some(Gesture::Story(d)) if d.normal.notes.binary_search(&id).is_ok() && d.moved => {
                    let mut note = note.clone();
                    if let Ok(delta) = d.event.start_time.checked_sub(d.original.start_time) {
                        note.start_time = note
                            .start_time
                            .checked_add(delta)
                            .unwrap_or(note.start_time);
                        note.end_time = note.end_time.checked_add(delta).unwrap_or(note.end_time);
                    }
                    Some(note)
                }
                _ => None,
            };
            let visual = match &app.timeline.drag {
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
                app.selected == Some(id) || app.tools.selection.notes.binary_search(&id).is_ok(),
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
            }) = &app.timeline.drag
        {
            let mut ghost = Note {
                kind: *kind,
                start_time: *start,
                end_time: if *kind == NoteKind::Hold {
                    (*end).max(*start)
                } else {
                    *start
                },
                x: *x,
                ..Default::default()
            };
            app.options
                .new_note_defaults
                .apply_style(&mut ghost, phistudio_core::NoteCreationPolicy::Regular);
            draw_note(&mut batch, &grid, &ghost, true, true, skin);
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
