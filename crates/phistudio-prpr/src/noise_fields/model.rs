//! BlockArea schema and PreviewBlockControl geometry. See SOURCE.json for evidence.
//! Event interpolation is pure in chart seconds so scrubbing never depends on
//! which frames were rendered previously.
use macroquad::prelude::Vec2;
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BlockArea {
    pub top_right_percentage: Point,
    pub bottom_left_percentage: Point,
    pub appear_time: f32,
    pub enable_time: f32,
    pub disable_time: f32,
    pub disappear_time: f32,
    pub is_subtract: bool,
    pub rotate_events: Vec<BlockRotateEvent>,
    pub move_events: Vec<BlockMoveEvent>,
    pub scale_events: Vec<BlockScaleEvent>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockRotateEvent {
    pub anchor: Point,
    pub time: f32,
    pub rotation: f32,
    #[serde(default)]
    pub ease_type: i32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockMoveEvent {
    pub end_position: Point,
    pub time: f32,
    #[serde(default)]
    pub ease_type_x: i32,
    #[serde(default)]
    pub ease_type_y: i32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockScaleEvent {
    pub anchor: Point,
    pub time: f32,
    pub scale: Point,
    #[serde(default)]
    pub ease_type_x: i32,
    #[serde(default)]
    pub ease_type_y: i32,
}

/// Longest easing table index; the original samples `1%` steps.
const EASE_SAMPLES: i32 = 100;
/// `disabledBlockReadyDuration` / `disabledBlockShowDuration`, both fixed at 0.5s.
const BLOCK_FADE_DURATION: f32 = 0.5;

/// Curve of a single `easeType`, before the 101-entry table sampling.
fn ease_raw(n: i32, t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    match n {
        // 13 = Zero: the value stays at the previous event until its own time.
        13 => 0.,
        // 14 = One: the value jumps to the target at this event's time.
        14 => 1.,
        // 1..=12: exponent n = (type - 1) / 3 + 2.
        1..=12 => {
            let group = (n - 1) / 3;
            let kind = (n - 1) % 3;
            let exp = group + 2;
            match kind {
                0 => t.powf(exp as f32),
                1 => 1. - (1. - t).powf(exp as f32),
                _ => {
                    if t < 0.5 {
                        (2. * t).powf(exp as f32) / 2.
                    } else {
                        1. - (2. - 2. * t).powf(exp as f32) / 2.
                    }
                }
            }
        }
        _ => t,
    }
}

/// `GetEase.GetEaseWithProgress`: 101-entry table with linear interpolation.
fn ease(n: i32, t: f32) -> f32 {
    let p = t.clamp(0., 1.) * EASE_SAMPLES as f32;
    let i = p.floor();
    let a = ease_raw(n, i / EASE_SAMPLES as f32);
    if i >= EASE_SAMPLES as f32 {
        return a;
    }
    a + (ease_raw(n, (i + 1.) / EASE_SAMPLES as f32) - a) * (p - i)
}

fn ratio(t: f32, a: f32, b: f32) -> f32 {
    if b <= a {
        1.
    } else {
        ((t - a) / (b - a)).clamp(0., 1.)
    }
}

/// Half of the chart's render height in world units; only the ratio
/// world-size/screen-size matters, so the absolute value is free.
const WORLD_HALF_HEIGHT: f32 = 5.;

/// Percentage coordinates -> world coordinates (isotropic, centre at origin).
pub fn world(p: Point, aspect: f32) -> Vec2 {
    Vec2::new(
        (p.x - 0.5) * 2. * WORLD_HALF_HEIGHT * aspect,
        (p.y - 0.5) * 2. * WORLD_HALF_HEIGHT,
    )
}

fn rotate(p: Vec2, angle: f32) -> Vec2 {
    let (s, c) = angle.sin_cos();
    Vec2::new(c * p.x - s * p.y, s * p.x + c * p.y)
}

fn safe_div(a: f32, b: f32) -> f32 {
    if b.abs() < f32::from_bits(8) {
        1.
    } else {
        a / b
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub center: Vec2,
    pub size: Vec2,
    pub angle: f32,
}

impl Pose {
    pub fn corners(self) -> [Vec2; 4] {
        [
            Vec2::new(-0.5, -0.5),
            Vec2::new(0.5, -0.5),
            Vec2::new(0.5, 0.5),
            Vec2::new(-0.5, 0.5),
        ]
        .map(|p| self.center + rotate(p * self.size, self.angle))
    }
}

impl BlockArea {
    /// Reject non-finite data and sort the renderer copy of the events.
    pub fn normalize(&mut self) -> anyhow::Result<()> {
        anyhow::ensure!(
            [
                self.appear_time,
                self.enable_time,
                self.disable_time,
                self.disappear_time,
                self.top_right_percentage.x,
                self.top_right_percentage.y,
                self.bottom_left_percentage.x,
                self.bottom_left_percentage.y
            ]
            .iter()
            .all(|v| v.is_finite()),
            "block area coordinates and times must be finite"
        );
        for e in &self.move_events {
            anyhow::ensure!(
                [e.time, e.end_position.x, e.end_position.y]
                    .iter()
                    .all(|v| v.is_finite())
                    && (0..=14).contains(&e.ease_type_x)
                    && (0..=14).contains(&e.ease_type_y),
                "block area move event is invalid or uses an unknown ease type"
            );
        }
        for e in &self.scale_events {
            anyhow::ensure!(
                [e.time, e.anchor.x, e.anchor.y, e.scale.x, e.scale.y]
                    .iter()
                    .all(|v| v.is_finite())
                    && (0..=14).contains(&e.ease_type_x)
                    && (0..=14).contains(&e.ease_type_y),
                "block area scale event is invalid or uses an unknown ease type"
            );
        }
        for e in &self.rotate_events {
            anyhow::ensure!(
                [e.time, e.anchor.x, e.anchor.y, e.rotation]
                    .iter()
                    .all(|v| v.is_finite())
                    && (0..=14).contains(&e.ease_type),
                "block area rotate event is invalid or uses an unknown ease type"
            );
        }
        self.move_events.sort_by(|a, b| a.time.total_cmp(&b.time));
        self.scale_events.sort_by(|a, b| a.time.total_cmp(&b.time));
        self.rotate_events.sort_by(|a, b| a.time.total_cmp(&b.time));
        Ok(())
    }

    /// `enableTime <= t < disableTime`.
    pub fn active(&self, t: f32) -> bool {
        t >= self.enable_time && t < self.disable_time
    }

    /// `appearTime <= t < disappearTime`.
    pub fn visible(&self, t: f32) -> bool {
        t >= self.appear_time && t < self.disappear_time
    }

    /// `[enableTime - 0.5, enableTime)`, the `Ready` phase.
    pub fn ready(&self, t: f32) -> bool {
        t >= self.enable_time - BLOCK_FADE_DURATION
            && t < self.enable_time
            && self.appear_time < self.enable_time
    }

    /// Initial show animation. A short pre-enable interval can carry the fade
    /// into Ready or Active, just as DisabledBlockShow continues across layers.
    pub fn fade(&self, t: f32) -> f32 {
        if self.appear_time >= self.enable_time {
            return 1.;
        }
        ((t - self.appear_time) / BLOCK_FADE_DURATION).clamp(0., 1.)
    }

    pub fn pose(&self, t: f32, aspect: f32) -> Pose {
        let lo = world(self.bottom_left_percentage, aspect);
        let hi = world(self.top_right_percentage, aspect);
        let base = (lo + hi) * 0.5;
        let mut center = base;
        let mut scale = Vec2::ONE;
        if let Some(first) = self.scale_events.first().filter(|e| t >= e.time) {
            scale = Vec2::new(first.scale.x, first.scale.y);
            for (i, e) in self.scale_events.iter().enumerate() {
                let Some(next) = self.scale_events.get(i + 1) else {
                    break;
                };
                let p = ratio(t, e.time, next.time);
                let completed = t >= next.time;
                let v = Vec2::new(
                    e.scale.x
                        + (next.scale.x - e.scale.x)
                            * if completed {
                                1.
                            } else {
                                ease(e.ease_type_x, p)
                            },
                    e.scale.y
                        + (next.scale.y - e.scale.y)
                            * if completed {
                                1.
                            } else {
                                ease(e.ease_type_y, p)
                            },
                );
                let anchor = world(e.anchor, aspect);
                let rel = Vec2::new(safe_div(v.x, e.scale.x), safe_div(v.y, e.scale.y));
                center = anchor + (center - anchor) * rel;
                scale = v;
                if t < next.time {
                    break;
                }
            }
        }
        let mut angle = 0.;
        if let Some(first) = self.rotate_events.first().filter(|e| t >= e.time) {
            angle = first.rotation;
            for (i, e) in self.rotate_events.iter().enumerate() {
                let Some(next) = self.rotate_events.get(i + 1) else {
                    break;
                };
                let v = e.rotation
                    + (next.rotation - e.rotation)
                        * if t >= next.time {
                            1.
                        } else {
                            ease(e.ease_type, ratio(t, e.time, next.time))
                        };
                let anchor = world(e.anchor, aspect);
                center = anchor + rotate(center - anchor, (v - e.rotation).to_radians());
                angle = v;
                if t < next.time {
                    break;
                }
            }
        }
        if let Some(first) = self.move_events.first().filter(|e| t >= e.time) {
            let mut v = first.end_position;
            for (i, e) in self.move_events.iter().enumerate() {
                let Some(next) = self.move_events.get(i + 1) else {
                    break;
                };
                let p = ratio(t, e.time, next.time);
                let completed = t >= next.time;
                v = Point {
                    x: e.end_position.x
                        + (next.end_position.x - e.end_position.x)
                            * if completed {
                                1.
                            } else {
                                ease(e.ease_type_x, p)
                            },
                    y: e.end_position.y
                        + (next.end_position.y - e.end_position.y)
                            * if completed {
                                1.
                            } else {
                                ease(e.ease_type_y, p)
                            },
                };
                if t < next.time {
                    break;
                }
            }
            center += world(v, aspect) - base;
        }
        Pose {
            center,
            size: ((hi - lo) * scale).abs(),
            angle: angle.to_radians(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use macroquad::prelude::vec2;
    fn field() -> BlockArea {
        BlockArea {
            bottom_left_percentage: Point { x: 0.2, y: 0.2 },
            top_right_percentage: Point { x: 0.8, y: 0.8 },
            appear_time: 1.,
            enable_time: 2.,
            disable_time: 3.,
            disappear_time: 4.,
            ..Default::default()
        }
    }
    fn close(a: Vec2, b: Vec2) {
        assert!((a - b).length() < 1e-5, "{a:?} != {b:?}");
    }
    #[test]
    fn four_timestamps_and_ready_boundary_match_preview_block_control() {
        let a = field();
        assert!(!a.visible(0.999));
        assert!(a.visible(1.));
        assert_eq!(a.fade(1.), 0.);
        assert_eq!(a.fade(1.25), 0.5);
        assert!(!a.ready(1.499));
        assert!(a.ready(1.5));
        assert!(!a.ready(2.));
        assert!(!a.active(1.999));
        assert!(a.active(2.));
        assert!(!a.active(3.));
        assert!(a.visible(3.999));
        assert_eq!(a.fade(3.5), 1.);
        assert!(!a.visible(4.));
    }
    #[test]
    fn instant_activation_has_no_preview_fade_or_ready_phase() {
        let a = BlockArea {
            appear_time: 2.,
            ..field()
        };
        assert!(!a.visible(1.9));
        assert!(!a.ready(1.9));
        assert!(a.active(2.));
        assert_eq!(a.fade(2.), 1.);
    }
    #[test]
    fn initial_fade_continues_when_ready_or_active_starts_early() {
        let a = BlockArea {
            appear_time: 1.8,
            ..field()
        };
        assert!(a.ready(1.9));
        assert!((a.fade(1.9) - 0.2).abs() < 1e-6);
        assert!(a.active(2.1));
        assert!((a.fade(2.1) - 0.6).abs() < 1e-6);
    }

    #[test]
    fn easing_samples_power_curves_and_step_endpoints() {
        for (n, p) in [(1, 2), (4, 3), (7, 4), (10, 5)] {
            assert!((ease(n, 0.5) - 0.5f32.powi(p)).abs() < 1e-6);
        }
        assert_eq!(ease(13, 1.), 0.);
        assert_eq!(ease(14, 0.), 1.);
        // The table linearly interpolates between 0.37 and 0.38, rather than
        // evaluating the quadratic directly at 0.375.
        assert!((ease(1, 0.375) - (0.37f32.powi(2) + 0.38f32.powi(2)) * 0.5).abs() < 1e-6);
    }
    #[test]
    fn keys_do_not_apply_before_their_first_timestamp() {
        let mut a = field();
        a.move_events = vec![BlockMoveEvent {
            time: 2.,
            end_position: Point { x: 0.8, y: 0.5 },
            ..Default::default()
        }];
        a.scale_events = vec![BlockScaleEvent {
            time: 2.,
            anchor: Point { x: 0.5, y: 0.5 },
            scale: Point { x: 2., y: 3. },
            ..Default::default()
        }];
        a.rotate_events = vec![BlockRotateEvent {
            time: 2.,
            anchor: Point { x: 0.5, y: 0.5 },
            rotation: 90.,
            ..Default::default()
        }];
        close(a.pose(1.5, 1.).center, Vec2::ZERO);
        close(a.pose(1.5, 1.).size, vec2(6., 6.));
        assert_eq!(a.pose(1.5, 1.).angle, 0.);
        close(a.pose(2., 1.).center, vec2(3., 0.));
        close(a.pose(2., 1.).size, vec2(12., 18.));
        assert!((a.pose(2., 1.).angle - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }
    #[test]
    fn pivot_scale_then_pivot_rotation_then_absolute_movement() {
        let mut a = field();
        a.scale_events = vec![
            BlockScaleEvent {
                time: 1.,
                anchor: Point { x: 0., y: 0.5 },
                scale: Point { x: 1., y: 1. },
                ..Default::default()
            },
            BlockScaleEvent {
                time: 3.,
                anchor: Point { x: 0.9, y: 0.9 },
                scale: Point { x: 2., y: 1. },
                ..Default::default()
            },
        ];
        a.rotate_events = vec![
            BlockRotateEvent {
                time: 1.,
                anchor: Point { x: 0.5, y: 0.5 },
                rotation: 0.,
                ..Default::default()
            },
            BlockRotateEvent {
                time: 3.,
                anchor: Point { x: 0.1, y: 0.1 },
                rotation: 90.,
                ..Default::default()
            },
        ];
        close(a.pose(3., 1.).center, vec2(0., 5.));
        close(a.pose(3., 1.).size, vec2(12., 6.));
        a.move_events = vec![BlockMoveEvent {
            time: 1.,
            end_position: Point { x: 0.6, y: 0.5 },
            ..Default::default()
        }];
        close(a.pose(3., 1.).center, vec2(1., 5.));
        let at = a.pose(2., 1.);
        a.pose(3., 1.);
        a.pose(1., 1.);
        let again = a.pose(2., 1.);
        close(at.center, again.center);
        close(at.size, again.size);
        assert_eq!(at.angle, again.angle);
    }
    #[test]
    fn zero_scale_transitions_and_non_square_coordinates_stay_finite() {
        let mut a = field();
        a.scale_events = vec![
            BlockScaleEvent {
                time: 1.,
                anchor: Point { x: 0., y: 0. },
                scale: Point { x: 0., y: 0. },
                ..Default::default()
            },
            BlockScaleEvent {
                time: 2.,
                anchor: Point { x: 0., y: 0. },
                scale: Point { x: 1., y: 1. },
                ..Default::default()
            },
        ];
        for aspect in [0.75, 1., 16. / 9., 2.4] {
            let pose = a.pose(1.5, aspect);
            assert!(pose.center.is_finite());
            assert!(pose.size.is_finite());
            close(pose.size, vec2(3. * aspect, 3.));
        }
    }
}
