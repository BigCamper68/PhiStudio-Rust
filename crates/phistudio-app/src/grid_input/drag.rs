use super::*;

pub(super) fn update(app: &mut Workbench, pos: Pos2) {
    let Some(mut gesture) = app.drag.take() else {
        return;
    };
    match &mut gesture {
        Gesture::SelectionScale {
            grid,
            origin,
            anchor,
            factor,
            moved,
            ..
        } => {
            *moved |= pos.distance(*origin) > SLOP;
            if *moved {
                let d = origin.x - grid.x(*anchor);
                if d.abs() > 1. {
                    *factor = (1. + f64::from((pos.x - origin.x) / d)).clamp(-100., 100.);
                }
            }
        }
        Gesture::Box { current, .. } => *current = pos,
        Gesture::Erase {
            selection,
            stories,
            grid,
            last,
        } => {
            let steps = ((pos.distance(*last) / 8.).ceil() as usize).clamp(1, 256);
            for i in 1..=steps {
                let next = erase_at(
                    app,
                    *grid,
                    *last + (pos - *last) * (i as f32 / steps as f32),
                );
                selection.notes.extend(next.notes);
                selection.events.extend(next.events);
                if crate::story_grid::active(app) {
                    stories.extend(crate::story_grid::hits(
                        app,
                        grid.events(),
                        *last + (pos - *last) * (i as f32 / steps as f32),
                    ));
                }
            }
            *selection = std::mem::take(selection).normalized();
            *last = pos;
        }
        Gesture::SelectionMove {
            origin,
            grid,
            beats,
            x,
            earliest,
            x_range,
            note_surface,
            moved,
            ..
        } => {
            *moved |= pos.distance(*origin) > SLOP;
            if *moved {
                if let Ok(delta) = Beat::from_grid(
                    grid.raw_beat(pos.y) - grid.raw_beat(origin.y),
                    grid.division,
                ) {
                    *beats = delta.max(Beat::ZERO.checked_sub(*earliest).unwrap_or(Beat::ZERO));
                }
                if *note_surface {
                    let step = 1350. / (grid.lanes.max(2) - 1) as f64;
                    let raw = ((pos.x - origin.x) / grid.rect.width()) as f64 * 1350.;
                    let delta = if grid.snap_x_enabled {
                        (raw / step).round() * step
                    } else {
                        raw
                    };
                    *x = if grid.camera_x == 0. && x_range[0] >= -675. && x_range[1] <= 675. {
                        delta.clamp(-675. - x_range[0], 675. - x_range[1])
                    } else {
                        delta
                    };
                }
            }
        }
        Gesture::Event(gesture) => crate::event_grid::update(gesture, pos),
        Gesture::Story(d) => crate::story_grid::update(d, pos),
        Gesture::Noise(d) => crate::noise_ui::grid::update(app, d, pos),
        Gesture::Shader(d) => crate::shader_ui::grid::update(app, d, pos),
        Gesture::Tap {
            origin,
            grid,
            moved,
            intent,
        } => {
            let panel_scroll =
                if matches!(intent, Intent::SelectEvent(..) | Intent::SelectStory(..)) {
                    app.multi_active(true)
                } else {
                    app.multi_active(false) && !app.multi_line.merged
                };
            let delta = pos - *origin;
            *moved |= delta.length() > SLOP;
            if *moved {
                if app.navigation_axis.is_none() {
                    if delta.y.abs() > 8. && delta.y.abs() >= delta.x.abs() * 0.6 {
                        app.navigation_axis = Some(false);
                    } else if matches!(
                        intent,
                        Intent::Select(..)
                            | Intent::SelectEvent(..)
                            | Intent::SelectStory(..)
                            | Intent::Place(..)
                            | Intent::Pan
                    ) && (!matches!(
                        intent,
                        Intent::SelectEvent(..) | Intent::SelectStory(..)
                    ) || app.multi_active(true))
                        && (!app.options.lock_horizontal || panel_scroll)
                        && grid.rect.contains(*origin)
                        && delta.x.abs() > 18.
                        && delta.x.abs() > delta.y.abs() * 2.
                    {
                        app.navigation_axis = Some(true);
                    }
                }
                let Some(horizontal) = app.navigation_axis else {
                    app.drag = Some(gesture);
                    return;
                };
                if horizontal {
                    if panel_scroll {
                        app.grid_input.panel_navigation = true;
                        let i = usize::from(app.multi_line.events);
                        app.multi_line.offsets[i] = (app.grid_input.panel_offset - delta.x).max(0.);
                    } else {
                        app.camera_x = (grid.camera_x
                            - delta.x as f64 / grid.rect.width() as f64 * 1350.)
                            .clamp(-100_000., 100_000.);
                    }
                } else {
                    app.set_grid_beat(
                        (grid.beat + (delta.y * grid.scroll_speed / grid.zoom) as f64).max(0.),
                    );
                }
            }
        }
        Gesture::Create {
            start, end, grid, ..
        } => {
            *end = grid.beat(pos.y).max(
                start
                    .checked_add(Beat::new(0, 1, grid.division).unwrap())
                    .unwrap_or(*start),
            );
        }
        Gesture::Move {
            original,
            current,
            origin,
            grid,
            moved,
            ..
        } => {
            *moved |= pos.distance(*origin) > SLOP;
            if *moved {
                *current = original.clone();
                current.x = grid
                    .snap_x(original.x + ((pos.x - origin.x) / grid.rect.width()) as f64 * 1350.);
                if let Ok(delta) = grid.beat(pos.y).checked_sub(grid.beat(origin.y))
                    && let Ok(start) = original.start_time.checked_add(delta)
                {
                    let start = start.max(Beat::ZERO);
                    if let Ok(end) = start
                        .checked_sub(original.start_time)
                        .and_then(|d| original.end_time.checked_add(d))
                    {
                        current.start_time = start;
                        current.end_time = end;
                    }
                }
            }
        }
        Gesture::Resize {
            current,
            start_handle,
            origin,
            grid,
            moved,
            ..
        } => {
            *moved |= pos.distance(*origin) > SLOP;
            if *moved {
                let tick = Beat::new(0, 1, grid.division).unwrap();
                if *start_handle {
                    current.start_time = grid
                        .beat(pos.y)
                        .min(current.end_time.checked_sub(tick).unwrap_or(Beat::ZERO))
                        .max(Beat::ZERO);
                } else {
                    current.end_time = grid.beat(pos.y).max(
                        current
                            .start_time
                            .checked_add(tick)
                            .unwrap_or(current.start_time),
                    );
                }
            }
        }
    }
    app.drag = Some(gesture);
}
