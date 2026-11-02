//! Build 160 JudgeControl block tests and FingerManagement's per-finger latch.
#[cfg(test)]
use super::model::BlockArea;
use super::model::Pose;
use macroquad::prelude::{Touch, TouchPhase, Vec2, vec2};
use std::collections::HashSet;

pub(super) const MAX_TOUCHES: usize = 10;
const INSET: f32 = 10. * 0.05; // JudgeControl prefab: 5% of world screen height.
const HOVER_DURATION: f32 = 0.1;

#[derive(Clone, Copy, Default)]
pub(super) struct Hover {
    id: Option<u64>,
    pub position: Vec2,
    pub scale: f32,
    from: f32,
    target: f32,
    elapsed: f32,
}
impl Hover {
    fn set_visible(&mut self, visible: bool) {
        let target = if visible { 11.5 } else { 0. };
        if self.target != target {
            // Show always starts at Vector3.zero; Hide starts at current scale.
            self.from = if visible { 0. } else { self.scale };
            if visible {
                self.scale = 0.;
            }
            self.target = target;
            self.elapsed = 0.;
        }
    }
    fn advance(&mut self, dt: f32) {
        self.elapsed = (self.elapsed + dt.max(0.)).min(HOVER_DURATION);
        self.scale = self.from + (self.target - self.from) * (self.elapsed / HOVER_DURATION);
        if self.target == 0. && self.elapsed >= HOVER_DURATION {
            self.id = None;
        }
    }
}

#[derive(Default)]
pub(super) struct Input {
    blocked: HashSet<u64>,
    suppressed: HashSet<u64>,
    pub positions: Vec<Vec2>,
    pub hovers: [Hover; MAX_TOUCHES],
}
impl Input {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn suppressed(&self, id: u64) -> bool {
        self.suppressed.contains(&id)
    }
    #[cfg(test)]
    pub fn filter(
        &mut self,
        touches: &mut Vec<Touch>,
        areas: &[BlockArea],
        t: f32,
        aspect: f32,
        dt: f32,
    ) {
        let samples: Vec<_> = areas
            .iter()
            .filter(|a| a.visible(t))
            .map(|a| phistudio_core::noise_fields::Sample {
                pose: a.pose(t, aspect),
                y_scale: 1.,
                phase: if a.active(t) { 2 } else { 0 },
                opacity: 1.,
                subtract: a.is_subtract,
                color: [255, 84, 84],
            })
            .collect();
        self.filter_samples(touches, &samples, aspect, dt, false);
    }
    pub fn filter_samples(
        &mut self,
        touches: &mut Vec<Touch>,
        samples: &[phistudio_core::noise_fields::Sample],
        aspect: f32,
        dt: f32,
        single: bool,
    ) {
        let fields: Vec<_> = samples.iter().filter(|a| a.phase == 2).collect();
        self.suppressed.clear();
        self.positions.clear();
        for hover in &mut self.hovers {
            hover.advance(dt);
        }
        let mut visible_ids = Vec::new();
        touches.retain(|touch| {
            let ended = matches!(touch.phase, TouchPhase::Ended | TouchPhase::Cancelled);
            if touch.phase == TouchPhase::Started {
                // Native finger IDs can be reused after release or cancellation.
                self.blocked.remove(&touch.id);
            }
            let position = touch_world(touch.position, aspect);
            let blocked = self.blocked.contains(&touch.id)
                || (!ended && position.is_finite() && contains_samples(&fields, position, single));
            if blocked {
                self.suppressed.insert(touch.id);
                if !ended {
                    self.blocked.insert(touch.id);
                    visible_ids.push(touch.id);
                    if self.positions.len() < MAX_TOUCHES {
                        self.positions.push(position);
                    }
                    let slot = self
                        .hovers
                        .iter()
                        .position(|h| h.id == Some(touch.id))
                        .or_else(|| self.hovers.iter().position(|h| h.id.is_none()));
                    if let Some(slot) = slot {
                        let hover = &mut self.hovers[slot];
                        hover.id = Some(touch.id);
                        hover.position = position;
                        hover.set_visible(true);
                    }
                }
            }
            !blocked
        });
        // Includes release, cancellation and a lost native contact on suspend.
        self.blocked.retain(|id| visible_ids.contains(id));
        for hover in &mut self.hovers {
            if !hover.id.is_some_and(|id| visible_ids.contains(&id)) {
                hover.set_visible(false);
                // EndTouchBlockFrame releases the slot while Hide still runs.
                hover.id = None;
            }
        }
    }
}

