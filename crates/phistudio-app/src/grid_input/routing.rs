use super::*;

pub(super) fn pair(touches: &BTreeMap<u64, Pos2>) -> Option<(Pos2, f32)> {
    let mut points = touches.values();
    let a = *points.next()?;
    let b = *points.next()?;
    Some((a + (b - a) * 0.5, a.distance(b).max(12.)))
}

/// Process in event order, including a complete down/move/up batched in one
/// frame. `eligible` rejects presses covered by a window or popup.
pub fn handle(
    app: &mut Workbench,
    events: &[Event],
    grid: Grid,
    area: Rect,
    eligible: impl Fn(Pos2) -> bool,
    canceled: bool,
) {
    app.multi_line.base = Some(grid);
    if canceled {
        app.grid_input.cycle = None;
        app.drag = None;
        app.grid_input.pointer_active = false;
        // A second finger promotes the draft to a pinch. Other cancellations
        // also stop playback intent, including suspend and canceled touches.
        if !events.iter().any(|e| {
            matches!(
                e,
                Event::Touch {
                    phase: TouchPhase::Start,
                    ..
                }
            )
        }) {
            app.playing = false;
        }
    }
    let mut suppress_pointer = app.grid_input.pinch.is_some();
    for event in events {
        match *event {
            Event::Touch { id, phase, pos, .. } => match phase {
                TouchPhase::Start => {
                    if app.grid_input.touches.is_empty() {
                        app.grid_input.owns_touches = area.contains(pos) && eligible(pos);
                    }
                    if app.grid_input.owns_touches {
                        app.grid_input.touches.insert(id.0, pos);
                        if app.grid_input.pinch.is_none()
                            && let Some((center, distance)) = pair(&app.grid_input.touches)
                        {
                            app.drag = None;
                            app.grid_input.cycle = None;
                            app.grid_input.pinch = Some(Pinch {
                                grid,
                                center,
                                distance,
                            });
                            suppress_pointer = true;
                        }
                    }
                }
                TouchPhase::Move => {
                    if let Some(point) = app.grid_input.touches.get_mut(&id.0) {
                        *point = pos;
                    }
                    if let (Some(pinch), Some((center, distance))) =
                        (&app.grid_input.pinch, pair(&app.grid_input.touches))
                    {
                        app.zoom = (pinch.grid.zoom * distance / pinch.distance).clamp(24., 280.);
                        let anchor = pinch.grid.raw_beat(pinch.center.y);
                        app.set_grid_beat(
                            (anchor - ((pinch.grid.baseline() - center.y) / app.zoom) as f64)
                                .max(0.),
                        );
                    }
                }
                TouchPhase::End | TouchPhase::Cancel => {
                    if phase == TouchPhase::Cancel {
                        app.drag = None;
                        app.grid_input.pointer_active = false;
                        app.playing = false;
                    }
                    app.grid_input.touches.remove(&id.0);
                    if app.grid_input.touches.is_empty() {
                        app.grid_input.pinch = None;
                        app.grid_input.owns_touches = false;
                    }
                }
            },
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers,
            } if !suppress_pointer && !canceled => {
                if pressed {
                    if area.contains(pos)
                        && eligible(pos)
                        && let Some(panel) = app.input_line_grid(grid, pos)
                    {
                        begin(
                            app,
                            panel,
                            pos,
                            app.tools.additive
                                || modifiers.shift
                                || modifiers.ctrl
                                || modifiers.command,
                        );
                    }
                } else if app.drag.is_some() || app.grid_input.pointer_active {
                    update(app, pos);
                    finish(app);
                }
            }
            Event::PointerMoved(pos) if !suppress_pointer && !canceled => update(app, pos),
            Event::Zoom(factor) if factor.is_finite() && factor > 0. => {
                if let Some(pos) = app.grid_hover.filter(|p| area.contains(*p) && eligible(*p)) {
                    let anchor = grid.raw_beat(pos.y);
                    app.zoom = (app.zoom * factor).clamp(24., 280.);
                    app.set_grid_beat(
                        (anchor - ((grid.baseline() - pos.y) / app.zoom) as f64).max(0.),
                    );
                    app.drag = None;
                    app.grid_input.cycle = None;
                    app.grid_input.pointer_active = false;
                }
            }
            Event::Scroll(delta)
                if app
                    .grid_hover
                    .is_some_and(|p| area.contains(p) && eligible(p)) =>
            {
                if app.advanced.right_mode == crate::advanced_ui::RightMode::NoiseFields
                    && delta.x.abs() > delta.y.abs()
                {
                    let keys = app.grid_hover.is_some_and(|p| grid.event_rect.contains(p));
                    if !keys {
                        crate::noise_ui::layout::pan(app, grid, -f64::from(delta.x));
                    }
                } else if delta.x.abs() > delta.y.abs()
                    && app
                        .grid_hover
                        .is_some_and(|p| app.multi_active(grid.event_rect.contains(p)))
                {
                    let i = usize::from(app.multi_line.events);
                    app.multi_line.offsets[i] = (app.multi_line.offsets[i] - delta.x).max(0.);
                } else {
                    app.set_grid_beat(
                        (app.grid_beat()
                            + (delta.y * app.options.scroll_speed) as f64 / app.zoom as f64)
                            .max(0.),
                    );
                }
                app.drag = None;
                app.grid_input.cycle = None;
                app.grid_input.pointer_active = false;
            }
            _ => {}
        }
    }
}
