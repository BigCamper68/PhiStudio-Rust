use super::{geometry::mix, *};
use std::collections::BTreeSet;

impl Curve {
    /// Freeze automatic tangents before local edits, preserving the current shape.
    pub fn materialized(&self) -> Result<Self, ChartError> {
        let segments = self.segments()?;
        let mut next = self.clone();
        for (i, s) in segments.iter().enumerate() {
            next.nodes[i].outgoing = Some(s.points[1]);
            next.nodes[i + 1].incoming = Some(s.points[2]);
        }
        for n in &mut next.nodes {
            if n.mode == Mode::Auto {
                n.mode = Mode::Smooth;
            }
        }
        Ok(next)
    }
    pub fn insert(&self, x: f64) -> Result<(Self, usize), ChartError> {
        self.validate()?;
        if self.nodes.len() >= MAX_NODES
            || !x.is_finite()
            || x <= 0.
            || x >= 1.
            || self.nodes.iter().any(|n| (n.x - x).abs() < GAP)
        {
            return Err(invalid(
                "Choose a distinct interior time; easing curves support up to 256 nodes",
            ));
        }
        let i = self.nodes.partition_point(|n| n.x < x) - 1;
        let segment = self.segment(i);
        let mut next = self.materialized()?;
        let (left, right) = segment.split(segment.parameter_at(x));
        let point = left.points[3];
        if segment.hold {
            next.nodes.insert(i + 1, Node::new(x, point[1], Mode::Hold));
        } else {
            next.nodes[i].mode = Mode::Corner;
            next.nodes[i].outgoing = Some(left.points[1]);
            next.nodes[i + 1].incoming = Some(right.points[2]);
            next.nodes.insert(
                i + 1,
                Node {
                    x,
                    y: point[1],
                    mode: Mode::Corner,
                    incoming: Some(left.points[2]),
                    outgoing: Some(right.points[1]),
                },
            );
        }
        next.validate()?;
        Ok((next, i + 1))
    }
    pub fn move_nodes(&self, indices: &[usize], dx: f64, dy: f64) -> Result<Self, ChartError> {
        self.validate()?;
        if !dx.is_finite() || !dy.is_finite() {
            return Err(invalid("Easing edits need finite coordinates"));
        }
        let indices: BTreeSet<_> = indices
            .iter()
            .copied()
            .filter(|i| *i > 0 && *i + 1 < self.nodes.len())
            .collect();
        let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
        for &i in &indices {
            if !indices.contains(&(i - 1)) {
                lo = lo.max(self.nodes[i - 1].x + GAP - self.nodes[i].x);
            }
            if !indices.contains(&(i + 1)) {
                hi = hi.min(self.nodes[i + 1].x - GAP - self.nodes[i].x);
            }
        }
        if lo > hi {
            return Err(invalid("Easing nodes are too close to move"));
        }
        let dx = dx.clamp(lo, hi);
        let mut next = self.clone();
        for i in indices {
            let n = &mut next.nodes[i];
            let y = (n.y + dy).clamp(-MAX_VALUE + 1., MAX_VALUE - 1.);
            let delta = y - n.y;
            n.x += dx;
            n.y = y;
            for p in [&mut n.incoming, &mut n.outgoing].into_iter().flatten() {
                p[0] += dx;
                p[1] += delta;
            }
        }
        next.clamp_handles();
        next.validate()?;
        Ok(next)
    }
    pub fn move_handle(
        &self,
        index: usize,
        outgoing: bool,
        point: [f64; 2],
        broken: bool,
    ) -> Result<Self, ChartError> {
        self.validate()?;
        if index >= self.nodes.len()
            || (outgoing && index + 1 == self.nodes.len())
            || (!outgoing && index == 0)
            || point.iter().any(|v| !v.is_finite())
        {
            return Err(invalid(
                "Choose an existing easing handle and finite coordinates",
            ));
        }
        let material = self.materialized()?;
        let mut next = self.clone();
        let n = &mut next.nodes[index];
        n.incoming = n.incoming.or(material.nodes[index].incoming);
        n.outgoing = n.outgoing.or(material.nodes[index].outgoing);
        n.mode = if broken {
            Mode::Corner
        } else if matches!(n.mode, Mode::Auto | Mode::Linear | Mode::Hold) {
            Mode::Smooth
        } else {
            n.mode
        };
        let lo = if outgoing {
            n.x
        } else {
            self.nodes[index - 1].x
        };
        let hi = if outgoing {
            self.nodes[index + 1].x
        } else {
            n.x
        };
        let point = [
            point[0].clamp(lo, hi),
            point[1].clamp(-MAX_VALUE, MAX_VALUE),
        ];
        if outgoing {
            n.outgoing = Some(point);
        } else {
            n.incoming = Some(point);
        }
        let origin = n.point();
        let smooth = n.mode == Mode::Smooth;
        let other = if outgoing {
            &mut n.incoming
        } else {
            &mut n.outgoing
        };
        if smooth && let Some(other) = other {
            let length = (other[0] - origin[0]).hypot(other[1] - origin[1]);
            let delta = [origin[0] - point[0], origin[1] - point[1]];
            let norm = delta[0].hypot(delta[1]);
            if norm > 1e-12 {
                *other = [
                    origin[0] + delta[0] * length / norm,
                    origin[1] + delta[1] * length / norm,
                ];
            }
        }
        if !outgoing && index > 0 && matches!(next.nodes[index - 1].mode, Mode::Linear | Mode::Hold)
        {
            next.nodes[index - 1].mode = Mode::Corner;
            next.nodes[index - 1].outgoing = material.nodes[index - 1].outgoing;
        }
        next.clamp_handles();
        next.validate()?;
        Ok(next)
    }
    pub fn set_mode(&self, indices: &[usize], mode: Mode) -> Result<Self, ChartError> {
        let material = self.materialized()?;
        let mut next = self.clone();
        for &i in indices {
            let Some(n) = next.nodes.get_mut(i) else {
                return Err(invalid("Easing node no longer exists"));
            };
            n.incoming = n.incoming.or(material.nodes[i].incoming);
            n.outgoing = n.outgoing.or(material.nodes[i].outgoing);
            n.mode = mode;
            if mode == Mode::Auto {
                n.incoming = None;
                n.outgoing = None;
            } else if mode == Mode::Smooth
                && let (Some(a), Some(b)) = (n.incoming, n.outgoing)
            {
                let direction = [b[0] - a[0], b[1] - a[1]];
                let norm = direction[0].hypot(direction[1]);
                if norm > 1e-12 {
                    let center = n.point();
                    let left = (a[0] - n.x).hypot(a[1] - n.y) / norm;
                    let right = (b[0] - n.x).hypot(b[1] - n.y) / norm;
                    n.incoming = Some([
                        center[0] - direction[0] * left,
                        center[1] - direction[1] * left,
                    ]);
                    n.outgoing = Some([
                        center[0] + direction[0] * right,
                        center[1] + direction[1] * right,
                    ]);
                }
            }
        }
        next.clamp_handles();
        next.validate()?;
        Ok(next)
    }
    pub fn delete_nodes(&self, indices: &[usize]) -> Result<Self, ChartError> {
        let mut next = self.materialized()?;
        let last = next.nodes.len() - 1;
        next.nodes = next
            .nodes
            .into_iter()
            .enumerate()
            .filter_map(|(i, n)| (i == 0 || i == last || !indices.contains(&i)).then_some(n))
            .collect();
        next.clamp_handles();
        next.validate()?;
        Ok(next)
    }
    fn clamp_handles(&mut self) {
        let times: Vec<_> = self.nodes.iter().map(|n| n.x).collect();
        let count = self.nodes.len();
        for (i, n) in self.nodes.iter_mut().enumerate() {
            let center = n.point();
            for (p, lo, hi) in [
                (&mut n.incoming, times[i.saturating_sub(1)], times[i]),
                (&mut n.outgoing, times[i], times[(i + 1).min(count - 1)]),
            ] {
                if let Some(p) = p {
                    let x = p[0].clamp(lo, hi);
                    let dx = p[0] - center[0];
                    if x != p[0] && x != center[0] && dx.abs() > 1e-14 {
                        p[1] = mix(center, *p, (x - center[0]) / dx)[1];
                    }
                    p[0] = x;
                    p[1] = p[1].clamp(-MAX_VALUE, MAX_VALUE);
                }
            }
        }
    }
}
