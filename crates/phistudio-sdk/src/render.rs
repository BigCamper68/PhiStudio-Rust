use crate::{DrawPrimitive, valid_id};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderKind {
    Preview,
    Track,
    PreviewEffect,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderTarget {
    pub kind: RenderKind,
    pub id: String,
}

/// The vertical clock matches the editor. baseline and size use local points;
/// x_min/x_max describe the horizontal chart-coordinate range.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineViewport {
    pub beat: f64,
    pub pixels_per_beat: f32,
    pub baseline: f32,
    pub x_min: f64,
    pub x_max: f64,
    pub division: i32,
    pub lanes: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Viewport {
    pub size: [f32; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline: Option<TimelineViewport>,
}

/// Each visible instance has its own token. Rendering is asynchronous and must
/// not edit the document; prepare expensive data in commands/services instead.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderFrame {
    pub token: u64,
    pub instance: String,
    pub target: RenderTarget,
    pub viewport: Viewport,
}
impl RenderFrame {
    pub fn validate(&self) -> Result<(), String> {
        if self.token == 0
            || !valid_id(&self.instance)
            || !valid_id(&self.target.id)
            || !self
                .viewport
                .size
                .iter()
                .all(|v| v.is_finite() && (1. ..=16_384.).contains(v))
        {
            return Err("Invalid render token, instance or viewport size".into());
        }
        if (self.target.kind == RenderKind::Track) != self.viewport.timeline.is_some() {
            return Err("Timeline viewports belong to track contributions".into());
        }
        if let Some(t) = &self.viewport.timeline
            && (!t.beat.is_finite()
                || !t.baseline.is_finite()
                || !t.pixels_per_beat.is_finite()
                || t.pixels_per_beat <= 0.
                || !t.x_min.is_finite()
                || !t.x_max.is_finite()
                || t.x_min >= t.x_max
                || !(1..=1920).contains(&t.division)
                || t.lanes < 2)
        {
            return Err("Invalid timeline render transform".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawSpace {
    #[default]
    Points,
    Normalized,
    /// x is a chart coordinate; y is an absolute beat. Marker radius, stroke
    /// width and font size remain in points in every coordinate space.
    Timeline,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawLayer {
    #[serde(default)]
    pub space: DrawSpace,
    pub primitives: Vec<DrawPrimitive>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DrawColor {
    Rgba([u8; 4]),
    Theme(ThemeColor),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeColor {
    Base,
    Panel,
    Raised,
    Border,
    Muted,
    Text,
    Accent,
    Tap,
    Hold,
    Flick,
    Drag,
}
