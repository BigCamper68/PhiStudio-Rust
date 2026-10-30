//! Touch-to-egui bridge. A second finger cancels the pointer and never becomes
//! a new click when the first finger is lifted. All fingers also reach the grid.
use egui::{Event, Modifiers, PointerButton, Pos2, TouchDeviceId, TouchId, TouchPhase};
use std::collections::{BTreeMap, VecDeque};

#[derive(Default)]
pub struct TouchRouter {
    fingers: BTreeMap<u64, Pos2>,
    primary: Option<u64>,
    blocked: bool,
    panel: Option<PanelGesture>,
    momentum: Option<Momentum>,
    clear_hover: bool,
}

struct PanelGesture {
    id: u64,
    zone: crate::scroll::Zone,
    origin: Pos2,
    last: Pos2,
    time: f64,
    motion: MotionHistory,
    scrolling: bool,
}
/// Fit a short motion window instead of amplifying the final sample's noise.
struct MotionHistory(VecDeque<(f64, f32)>);
impl MotionHistory {
    fn new(time: f64, y: f32) -> Self {
        Self(VecDeque::from([(time, y)]))
    }
    fn push(&mut self, time: f64, y: f32) {
        if self.0.back().is_some_and(|(last, _)| time <= *last) {
            return;
        }
        self.0.push_back((time, y));
        // Retain one sample before the cutoff to avoid fitting only two tiny
        // release movements after a sparse, fast Android event batch.
        while self.0.len() > 2 && (self.0[1].0 < time - 0.1 || self.0.len() > 32) {
            self.0.pop_front();
        }
    }
    fn velocity(&self) -> f32 {
        let origin = self.0[0].0;
        if self.0.back().unwrap().0 - origin < 0.01 {
            return 0.;
        }
        let count = self.0.len() as f64;
        let mean_t = self.0.iter().map(|(t, _)| t - origin).sum::<f64>() / count;
        let mean_y = self.0.iter().map(|(_, y)| *y as f64).sum::<f64>() / count;
        let mut numerator = 0.;
        let mut denominator = 0.;
        for (t, y) in &self.0 {
            let dt = t - origin - mean_t;
            numerator += dt * (*y as f64 - mean_y);
            denominator += dt * dt;
        }
        (numerator / denominator).clamp(-2500., 2500.) as f32
    }
}
struct Momentum {
    zone: crate::scroll::Zone,
    anchor: Pos2,
    velocity: f32,
    time: f64,
}

/// Gesture velocity uses event timestamps; inertia advances on the frame clock.
/// Android batches historical events and its uptime has a different epoch.
pub struct TouchTime {
    event: f64,
    frame: f64,
}
impl From<f64> for TouchTime {
    fn from(time: f64) -> Self {
        Self {
            event: time,
            frame: time,
        }
    }
}
impl From<(f64, f64)> for TouchTime {
    fn from((event, frame): (f64, f64)) -> Self {
        Self {
            event: if event.is_finite() { event } else { frame },
            frame,
        }
    }
}