/// Judge already applies the physical viewport, aspect normalization and mirror.
fn touch_world(p: Vec2, aspect: f32) -> Vec2 {
    vec2(p.x, -p.y) * (5. * aspect)
}

fn inside(pose: Pose, p: Vec2, margin: f32) -> bool {
    if pose.size.x < 0.0001 || pose.size.y < 0.0001 {
        return false;
    }
    let d = p - pose.center;
    let (s, c) = pose.angle.sin_cos();
    let local = vec2(c * d.x + s * d.y, -s * d.x + c * d.y).abs();
    let half = (pose.size * 0.5 + Vec2::splat(margin)).max(Vec2::ZERO);
    local.x <= half.x && local.y <= half.y
}

fn contains_samples(
    fields: &[&phistudio_core::noise_fields::Sample],
    p: Vec2,
    single: bool,
) -> bool {
    let (mut normal, mut count, mut inset, mut expanded) = (false, 0, false, 0);
    for s in fields {
        let p = s.pose.center + (p - s.pose.center) / vec2(1., s.y_scale);
        if s.subtract {
            count += usize::from(inside(s.pose, p, 0.));
            expanded += usize::from(inside(s.pose, p, INSET));
        } else {
            normal |= inside(s.pose, p, 0.);
            inset |= inside(s.pose, p, -INSET);
        }
    }
    let parity = |n| if single { n == 1 } else { n % 2 == 1 };
    (normal ^ parity(count)) && (inset ^ parity(expanded))
}
#[cfg(test)]
fn contains(fields: &[(Pose, bool)], p: Vec2) -> bool {
    let (mut normal, mut subtract, mut inset_normal, mut expanded_subtract) =
        (false, false, false, false);
    for &(pose, is_subtract) in fields {
        if is_subtract {
            subtract ^= inside(pose, p, 0.);
            expanded_subtract ^= inside(pose, p, INSET);
        } else {
            normal |= inside(pose, p, 0.);
            inset_normal |= inside(pose, p, -INSET);
        }
    }
    (normal ^ subtract) && (inset_normal ^ expanded_subtract)
}

