//! Consume the primary pointer stream in order, including batched Android touches.
use super::*;
use egui::{Event, PointerButton};

pub(in crate::speed_curve_ui) struct PointerGesture {
    origin: Pos2,
    mode: Mode,
    moved: bool,
    started: f64,
    action: Action,
}

enum Action {
    Point,
    Pan { start: f64 },
    Seek,
    Range { edge: usize, before: [Beat; 2] },
}
impl PointerGesture {
    pub(in crate::speed_curve_ui) fn is_scrubbing(&self) -> bool {
        matches!(self.action, Action::Seek)
    }
    pub(in crate::speed_curve_ui) fn restore_range(self, range: &mut [Beat; 2]) {
        if let Action::Range { before, .. } = self.action {
            *range = before;
        }
    }
}
pub(super) fn handle(
    ui: &egui::Ui,
    state: &mut State,
    view: &View,
    division: i32,
    cursor: &mut f64,
) -> bool {
    if !ui.is_enabled() || state.input_canceled {
        return false;
    }
    let mut changed = false;
    let held = state.pointer.as_ref().is_some_and(|p| {
        !p.moved
            && !matches!(p.action, Action::Seek | Action::Range { .. })
            && ui.input(|i| i.time) - p.started > 0.5
    });
    if held {
        let pos = state.pointer.as_ref().unwrap().origin;
        state.cancel();
        state.context = Some(pos);
        return false;
    }
    let events = ui.input(|i| i.events.clone());
    for event in events {
        match event {
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                ..
            } if view.rect.intersect(ui.clip_rect()).contains(pos)
                && ui.ctx().layer_id_at(pos) == Some(ui.layer_id())
                && !ui.ctx().memory(|m| m.any_popup_open()) =>
            {
                if state.commit_pending {
                    continue;
                }
                state.error = None;
                state.selected = view.nearest(&state.points, pos);
                let cursor_x = view.point(*cursor, 0.).x;
                let range_edge = state
                    .play_range
                    .iter()
                    .position(|b| (view.point(b.as_f64(), 0.).x - pos.x).abs() < 14.);
                let action = if pos.y < view.rect.top() + 16. && (cursor_x - pos.x).abs() < 14. {
                    Action::Seek
                } else if pos.y > view.rect.bottom() - 18. && range_edge.is_some() {
                    Action::Range {
                        edge: range_edge.unwrap(),
                        before: state.play_range,
                    }
                } else if state.mode == Mode::Edit
                    && state.selected.is_none()
                    && (cursor_x - pos.x).abs() < 10.
                {
                    Action::Seek
                } else if (state.mode == Mode::Edit && state.selected.is_some())
                    || state.mode == Mode::Add
                {
                    Action::Point
                } else {
                    Action::Pan { start: state.start }
                };
                state.pointer = Some(PointerGesture {
                    origin: pos,
                    mode: state.mode,
                    moved: false,
                    started: ui.input(|i| i.time),
                    action,
                });
                if state
                    .pointer
                    .as_ref()
                    .is_some_and(|p| matches!(p.action, Action::Point))
                    && state.mode == Mode::Edit
                    && state.selected.is_some_and(|i| !state.points[i].fixed)
                {
                    state.drag = Some(state.points.clone());
                }
            }
            Event::PointerMoved(pos) if state.pointer.is_some() => {
                changed |= move_pointer(state, view, division, pos, cursor);
            }
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                ..
            } if state.pointer.is_some() => {
                // Android UP can carry the final position without a preceding MOVE.
                changed |= move_pointer(state, view, division, pos, cursor);
                let Some(pointer) = state.pointer.take() else {
                    continue;
                };
                if matches!(pointer.action, Action::Pan { .. }) && pointer.moved {
                    state.load_visible_range();
                    continue;
                }
                if matches!(pointer.action, Action::Seek | Action::Range { .. }) {
                    continue;
                }
                match pointer.mode {
                    Mode::Edit => {
                        if let Some(before) = state.drag.as_ref() {
                            if state.points != *before {
                                state.drag = None;
                                state.request_fit(true);
                                changed = true;
                            } else {
                                state.cancel();
                            }
                        }
                    }
                    Mode::Add if view.rect.contains(pos) => {
                        changed |= add_point(state, view, division, pos);
                    }
                    Mode::Delete if !pointer.moved && view.rect.contains(pos) => {
                        let count = state.points.len();
                        state.delete_selected();
                        changed |= state.points.len() != count;
                    }
                    _ => {}
                }
            }
            Event::PointerButton {
                pos,
                button: PointerButton::Secondary,
                pressed: false,
                ..
            } if view.rect.contains(pos) && ui.ctx().layer_id_at(pos) == Some(ui.layer_id()) => {
                state.cancel();
                state.selected = view.nearest(&state.points, pos);
                state.context = Some(pos);
            }
            _ => {}
        }
    }
    changed
}

