//! Process ordered pointer events and retain two-finger ownership until release.
use super::{
    graph::{Bounds, Hit, View},
    *,
};
use egui::{Event, PointerButton, Pos2, TouchPhase, Vec2};
use std::collections::BTreeMap;

pub(super) struct Pointer {
    pub before: Draft,
    origin: Pos2,
    view: Bounds,
    action: Action,
    moved: bool,
}
enum Action {
    Drag(Hit),
    Pan,
    Add,
}
#[derive(Default)]
pub(super) struct Navigation {
    fingers: BTreeMap<u64, Pos2>,
    pinch: Option<Pinch>,
    pub(super) owned: bool,
}
struct Pinch {
    center: Pos2,
    gap: f32,
    view: Bounds,
}
impl Navigation {
    fn pair(&self) -> Option<(Pos2, Pos2)> {
        let mut positions = self.fingers.values();
        Some((*positions.next()?, *positions.next()?))
    }
}
pub(super) fn handle(ui: &egui::Ui, state: &mut State, view: &View, response: &egui::Response) {
    let events = ui.input(|i| i.events.clone());
    let mut consumed = state.navigation.owned;
    for event in &events {
        let Event::Touch { id, phase, pos, .. } = event else {
            continue;
        };
        match phase {
            TouchPhase::Start
                if view.rect.contains(*pos)
                    && ui.ctx().layer_id_at(*pos) == Some(ui.layer_id()) =>
            {
                state.navigation.fingers.insert(id.0, *pos);
                if state.navigation.fingers.len() >= 2 {
                    state.cancel_gesture();
                    consumed = true;
                    state.navigation.owned = true;
                    state.navigation.pinch = None;
                    if state.navigation.fingers.len() == 2
                        && let Some((a, b)) = state.navigation.pair()
                    {
                        state.navigation.pinch = Some(Pinch {
                            center: a + (b - a) * 0.5,
                            gap: a.distance(b).max(12.),
                            view: state.view,
                        });
                    }
                }
            }
            TouchPhase::Move => {
                if let Some(finger) = state.navigation.fingers.get_mut(&id.0) {
                    *finger = *pos;
                }
                if let (Some((a, b)), Some(pinch)) =
                    (state.navigation.pair(), &state.navigation.pinch)
                {
                    let center = a + (b - a) * 0.5;
                    zoom_pan(
                        &mut state.view,
                        view.rect,
                        pinch.view,
                        pinch.center,
                        center,
                        f64::from(pinch.gap / a.distance(b).max(12.)),
                    );
                }
            }
            TouchPhase::End | TouchPhase::Cancel => {
                state.navigation.fingers.remove(&id.0);
                if state.navigation.fingers.len() < 2 {
                    state.navigation.pinch = None;
                }
                if state.navigation.fingers.is_empty() {
                    consumed |= state.navigation.owned;
                    state.navigation.owned = false;
                }
            }
            _ => {}
        }
    }
    if consumed {
        return;
    }
    if state.input_canceled {
        state.cancel_gesture();
        return;
    }
    if response.hovered() && !ui.ctx().memory(|m| m.any_popup_open()) {
        let (zoom, scroll, anchor) =
            ui.input(|i| (i.zoom_delta(), i.scroll_delta, i.pointer.hover_pos()));
        if let Some(anchor) = anchor
            && ((zoom - 1.).abs() > 0.0001 || scroll != Vec2::ZERO)
        {
            let before = state.view;
            let factor = if (zoom - 1.).abs() > 0.0001 {
                1. / f64::from(zoom)
            } else {
                (-f64::from(scroll.y) * 0.003).exp()
            };
            zoom_pan(
                &mut state.view,
                view.rect,
                before,
                anchor,
                anchor + egui::vec2(scroll.x, 0.),
                factor,
            );
        }
    }
    for event in events {
        match event {
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers,
            } if view.rect.intersect(ui.clip_rect()).contains(pos)
                && ui.ctx().layer_id_at(pos) == Some(ui.layer_id())
                && !ui.ctx().memory(|m| m.any_popup_open()) =>
            {
                state.finish_numeric();
                let hit = view.hit(&state.draft, pos);
                let before = state.draft.clone();
                let action = match state.tool {
                    Tool::Add => Action::Add,
                    Tool::Pan => Action::Pan,
                    Tool::Edit => match hit {
                        Some(hit) => {
                            if let Hit::Node(index) = hit {
                                if state.multiple || modifiers.shift {
                                    if !state.draft.selected.insert(index) {
                                        state.draft.selected.remove(&index);
                                    }
                                } else if !state.draft.selected.contains(&index) {
                                    state.draft.selected = BTreeSet::from([index]);
                                }
                            }
                            Action::Drag(hit)
                        }
                        None => Action::Pan,
                    },
                };
                state.pointer = Some(Pointer {
                    before: Draft {
                        curve: before.curve,
                        selected: state.draft.selected.clone(),
                        preserve_domain: before.preserve_domain,
                    },
                    origin: pos,
                    view: state.view,
                    action,
                    moved: false,
                });
                state.error = None;
            }
            Event::PointerMoved(pos) if state.pointer.is_some() => {
                move_pointer(ui, state, view, pos)
            }
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                ..
            } if state.pointer.is_some() => {
                move_pointer(ui, state, view, pos);
                if let Some(pointer) = state.pointer.take() {
                    match pointer.action {
                        Action::Add if !pointer.moved && view.rect.contains(pos) => {
                            state.insert(view.world(pos)[0])
                        }
                        Action::Drag(_) => state.remember(pointer.before),
                        _ => {}
                    }
                }
            }
            Event::PointerGone => state.cancel_gesture(),
            _ => {}
        }
    }
}
fn move_pointer(ui: &egui::Ui, state: &mut State, view: &View, pos: Pos2) {
    let Some(pointer) = &mut state.pointer else {
        return;
    };
    pointer.moved |= pointer.origin.distance(pos) > 4.;
    if !pointer.moved {
        return;
    }
    let source_view = View {
        rect: view.rect,
        bounds: pointer.view,
    };
    let origin = source_view.world(pointer.origin);
    let target = source_view.world(pos);
    let result = match pointer.action {
        Action::Drag(Hit::Node(index)) => {
            if !pointer.before.selected.contains(&index) {
                return;
            }
            pointer.before.curve.move_nodes(
                &pointer.before.selected.iter().copied().collect::<Vec<_>>(),
                target[0] - origin[0],
                target[1] - origin[1],
            )
        }
        Action::Drag(Hit::Handle { index, outgoing }) => {
            // Preserve the grab offset, including a finger landing beside a handle.
            let Ok(material) = pointer.before.curve.materialized() else {
                return;
            };
            let node = &material.nodes[index];
            let Some(handle) = (if outgoing {
                node.outgoing
            } else {
                node.incoming
            }) else {
                return;
            };
            pointer.before.curve.move_handle(
                index,
                outgoing,
                [
                    handle[0] + target[0] - origin[0],
                    handle[1] + target[1] - origin[1],
                ],
                ui.input(|i| i.modifiers.alt),
            )
        }
        Action::Pan | Action::Add => {
            state.view.lo = [
                pointer.view.lo[0] - target[0] + origin[0],
                pointer.view.lo[1] - target[1] + origin[1],
            ];
            state.view.clamp();
            return;
        }
    };
    match result {
        Ok(curve) => state.draft.curve = curve,
        Err(error) => state.error = Some(error.to_string()),
    }
}
fn zoom_pan(
    bounds: &mut Bounds,
    rect: egui::Rect,
    before: Bounds,
    from: Pos2,
    to: Pos2,
    scale: f64,
) {
    let view = View {
        rect,
        bounds: before,
    };
    let anchor = view.world(from);
    bounds.span = [before.span[0] * scale, before.span[1] * scale];
    bounds.clamp();
    bounds.lo = [
        anchor[0] - f64::from((to.x - rect.left()) / rect.width()) * bounds.span[0],
        anchor[1] - f64::from((rect.bottom() - to.y) / rect.height()) * bounds.span[1],
    ];
    bounds.clamp();
}