#[cfg(test)]
mod tests {
    use super::super::model::{BlockRotateEvent, Point};
    use super::*;
    fn area(subtract: bool) -> BlockArea {
        BlockArea {
            appear_time: 0.,
            enable_time: 1.,
            disable_time: 2.,
            disappear_time: 3.,
            bottom_left_percentage: Point { x: 0.2, y: 0.2 },
            top_right_percentage: Point { x: 0.8, y: 0.8 },
            is_subtract: subtract,
            ..Default::default()
        }
    }
    fn touch(id: u64, phase: TouchPhase, p: Vec2) -> Touch {
        Touch {
            id,
            phase,
            position: p,
            time: 0.,
        }
    }
    #[test]
    fn contact_latches_until_release_even_outside_or_after_deactivation() {
        let mut input = Input::default();
        let fields = [area(false)];
        let mut frame = |t, phase, p| {
            let mut touches = vec![touch(7, phase, p)];
            input.filter(&mut touches, &fields, t, 1., 0.02);
            touches.is_empty()
        };
        assert!(!frame(0.9, TouchPhase::Started, Vec2::ZERO));
        assert!(frame(1., TouchPhase::Stationary, Vec2::ZERO));
        assert!(frame(1.5, TouchPhase::Moved, vec2(0.95, 0.95)));
        assert!(frame(2.5, TouchPhase::Stationary, vec2(0.95, 0.95)));
        assert!(frame(2.5, TouchPhase::Ended, vec2(0.95, 0.95)));
        assert!(!frame(2.5, TouchPhase::Started, Vec2::ZERO));
    }
    #[test]
    fn entering_contact_blocks_only_that_finger_and_cancel_or_loss_releases_it() {
        let mut input = Input::default();
        let fields = [area(false)];
        let mut touches = vec![
            touch(1, TouchPhase::Started, Vec2::ZERO),
            touch(2, TouchPhase::Started, vec2(0.9, 0.)),
        ];
        input.filter(&mut touches, &fields, 1.5, 1., 0.02);
        assert_eq!(touches.len(), 1);
        assert_eq!(touches[0].id, 2);
        let mut touches = vec![
            touch(1, TouchPhase::Cancelled, Vec2::ZERO),
            touch(2, TouchPhase::Moved, Vec2::ZERO),
        ];
        input.filter(&mut touches, &fields, 1.5, 1., 0.02);
        assert!(touches.is_empty());
        input.filter(&mut vec![], &fields, 1.5, 1., 0.02);
        let mut touches = vec![touch(2, TouchPhase::Started, vec2(0.9, 0.))];
        input.filter(&mut touches, &fields, 1.5, 1., 0.02);
        assert_eq!(touches.len(), 1);
        input.reset();
        assert!(input.positions.is_empty());
    }
    #[test]
    fn original_and_adjusted_xor_masks_agree_including_subtract_only_regions() {
        let normal = area(false).pose(1.5, 1.);
        let small = Pose {
            size: vec2(2., 2.),
            ..normal
        };
        assert!(contains(&[(normal, false)], vec2(2.49, 0.)));
        assert!(!contains(&[(normal, false)], vec2(2.51, 0.)));
        assert!(!contains(&[(normal, false), (small, true)], vec2(1.2, 0.)));
        assert!(contains(&[(normal, false), (small, true)], vec2(1.51, 0.)));
        assert!(!contains(&[(normal, false), (small, true)], Vec2::ZERO));
        assert!(contains(
            &[(normal, false), (small, true), (small, true)],
            Vec2::ZERO
        ));
        assert!(contains(&[(small, true)], Vec2::ZERO));
        assert!(!contains(&[(small, true)], vec2(1.1, 0.)));
        let thin = Pose {
            size: vec2(0.8, 4.),
            ..normal
        };
        assert!(!contains(&[(thin, false)], vec2(0.1, 0.)));
        assert!(!contains(
            &[(
                Pose {
                    size: Vec2::ZERO,
                    ..normal
                },
                true
            )],
            Vec2::ZERO
        ));
    }
    #[test]
    fn released_hover_slots_can_be_reused_before_hide_finishes() {
        let mut input = Input::default();
        let fields = [area(false)];
        for _ in 0..2 {
            let mut touches = (0..10)
                .map(|id| touch(id, TouchPhase::Stationary, Vec2::ZERO))
                .collect();
            input.filter(&mut touches, &fields, 1.5, 1., 0.1);
        }
        assert!(input.hovers.iter().all(|h| h.scale == 11.5));
        input.filter(&mut vec![], &fields, 1.5, 1., 0.01);
        assert!(
            input
                .hovers
                .iter()
                .all(|h| h.id.is_none() && h.scale == 11.5)
        );
        let mut touches = (10..20)
            .map(|id| touch(id, TouchPhase::Started, Vec2::ZERO))
            .collect();
        input.filter(&mut touches, &fields, 1.5, 1., 0.01);
        assert!(touches.is_empty());
        assert!(input.hovers.iter().all(|h| h.id.is_some() && h.scale == 0.));
    }
    #[test]
    fn animated_rotation_aspect_and_normalized_touch_use_the_same_world() {
        let mut a = area(false);
        a.top_right_percentage = Point { x: 0.9, y: 0.6 };
        a.rotate_events = vec![BlockRotateEvent {
            time: 1.,
            rotation: 45.,
            anchor: Point { x: 0.5, y: 0.5 },
            ..Default::default()
        }];
        for aspect in [0.75, 1., 16. / 9., 2.4] {
            let pose = a.pose(1.5, aspect);
            let normalized = vec2(pose.center.x, -pose.center.y) / (5. * aspect);
            assert!(contains(&[(pose, false)], touch_world(normalized, aspect)));
            assert!(!contains(&[(pose, false)], pose.corners()[0]));
        }
    }
    #[test]
    fn hover_follows_latched_contact_and_shrinks_after_release() {
        let mut input = Input::default();
        let fields = [area(false)];
        for _ in 0..3 {
            input.filter(
                &mut vec![touch(1, TouchPhase::Stationary, Vec2::ZERO)],
                &fields,
                1.5,
                1.,
                0.05,
            );
        }
        assert_eq!(input.hovers[0].scale, 11.5);
        input.filter(
            &mut vec![touch(1, TouchPhase::Moved, vec2(0.9, 0.))],
            &fields,
            2.5,
            1.,
            0.05,
        );
        assert_eq!(input.positions, vec![vec2(4.5, 0.)]);
        input.filter(&mut vec![], &fields, 2.5, 1., 0.05);
        assert_eq!(input.hovers[0].scale, 11.5);
        input.filter(&mut vec![], &fields, 2.5, 1., 0.05);
        assert_eq!(input.hovers[0].scale, 5.75);
        input.filter(&mut vec![], &fields, 2.5, 1., 0.05);
        assert_eq!(input.hovers[0].scale, 0.);
    }
}
