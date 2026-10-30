//! Editor recipes materialize ordinary RPE events; no alternate playback engine.
mod generate;
pub use generate::{detach, prepare, recipes};

mod evaluate;
use crate::{Beat, ChartDocument, ChartError, Editor, EventType, Formula, LineEvent, tools};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::f64::consts::{PI, TAU};

pub const RECIPE_FIELD: &str = "phiStudioTrajectories";
fn invalid(message: &str) -> ChartError {
    ChartError::Invalid(message.into())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shape {
    Line,
    Arc,
    #[default]
    Circle,
    Ellipse,
    Wave,
    Spiral,
    LogSpiral,
    Rose,
    Lissajous,
    Lemniscate,
    Heart,
    Cardioid,
    Astroid,
    Deltoid,
    Nephroid,
    Cycloid,
    Epicycloid,
    Hypocycloid,
    Epitrochoid,
    Hypotrochoid,
    Superellipse,
    Triangle,
    Square,
    Pentagon,
    Hexagon,
    Octagon,
    Star,
    Rectangle,
    Diamond,
    Zigzag,
    RandomLoop,
    Butterfly,
    Formula,
    Polar,
}
impl Shape {
    pub const ALL: [Self; 34] = [
        Self::Line,
        Self::Arc,
        Self::Circle,
        Self::Ellipse,
        Self::Wave,
        Self::Spiral,
        Self::LogSpiral,
        Self::Rose,
        Self::Lissajous,
        Self::Lemniscate,
        Self::Heart,
        Self::Cardioid,
        Self::Astroid,
        Self::Deltoid,
        Self::Nephroid,
        Self::Cycloid,
        Self::Epicycloid,
        Self::Hypocycloid,
        Self::Epitrochoid,
        Self::Hypotrochoid,
        Self::Superellipse,
        Self::Triangle,
        Self::Square,
        Self::Pentagon,
        Self::Hexagon,
        Self::Octagon,
        Self::Star,
        Self::Rectangle,
        Self::Diamond,
        Self::Zigzag,
        Self::RandomLoop,
        Self::Butterfly,
        Self::Formula,
        Self::Polar,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Line => "Line",
            Self::Arc => "Arc",
            Self::Circle => "Circle",
            Self::Ellipse => "Ellipse",
            Self::Wave => "Wave",
            Self::Spiral => "Spiral",
            Self::LogSpiral => "Logarithmic spiral",
            Self::Rose => "Rose",
            Self::Lissajous => "Lissajous",
            Self::Lemniscate => "Lemniscate",
            Self::Heart => "Heart",
            Self::Cardioid => "Cardioid",
            Self::Astroid => "Astroid",
            Self::Deltoid => "Deltoid",
            Self::Nephroid => "Nephroid",
            Self::Cycloid => "Cycloid",
            Self::Epicycloid => "Epicycloid",
            Self::Hypocycloid => "Hypocycloid",
            Self::Epitrochoid => "Epitrochoid",
            Self::Hypotrochoid => "Hypotrochoid",
            Self::Superellipse => "Superellipse",
            Self::Triangle => "Triangle",
            Self::Square => "Square",
            Self::Pentagon => "Pentagon",
            Self::Hexagon => "Hexagon",
            Self::Octagon => "Octagon",
            Self::Star => "Star",
            Self::Rectangle => "Rectangle",
            Self::Diamond => "Diamond",
            Self::Zigzag => "Zigzag",
            Self::RandomLoop => "Seeded loop",
            Self::Butterfly => "Butterfly",
            Self::Formula => "X(t), Y(t) expressions",
            Self::Polar => "Polar expressions",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spec {
    pub shape: Shape,
    pub start: Beat,
    pub end: Beat,
    pub step: Beat,
    pub center: [f64; 2],
    pub radius: [f64; 2],
    pub scale: [f64; 2],
    pub rotation: f64,
    pub turns: f64,
    pub frequency: [f64; 2],
    pub phase: f64,
    pub trim: [f64; 2],
    pub parameter: [f64; 2],
    pub tangent: bool,
    pub expressions: [String; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polar_expressions: Option<[String; 2]>,
    pub easing: [i32; 2],
    pub seed: u32,
    pub simplify: bool,
    pub tolerance: [f64; 3],
}
impl Default for Spec {
    fn default() -> Self {
        Self {
            shape: Shape::Circle,
            start: Beat::ZERO,
            end: Beat::new(4, 0, 1).unwrap(),
            step: Beat::new(0, 1, 16).unwrap(),
            center: [0.; 2],
            radius: [300., 200.],
            scale: [1.; 2],
            rotation: 0.,
            turns: 1.,
            frequency: [3., 2.],
            phase: 0.,
            trim: [0., 1.],
            parameter: [0., 1.],
            tangent: false,
            expressions: ["300*cos(2*pi*t)".into(), "200*sin(2*pi*t)".into()],
            polar_expressions: None,
            easing: [1, 1],
            seed: 1,
            simplify: true,
            tolerance: [0.5, 0.5, 0.25],
        }
    }
}
pub struct Compiled {
    spec: Spec,
    formulas: Option<[Formula; 2]>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Generated {
    pub kind: EventType,
    pub value: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub name: String,
    pub layer: usize,
    pub spec: Spec,
    pub generated: Vec<Generated>,
}

pub struct Edit {
    pub(crate) line: usize,
    pub(crate) before: Option<Value>,
    pub(crate) after: Option<Value>,
    pub(crate) tool: Option<tools::ToolEdit>,
    points: Vec<[f64; 2]>,
}
impl Edit {
    pub fn is_noop(&self) -> bool {
        self.before == self.after && self.tool.is_none()
    }
    pub fn points(&self) -> &[[f64; 2]] {
        &self.points
    }
    pub fn apply(self, editor: &mut Editor) -> Result<tools::Selection, ChartError> {
        editor.execute_trajectory(self)
    }
}
