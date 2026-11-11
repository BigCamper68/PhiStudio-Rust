use super::*;

pub struct Geometry {
    pub segments: Vec<Segment>,
}
impl Geometry {
    pub fn sample(&self, x: f64) -> f64 {
        self.sample_in_direction(x, false)
    }
    /// Reverse traversal takes the left limit at jumps so animation time
    /// retains the native right-continuous boundary convention.
    pub fn sample_in_direction(&self, x: f64, reversed: bool) -> f64 {
        if x <= 0. {
            return 0.;
        }
        if x >= 1. && !reversed {
            return 1.;
        }
        let index = self
            .segments
            .partition_point(|s| {
                if reversed {
                    s.points[3][0] < x
                } else {
                    s.points[3][0] <= x
                }
            })
            .min(self.segments.len() - 1);
        let s = &self.segments[index];
        s.point(s.parameter_at(x))[1]
    }
}
impl Curve {
    pub fn geometry(&self) -> Result<Geometry, ChartError> {
        Ok(Geometry {
            segments: self.segments()?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub points: [[f64; 2]; 4],
    pub hold: bool,
    pub linear: bool,
}
pub(super) fn mix(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}
impl Segment {
    pub fn point(&self, t: f64) -> [f64; 2] {
        let [a, b, c, d] = self.points;
        let ab = mix(a, b, t);
        let bc = mix(b, c, t);
        let cd = mix(c, d, t);
        mix(mix(ab, bc, t), mix(bc, cd, t), t)
    }
    /// A time-monotone cubic is inverted only for authoring operations.
    pub fn parameter_at(&self, x: f64) -> f64 {
        if x <= self.points[0][0] {
            return 0.;
        }
        if x >= self.points[3][0] {
            return 1.;
        }
        let (mut lo, mut hi) = (0., 1.);
        for _ in 0..52 {
            let mid = (lo + hi) * 0.5;
            if self.point(mid)[0] < x {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        (lo + hi) * 0.5
    }
    pub fn split(&self, t: f64) -> (Self, Self) {
        let [a, b, c, d] = self.points;
        let ab = mix(a, b, t);
        let bc = mix(b, c, t);
        let cd = mix(c, d, t);
        let abc = mix(ab, bc, t);
        let bcd = mix(bc, cd, t);
        let center = mix(abc, bcd, t);
        (
            Self {
                points: [a, ab, abc, center],
                ..*self
            },
            Self {
                points: [center, bcd, cd, d],
                ..*self
            },
        )
    }
    pub fn slice(&self, from: f64, to: f64) -> Result<Self, ChartError> {
        if !from.is_finite() || !to.is_finite() || from < 0. || to > 1. || from >= to {
            return Err(invalid("Invalid easing segment slice"));
        }
        let left = if to < 1. { self.split(to).0 } else { *self };
        Ok(if from > 0. {
            left.split(from / to).1
        } else {
            left
        })
    }
    /// Split at vertical extrema so each native Bezier has nondegenerate values.
    pub fn extrema(&self) -> Vec<f64> {
        let [p0, p1, p2, p3] = self.points.map(|p| p[1]);
        let a = -p0 + 3. * p1 - 3. * p2 + p3;
        let b = 2. * (p0 - 2. * p1 + p2);
        let c = p1 - p0;
        let scale = a.abs().max(b.abs()).max(c.abs()).max(1.);
        let mut roots = Vec::new();
        if a.abs() < scale * 1e-14 {
            if b.abs() > scale * 1e-14 {
                roots.push(-c / b);
            }
        } else {
            let discriminant = b * b - 4. * a * c;
            if discriminant >= 0. {
                // This form avoids cancellation when the roots differ greatly.
                let q = -0.5 * (b + discriminant.sqrt().copysign(b));
                if q.abs() > scale * 1e-14 {
                    roots.extend([q / a, c / q]);
                } else {
                    roots.push(-b / (2. * a));
                }
            }
        }
        roots.retain(|t| t.is_finite() && *t > 1e-10 && *t < 1. - 1e-10);
        roots.sort_by(f64::total_cmp);
        roots.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
        roots
    }
    /// The control hull bounds error against the endpoint chord over the whole segment.
    pub fn chord_error(&self) -> f64 {
        if self.hold {
            return 0.;
        }
        let [a, _, _, d] = self.points;
        let dx = d[0] - a[0];
        if dx <= 0. {
            return f64::INFINITY;
        }
        self.points
            .iter()
            .map(|p| (p[1] - a[1] - (d[1] - a[1]) * (p[0] - a[0]) / dx).abs())
            .fold(0., f64::max)
    }
}
