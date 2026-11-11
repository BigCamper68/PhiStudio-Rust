//! A logical event retains its full curve when cropped, reversed or rebased.
use super::*;
use serde_json::{Value, json};

pub const CURVE_KEY: &str = "phiStudioCustomEasing";
fn full_domain() -> [f64; 2] {
    [0., 1.]
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub curve: Curve,
    #[serde(default = "full_domain")]
    pub domain: [f64; 2],
    /// Affine source endpoints before cropping. Values retain their owner's type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<[Value; 2]>,
}
impl Binding {
    pub fn new(curve: Curve) -> Self {
        Self {
            curve,
            domain: full_domain(),
            source: None,
        }
    }
    pub fn validate(&self) -> Result<(), ChartError> {
        self.curve.validate()?;
        if self
            .domain
            .iter()
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            || self.domain[0] == self.domain[1]
            || (self.source.is_none() && self.domain != full_domain())
        {
            return Err(invalid(
                "Custom easing needs a distinct source domain inside 0–1",
            ));
        }
        if let Some([a, b]) = &self.source {
            let valid = |v: &Value| {
                v.as_f64()
                    .is_some_and(|n| n.is_finite() && (n as f32).is_finite())
                    || v.as_str().is_some_and(|s| s.len() <= 16_384)
                    || v.is_boolean()
                    || v.as_array().is_some_and(|a| {
                        !a.is_empty()
                            && a.len() <= 64
                            && a.iter().all(|v| {
                                v.as_f64()
                                    .is_some_and(|n| n.is_finite() && (n as f32).is_finite())
                            })
                    })
            };
            let same_type = (a.is_number() && b.is_number())
                || (a.is_string() && b.is_string())
                || (a.is_boolean() && b.is_boolean())
                || (a.is_array()
                    && b.is_array()
                    && a.as_array().unwrap().len() == b.as_array().unwrap().len());
            if !valid(a) || !valid(b) || !same_type {
                return Err(invalid(
                    "Custom easing source endpoints must have matching finite value types",
                ));
            }
        }
        Ok(())
    }
    pub fn scalar_source(&self, endpoints: [f64; 2]) -> Result<[f64; 2], ChartError> {
        match &self.source {
            None => Ok(endpoints),
            Some([a, b]) => a
                .as_f64()
                .zip(b.as_f64())
                .map(|(a, b)| [a, b])
                .ok_or_else(|| invalid("This custom easing needs scalar source values")),
        }
    }
    pub fn scalar_value(&self, endpoints: [f64; 2], fraction: f64) -> Result<f64, ChartError> {
        if !fraction.is_finite() {
            return Err(invalid("Custom easing progress must be finite"));
        }
        self.validate()?;
        let [a, b] = self.scalar_source(endpoints)?;
        let x = self.domain[0] + (self.domain[1] - self.domain[0]) * fraction.clamp(0., 1.);
        Ok(a + (b - a)
            * self
                .curve
                .geometry()?
                .sample_in_direction(x, self.domain[1] < self.domain[0]))
    }
    pub fn crop(&self, endpoints: [f64; 2], from: f64, to: f64) -> Result<Self, ChartError> {
        self.validate()?;
        if !from.is_finite() || !to.is_finite() || from < 0. || to > 1. || from >= to {
            return Err(invalid("Invalid custom easing crop"));
        }
        let [a, b] = self.scalar_source(endpoints)?;
        let lo = self.domain[0];
        let span = self.domain[1] - lo;
        let next = Self {
            curve: self.curve.clone(),
            domain: [lo + span * from, lo + span * to],
            source: Some([json!(a), json!(b)]),
        };
        next.validate()?;
        Ok(next)
    }
    /// Called after the event's visible endpoints have been swapped.
    pub fn reverse(&mut self, swapped: [f64; 2]) {
        if self.source.is_none() {
            self.source = Some([json!(swapped[1]), json!(swapped[0])]);
        }
        self.domain.swap(0, 1);
        if self.domain == full_domain() && self.scalar_source(swapped).ok() == Some(swapped) {
            self.source = None;
        }
    }
    pub fn rebase(&self, before: [f64; 2], after: [f64; 2]) -> Result<Self, ChartError> {
        self.validate()?;
        if before == after || self.source.is_none() {
            return Ok(self.clone());
        }
        let geometry = self.curve.geometry()?;
        let reversed = self.domain[1] < self.domain[0];
        let p = geometry.sample_in_direction(self.domain[0], reversed);
        let q = geometry.sample_in_direction(self.domain[1], reversed);
        let [a, b] = self.scalar_source(before)?;
        let source = if (q - p).abs() < 1e-12 {
            if (after[1] - after[0]).abs() > 1e-8 {
                return Err(invalid(
                    "A cropped easing with equal curve endpoints cannot have different event values",
                ));
            }
            let shift = after[0] - (a + (b - a) * p);
            [a + shift, b + shift]
        } else {
            let delta = (after[1] - after[0]) / (q - p);
            let start = after[0] - delta * p;
            [start, start + delta]
        };
        let mut next = self.clone();
        next.source = Some(source.map(|v| json!(v)));
        next.validate()?;
        Ok(next)
    }
    pub fn validate_scalar_endpoints(&self, ends: [f64; 2]) -> Result<(), ChartError> {
        self.validate()?;
        if self.source.is_some() {
            for (i, t) in [0., 1.].into_iter().enumerate() {
                let expected = self.scalar_value(ends, t)?;
                if !expected.is_finite()
                    || (expected - ends[i]).abs() > 1e-7 * expected.abs().max(ends[i].abs()).max(1.)
                {
                    return Err(invalid(
                        "Custom easing source values do not match this event's endpoints",
                    ));
                }
            }
        }
        Ok(())
    }
    pub(crate) fn canonicalize_scalar(&mut self, ends: [f64; 2]) {
        if self.domain == full_domain() && self.scalar_source(ends).ok() == Some(ends) {
            self.source = None;
        }
    }
    pub fn is_scalar_linear(&self, ends: [f64; 2]) -> Result<bool, ChartError> {
        self.validate_scalar_endpoints(ends)?;
        let [a, b] = self.scalar_source(ends)?;
        let [from, to] = self.domain;
        let (lo, hi) = (from.min(to), from.max(to));
        for segment in self.curve.segments()? {
            let left = lo.max(segment.points[0][0]);
            let right = hi.min(segment.points[3][0]);
            if left >= right {
                continue;
            }
            let piece = segment.slice(segment.parameter_at(left), segment.parameter_at(right))?;
            for [x, y] in piece.points {
                let value = a + (b - a) * y;
                let chord = ends[0] + (ends[1] - ends[0]) * (x - from) / (to - from);
                if (value - chord).abs() > 1e-7 {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}
