//! Authoring geometry for normalized, reusable multi-node easing curves.
//! Playback remains ordinary RPE events evaluated by the original prpr engine.
mod authoring;
mod bake;
mod binding;
mod edit;
mod geometry;
mod library;
pub mod projection;
use crate::ChartError;
pub use authoring::{Edit, RECIPE_KEY, Snapshot, Source};
pub use binding::{Binding, CURVE_KEY};
pub use geometry::{Geometry, Segment};
pub use library::{CurveLibrary, Template};
use serde::{Deserialize, Serialize};

/// Maximum geometric error in event-value units before native f32/time rounding.
pub const BAKE_TOLERANCE: f64 = 0.001;
pub const MAX_NODES: usize = 256;
pub const MAX_VALUE: f64 = 10_000.;
const GAP: f64 = 1e-6;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Auto,
    Smooth,
    #[default]
    Corner,
    Linear,
    Hold,
}
impl Mode {
    pub const ALL: [Self; 5] = [
        Self::Auto,
        Self::Smooth,
        Self::Corner,
        Self::Linear,
        Self::Hold,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::Smooth => "Smooth",
            Self::Corner => "Corner",
            Self::Linear => "Linear",
            Self::Hold => "Hold value",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default, rename = "in", skip_serializing_if = "Option::is_none")]
    pub incoming: Option<[f64; 2]>,
    #[serde(default, rename = "out", skip_serializing_if = "Option::is_none")]
    pub outgoing: Option<[f64; 2]>,
}
impl Node {
    pub fn new(x: f64, y: f64, mode: Mode) -> Self {
        Self {
            x,
            y,
            mode,
            incoming: None,
            outgoing: None,
        }
    }
    pub fn point(&self) -> [f64; 2] {
        [self.x, self.y]
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    pub version: u32,
    #[serde(default)]
    pub name: String,
    pub nodes: Vec<Node>,
}
impl Default for Curve {
    fn default() -> Self {
        Self {
            version: 1,
            name: String::new(),
            nodes: vec![
                Node::new(0., 0., Mode::Linear),
                Node::new(1., 1., Mode::Linear),
            ],
        }
    }
}
fn invalid(message: &str) -> ChartError {
    ChartError::Invalid(message.into())
}
impl Curve {
    pub fn validate(&self) -> Result<(), ChartError> {
        if self.version != 1
            || !(2..=MAX_NODES).contains(&self.nodes.len())
            || self.name.chars().count() > 160
        {
            return Err(invalid(
                "An easing curve needs version 1, 2–256 nodes and a name up to 160 characters",
            ));
        }
        if self.nodes[0].point() != [0., 0.] || self.nodes.last().unwrap().point() != [1., 1.] {
            return Err(invalid("Easing endpoints must stay at (0, 0) and (1, 1)"));
        }
        for (i, n) in self.nodes.iter().enumerate() {
            if !n.x.is_finite()
                || !n.y.is_finite()
                || n.y.abs() > MAX_VALUE
                || (i > 0 && n.x <= self.nodes[i - 1].x)
            {
                return Err(invalid(
                    "Easing nodes need distinct ordered times and finite values within ±10000",
                ));
            }
            for (handle, lo, hi) in [
                (n.incoming, self.nodes[i.saturating_sub(1)].x, n.x),
                (
                    n.outgoing,
                    n.x,
                    self.nodes[(i + 1).min(self.nodes.len() - 1)].x,
                ),
            ] {
                if handle.is_some_and(|p| {
                    !p[0].is_finite()
                        || !p[1].is_finite()
                        || p[1].abs() > MAX_VALUE
                        || p[0] < lo
                        || p[0] > hi
                }) {
                    return Err(invalid(
                        "Easing handles must stay between neighboring node times with finite values within ±10000",
                    ));
                }
            }
        }
        Ok(())
    }
    pub fn segments(&self) -> Result<Vec<Segment>, ChartError> {
        self.validate()?;
        Ok((0..self.nodes.len() - 1).map(|i| self.segment(i)).collect())
    }
    fn segment(&self, i: usize) -> Segment {
        let a = &self.nodes[i];
        let b = &self.nodes[i + 1];
        let dx = b.x - a.x;
        let handle = |index: usize, outgoing: bool| {
            let n = &self.nodes[index];
            if n.mode != Mode::Auto
                && let Some(p) = if outgoing { n.outgoing } else { n.incoming }
            {
                return p;
            }
            let before = &self.nodes[index.saturating_sub(1)];
            let after = &self.nodes[(index + 1).min(self.nodes.len() - 1)];
            let slope = (after.y - before.y) / (after.x - before.x);
            let length = if outgoing { dx / 3. } else { -dx / 3. };
            [
                n.x + length,
                (n.y + length * slope).clamp(-MAX_VALUE, MAX_VALUE),
            ]
        };
        let points = match a.mode {
            Mode::Hold => [a.point(), a.point(), [b.x, a.y], [b.x, a.y]],
            Mode::Linear => [
                a.point(),
                [a.x + dx / 3., a.y + (b.y - a.y) / 3.],
                [b.x - dx / 3., b.y - (b.y - a.y) / 3.],
                b.point(),
            ],
            _ => [a.point(), handle(i, true), handle(i + 1, false), b.point()],
        };
        Segment {
            points,
            hold: a.mode == Mode::Hold,
            linear: a.mode == Mode::Linear,
        }
    }
}
