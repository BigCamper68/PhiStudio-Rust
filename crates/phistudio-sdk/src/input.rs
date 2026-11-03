use crate::{Viewport, valid_id};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputSurfaceKind {
    Notes,
    Events,
    Storyboard,
    ShaderEffects,
    ShaderParameters,
    NoiseFields,
    NoiseAnimation,
    Preview,
    Track,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolGesture {
    /// A movement beyond the host slop cancels the tap and starts navigation.
    Tap,
    #[default]
    Drag,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputButton {
    Primary,
    Secondary,
    Middle,
    Extra1,
    Extra2,
}
pub fn default_input_buttons() -> Vec<InputButton> {
    vec![InputButton::Primary]
}
pub fn default_input_surfaces() -> Vec<InputSurfaceKind> {
    vec![InputSurfaceKind::Notes]
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputSurface {
    pub kind: InputSurfaceKind,
    /// The owned track ID for Track; None on application surfaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub instance: String,
    pub viewport: Viewport,
    #[serde(default)]
    pub metadata: Value,
}
impl InputSurface {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.instance) {
            return Err("Invalid input instance".into());
        }
        let track = self.kind == InputSurfaceKind::Track;
        if track != self.id.is_some() || self.id.as_ref().is_some_and(|id| !valid_id(id)) {
            return Err("Input track IDs must name an owned track".into());
        }
        let preview = self.kind == InputSurfaceKind::Preview;
        if preview == self.viewport.timeline.is_some() {
            return Err("Invalid input viewport clock".into());
        }
        crate::RenderFrame {
            token: 1,
            instance: self.instance.clone(),
            target: crate::RenderTarget {
                kind: if preview {
                    crate::RenderKind::Preview
                } else {
                    crate::RenderKind::Track
                },
                id: "input".into(),
            },
            viewport: self.viewport.clone(),
        }
        .validate()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerKind {
    Mouse,
    Touch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputPointer {
    pub kind: PointerKind,
    pub device: u64,
    pub id: u64,
    pub button: InputButton,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputModifiers {
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub command: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputPoint {
    pub local: [f64; 2],
    pub normalized: [f64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_beat: Option<f64>,
    /// Exact snapped RPE time, provided by the host without SDK/core coupling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beat: Option<[i32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputPhase {
    Begin,
    Update,
    End,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointerInput {
    pub phase: InputPhase,
    pub position: InputPoint,
    pub modifiers: InputModifiers,
    pub moved: bool,
}

/// A coalesced ordered batch for one pointer and one frozen chart transform.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputBatch {
    pub gesture: u64,
    pub surface: InputSurface,
    pub pointer: InputPointer,
    pub start: InputPoint,
    pub events: Vec<PointerInput>,
}
impl InputBatch {
    pub fn phase(&self) -> Option<InputPhase> {
        self.events.last().map(|e| e.phase)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.gesture == 0 || self.events.is_empty() || self.events.len() > 64 {
            return Err("Invalid gesture ID or input batch size".into());
        }
        self.surface.validate()?;
        validate_point(&self.start)?;
        for (index, event) in self.events.iter().enumerate() {
            validate_point(&event.position)?;
            if (event.phase == InputPhase::Begin && index != 0)
                || (matches!(event.phase, InputPhase::End | InputPhase::Cancel)
                    && index + 1 != self.events.len())
            {
                return Err("Input batch phases are out of order".into());
            }
        }
        Ok(())
    }
}
fn validate_point(p: &InputPoint) -> Result<(), String> {
    if !p
        .local
        .iter()
        .chain(&p.normalized)
        .all(|v| v.is_finite() && v.abs() <= 1e12)
        || p.chart_x
            .into_iter()
            .chain(p.raw_beat)
            .any(|v| !v.is_finite())
        || p.beat.is_some_and(|[_, n, d]| d <= 0 || n < 0 || n >= d)
        || p.pressure
            .is_some_and(|p| !p.is_finite() || !(0. ..=1.).contains(&p))
    {
        return Err("Invalid pointer coordinates, snapped time or pressure".into());
    }
    Ok(())
}