impl TouchRouter {
    pub fn event_with_context(
        &mut self,
        ctx: &egui::Context,
        phase: TouchPhase,
        id: u64,
        pos: Pos2,
        time: impl Into<TouchTime>,
    ) -> (Vec<Event>, bool) {
        let time = time.into();
        let now = time.event;
        if phase == TouchPhase::Start {
            self.momentum = None;
            self.clear_hover = false;
            if self.fingers.is_empty() && !self.blocked {
                self.panel = crate::scroll::at(ctx, pos).map(|zone| PanelGesture {
                    id,
                    zone,
                    origin: pos,
                    last: pos,
                    time: now,
                    motion: MotionHistory::new(now, pos[zone.axis]),
                    scrolling: false,
                });
            }
        }
        let panel = self.panel.take();
        let (raw, canceled) = self.event(phase, id, pos);
        let Some(mut panel) = panel else {
            return (raw, canceled);
        };
        if panel.id != id || canceled {
            // The original router cancels all pointer ownership on multitouch.
            return (raw, canceled);
        }
        let mut events: Vec<_> = raw
            .into_iter()
            .filter(|e| matches!(e, Event::Touch { .. }))
            .collect();
        match phase {
            TouchPhase::Start => events.push(Event::PointerMoved(panel.origin)),
            TouchPhase::Move | TouchPhase::End => {
                let movement = pos - panel.origin;
                if !panel.scrolling && movement.length() > 8. {
                    if movement[panel.zone.axis].abs() >= movement[1 - panel.zone.axis].abs() {
                        panel.scrolling = true;
                    } else {
                        // Horizontal motion still edits sliders/numbers. A tap
                        // reaches its control only after intent is decided.
                        events.push(Event::PointerMoved(panel.origin));
                        events.push(button(panel.origin, true));
                        events.push(Event::PointerMoved(pos));
                        if phase == TouchPhase::End {
                            events.push(button(pos, false));
                            events.push(Event::PointerGone);
                        }
                        return (events, false);
                    }
                }
                if panel.scrolling {
                    let delta = pos[panel.zone.axis] - panel.last[panel.zone.axis];
                    // UP coordinates often contain a tiny reverse jitter. Apply
                    // them to position, but estimate inertia from MOVE history.
                    if phase == TouchPhase::Move {
                        panel.motion.push(now, pos[panel.zone.axis]);
                    }
                    events.push(Event::PointerGone);
                    if delta != 0. {
                        crate::scroll::move_by(ctx, panel.zone, delta);
                    }
                }
                if phase == TouchPhase::End {
                    if !panel.scrolling {
                        events.push(Event::PointerMoved(panel.origin));
                        events.push(button(panel.origin, true));
                        events.push(button(panel.origin, false));
                    } else if now - panel.time < 0.1 && panel.motion.velocity().abs() > 20. {
                        self.momentum = Some(Momentum {
                            zone: panel.zone,
                            anchor: panel.origin,
                            velocity: panel.motion.velocity(),
                            time: time.frame,
                        });
                    }
                    if panel.scrolling {
                        self.clear_hover = true;
                    } else {
                        events.push(Event::PointerGone);
                    }
                }
            }
            TouchPhase::Cancel => events.push(Event::PointerGone),
        }
        if phase != TouchPhase::End && phase != TouchPhase::Cancel {
            panel.last = pos;
            panel.time = now;
            self.panel = Some(panel);
        }
        (events, false)
    }
    pub fn scroll_momentum(&mut self, ctx: &egui::Context, now: f64) -> Vec<Event> {
        let Some(mut motion) = self.momentum.take() else {
            return Vec::new();
        };
        if !self.fingers.is_empty() || !crate::scroll::exists(ctx, motion.zone, motion.anchor) {
            return vec![Event::PointerGone];
        }
        let dt = (now - motion.time).clamp(0., 0.05) as f32;
        if dt == 0. {
            self.momentum = Some(motion);
            return Vec::new();
        }
        let delta = motion.velocity * dt;
        if !crate::scroll::move_by(ctx, motion.zone, delta) {
            self.clear_hover = true;
            return vec![Event::PointerGone];
        }
        motion.velocity = motion.velocity.signum() * (motion.velocity.abs() - 2200. * dt).max(0.);
        motion.time = now;
        let events = vec![Event::PointerGone];
        if motion.velocity.abs() > 20. {
            self.momentum = Some(motion);
        } else {
            self.clear_hover = true;
        }
        events
    }
    pub fn frame_start(&mut self) -> Vec<Event> {
        if self.clear_hover && self.momentum.is_none() {
            self.clear_hover = false;
            vec![Event::PointerGone]
        } else {
            Vec::new()
        }
    }
    pub fn stop_momentum(&mut self) {
        self.momentum = None;
    }
    pub fn event(&mut self, phase: TouchPhase, id: u64, pos: Pos2) -> (Vec<Event>, bool) {
        let mut events = vec![Event::Touch {
            device_id: TouchDeviceId(0),
            id: TouchId(id),
            phase,
            pos,
            force: 0.,
        }];
        let mut canceled = false;
        match phase {
            TouchPhase::Start => {
                self.fingers.insert(id, pos);
                if self.fingers.len() == 1 && !self.blocked {
                    self.primary = Some(id);
                    events.push(Event::PointerMoved(pos));
                    events.push(button(pos, true));
                } else if !self.blocked {
                    self.blocked = true;
                    self.primary = None;
                    cancel_pointer(&mut events);
                    canceled = true;
                }
            }
            TouchPhase::Move => {
                if let Some(finger) = self.fingers.get_mut(&id) {
                    *finger = pos;
                    if self.primary == Some(id) && !self.blocked {
                        events.push(Event::PointerMoved(pos));
                    }
                }
            }
            TouchPhase::End | TouchPhase::Cancel => {
                if self.fingers.remove(&id).is_some() && self.primary == Some(id) {
                    if phase == TouchPhase::Cancel {
                        cancel_pointer(&mut events);
                        canceled = true;
                    } else {
                        events.push(Event::PointerMoved(pos));
                        events.push(button(pos, false));
                        events.push(Event::PointerGone);
                    }
                    self.primary = None;
                }
                if self.fingers.is_empty() {
                    self.blocked = false;
                }
            }
        }
        (events, canceled)
    }

    pub fn cancel(&mut self) -> Vec<Event> {
        self.panel = None;
        self.momentum = None;
        self.clear_hover = false;
        let fingers = std::mem::take(&mut self.fingers);
        let mut events = Vec::new();
        for (id, pos) in fingers {
            events.extend(self.event(TouchPhase::Cancel, id, pos).0);
        }
        cancel_pointer(&mut events);
        self.primary = None;
        self.blocked = false;
        events
    }
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn cancel_pointer(events: &mut Vec<Event>) {
    // egui 0.21 has no PointerCancel. Move away before release so a canceled
    // touch cannot click its original button, slider or text field.
    let outside = egui::pos2(-1_000_000., -1_000_000.);
    events.push(Event::PointerMoved(outside));
    events.push(button(outside, false));
    events.push(Event::PointerGone);
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod horizontal_scroll_tests;