fn move_pointer(
    state: &mut State,
    view: &View,
    division: i32,
    pos: Pos2,
    cursor: &mut f64,
) -> bool {
    let Some(pointer) = &mut state.pointer else {
        return false;
    };
    pointer.moved |= pointer.origin.distance(pos) > 6.;
    match pointer.action {
        Action::Seek => {
            *cursor = view.world(pos)[0].clamp(0., state.navigation_end());
            return false;
        }
        Action::Range { edge, .. } => {
            if let Ok(beat) = Beat::from_grid(
                view.world(pos)[0].clamp(0., state.navigation_end()),
                division.max(1),
            ) && ((edge == 0 && beat < state.play_range[1])
                || (edge == 1 && beat > state.play_range[0]))
            {
                state.play_range[edge] = beat;
            }
            return false;
        }
        Action::Pan { start } => {
            if pointer.moved {
                state.start =
                    start - f64::from((pos.x - pointer.origin.x) / view.rect.width()) * view.span;
                state.clamp_view();
            }
            return false;
        }
        Action::Point => {}
    }
    if pointer.mode != Mode::Edit || !pointer.moved || state.drag.is_none() {
        return false;
    }
    let Some(index) = state
        .selected
        .filter(|i| *i > 0 && *i + 1 < state.points.len())
    else {
        return false;
    };
    let [beat, distance] = view.world(pos);
    let Ok(beat) = Beat::from_grid(beat, division.max(1)) else {
        return false;
    };
    let beat = if beat > state.points[index - 1].beat && beat < state.points[index + 1].beat {
        beat
    } else {
        state.points[index].beat
    };
    let distance = if state.snap {
        (distance / state.snap_step).round() * state.snap_step
    } else {
        distance
    }
    .clamp(-1e9, 1e9);
    if state.points[index].beat == beat && state.points[index].distance == distance {
        return false;
    }
    state.points[index].beat = beat;
    state.points[index].distance = distance;
    state.request_fit(false);
    true
}

fn add_point(state: &mut State, view: &View, division: i32, pos: Pos2) -> bool {
    if state.points.len() >= phistudio_core::speed_curve::MAX_CURVE_POINTS {
        return false;
    }
    let [beat, distance] = view.world(pos);
    let Ok(beat) = Beat::from_grid(beat, division.max(1)) else {
        return false;
    };
    if beat <= state.range[0]
        || beat >= state.range[1]
        || state.source.as_ref().is_some_and(|s| {
            s.note.kind == phistudio_core::NoteKind::Hold && beat >= s.note.start_time
        })
    {
        return false;
    }
    if let Some(index) = state.points.iter().position(|p| p.beat == beat) {
        state.selected = Some(index);
        state.error = Some("Speed curve point times must be distinct".into());
        return false;
    }
    let Some(profile) = state.preview.as_ref().or(state.profile.as_deref()) else {
        return false;
    };
    let original = distance_at(profile, beat.as_f64());
    let on_curve = (view.point(beat.as_f64(), original).y - pos.y).abs() < 9.;
    let distance = if on_curve {
        original
    } else if state.snap {
        (distance / state.snap_step).round() * state.snap_step
    } else {
        distance
    };
    let index = state.points.partition_point(|p| p.beat < beat);
    state.points.insert(
        index,
        SpeedCurvePoint {
            beat,
            source_beat: beat,
            distance,
            easing: None,
            fixed: false,
        },
    );
    state.selected = Some(index);
    state.request_fit(true);
    true
}
