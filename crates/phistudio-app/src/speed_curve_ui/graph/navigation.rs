//! Multitouch owns its full lifetime, including Android's primary-pointer cancel.
use super::*;
use egui::{Event, TouchPhase};
use std::collections::BTreeMap;

#[derive(Default)]
pub(in crate::speed_curve_ui) struct Navigation {
    fingers: BTreeMap<u64, Pos2>,
    pinch: Option<Pinch>,
    blocked: bool,
}
struct Pinch {
    center: Pos2,
    gap: f32,
    width: f32,
    start: f64,
    span: f64,
    limit: f64,
}
impl Navigation {
    fn pair(&self) -> Option<(Pos2, Pos2)> {
        let mut fingers = self.fingers.values();
        Some((*fingers.next()?, *fingers.next()?))
    }
}
/// Returns true while a multi-finger sequence consumes the synthetic pointer.
pub(super) fn handle(ui: &egui::Ui, state: &mut State, view: &View) -> bool {
    let mut consumed = state.navigation.blocked;
    let mut end = false;
    for event in ui.input(|i| i.events.clone()) {
        let Event::Touch { id, phase, pos, .. } = event else {
            continue;
        };
        match phase {
            TouchPhase::Start
                if view.rect.contains(pos) && ui.ctx().layer_id_at(pos) == Some(ui.layer_id()) =>
            {
                state.navigation.fingers.insert(id.0, pos);
                if state.navigation.fingers.len() >= 2 {
                    state.cancel();
                    state.navigation.blocked = true;
                    consumed = true;
                    // Additional fingers freeze navigation until a fresh pair;
                    // changing which pair is sampled must not jump the view.
                    state.navigation.pinch = None;
                    if state.navigation.fingers.len() == 2
                        && let Some((a, b)) = state.navigation.pair()
                    {
                        state.navigation.pinch = Some(Pinch {
                            center: a + (b - a) * 0.5,
                            gap: a.distance(b).max(12.),
                            width: (a.x - b.x).abs(),
                            start: state.start,
                            span: state.span,
                            limit: state.limit,
                        });
                    }
                }
            }
            TouchPhase::Move => {
                if let Some(finger) = state.navigation.fingers.get_mut(&id.0) {
                    *finger = pos;
                }
                if let (Some((a, b)), Some(base)) =
                    (state.navigation.pair(), &state.navigation.pinch)
                {
                    let center = a + (b - a) * 0.5;
                    state.limit = base.limit * f64::from(base.gap / a.distance(b).max(12.));
                    let width = (a.x - b.x).abs();
                    if base.width >= 24. && width >= 12. {
                        state.span = base.span * f64::from(base.width / width);
                    }
                    let anchor = base.start
                        + f64::from((base.center.x - view.rect.left()) / view.rect.width())
                            * base.span;
                    state.start = anchor
                        - f64::from((center.x - view.rect.left()) / view.rect.width()) * state.span;
                    state.clamp_view();
                }
            }
            TouchPhase::End | TouchPhase::Cancel => {
                state.navigation.fingers.remove(&id.0);
                if state.navigation.fingers.len() < 2 {
                    state.navigation.pinch = None;
                }
                if state.navigation.fingers.is_empty() {
                    consumed |= state.navigation.blocked;
                    end |= state.navigation.blocked;
                    state.navigation.blocked = false;
                }
            }
            _ => {}
        }
    }
    if end {
        state.load_visible_range();
    }
    consumed
}
