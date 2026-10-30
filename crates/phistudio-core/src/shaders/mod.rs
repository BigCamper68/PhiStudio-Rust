//! Phira extra.json is retained verbatim; edits live in the working chart and
//! export as a standard sidecar. History never snapshots the note document.
mod transactions;
pub(crate) use transactions::{prepare, prepare_source};
mod extra;
use extra::validate_authored_effect;
pub use extra::{
    default_shader_extra, load_shader_extra, shader_effects, shader_tempo, validate_shader_extra,
};
mod source;
pub use source::{read_shader_source, shader_parameters, shader_sources, validate_shader_source};
use source::{resource_path, uniform_name};
mod delta;
use crate::{Beat, ChartDocument, ChartError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{fs, io::Read, path::Path};
pub const EXTRA_KEY: &str = "phiStudioExtra";
fn invalid(s: &str) -> ChartError {
    ChartError::Invalid(s.into())
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShaderEffect {
    pub start: Beat,
    pub end: Beat,
    pub shader: String,
    #[serde(default)]
    pub vars: Map<String, Value>,
    #[serde(default)]
    pub global: bool,
}
impl ShaderEffect {
    pub fn new(start: Beat, end: Beat) -> Self {
        Self {
            start,
            end,
            shader: "grayscale".into(),
            vars: Map::new(),
            global: false,
        }
    }
    pub fn validate(&self) -> Result<(), ChartError> {
        if self.end <= self.start
            || self.shader.is_empty()
            || self.shader.len() > 512
            || self.vars.len() > 128
        {
            return Err(invalid("Choose a shader and a positive effect interval"));
        }
        if let Some(path) = self.shader.strip_prefix('/') {
            resource_path(path)?;
        }
        for (name, value) in &self.vars {
            if name.is_empty()
                || name.len() > 128
                || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            {
                return Err(invalid("Use a GLSL uniform name"));
            }
            if let Some(events) = value
                .as_array()
                .filter(|a| a.first().is_some_and(Value::is_object))
            {
                for event in events {
                    let a: Beat = serde_json::from_value(event["startTime"].clone())?;
                    let b: Beat = serde_json::from_value(event["endTime"].clone())?;
                    if b <= a
                        || uniform_type(&event["start"]).is_none()
                        || uniform_type(&event["start"]) != uniform_type(&event["end"])
                    {
                        return Err(invalid("Invalid uniform keyframe interval or value"));
                    }
                }
            } else if !value.is_null()
                && !value.as_array().is_some_and(|v| v.is_empty())
                && uniform_type(value).is_none()
            {
                return Err(invalid(
                    "Shader uniforms support float, vec2 or RGBA values",
                ));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UniformKind {
    Float,
    Vec2,
    Color,
}
pub fn uniform_type(v: &Value) -> Option<UniformKind> {
    if v.as_f64().is_some_and(|n| (n as f32).is_finite()) {
        Some(UniformKind::Float)
    } else if v.as_array().is_some_and(|a| {
        a.len() == 2
            && a.iter()
                .all(|v| v.as_f64().is_some_and(|n| (n as f32).is_finite()))
    }) {
        Some(UniformKind::Vec2)
    } else if v
        .as_array()
        .is_some_and(|a| a.len() == 4 && a.iter().all(|v| v.as_u64().is_some_and(|n| n <= 255)))
    {
        Some(UniformKind::Color)
    } else {
        None
    }
}

/// Working source overrides are saved/recovered with the chart and exported as GLSL.
pub const SOURCES_KEY: &str = "phiStudioShaderSources";
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
pub const PRESETS: [&str; 10] = [
    "chromatic",
    "circleBlur",
    "fisheye",
    "glitch",
    "grayscale",
    "noise",
    "pixel",
    "radialBlur",
    "shockwave",
    "vignette",
];

#[derive(Clone, Debug)]
pub enum ShaderAction {
    Insert { index: usize, effect: Value },
    Update { index: usize, effect: Value },
    Delete(usize),
    Move { from: usize, to: usize },
    Tempo(Value),
}

#[derive(Debug)]
pub(crate) enum ShaderDelta {
    Initialize(Value),
    Effect(crate::array_edit::ArrayEdit),
    Move {
        from: usize,
        to: usize,
    },
    Tempo {
        before: Value,
        after: Value,
    },
    Source {
        path: String,
        before: Option<Value>,
        after: Value,
        container: Option<Value>,
    },
}

#[derive(Clone, Debug)]
pub struct ShaderParameter {
    pub name: String,
    pub default: Value,
}
