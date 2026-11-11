//! Persistent authoring preferences, separate from imported chart data.
use crate::{ChartDocument, ChartError, EventType};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LineCreationDefaults {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub alpha: f64,
    pub speed: f64,
}
impl Default for LineCreationDefaults {
    fn default() -> Self {
        Self {
            x: 0.,
            y: 0.,
            rotation: 0.,
            alpha: 255.,
            speed: 10.,
        }
    }
}
impl LineCreationDefaults {
    pub fn normalized(mut self) -> Self {
        let defaults = Self::default();
        for (value, fallback) in [
            (&mut self.x, defaults.x),
            (&mut self.y, defaults.y),
            (&mut self.rotation, defaults.rotation),
            (&mut self.speed, defaults.speed),
        ] {
            *value = if value.is_finite() {
                value.clamp(-1e9, 1e9)
            } else {
                fallback
            };
        }
        self.alpha = if self.alpha.is_finite() {
            self.alpha.clamp(-255., 255.)
        } else {
            defaults.alpha
        };
        self
    }
    pub(crate) fn validate(&self) -> Result<(), ChartError> {
        if [self.x, self.y, self.rotation, self.speed]
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1e9)
            || !self.alpha.is_finite()
            || !(-255. ..=255.).contains(&self.alpha)
        {
            return Err(ChartError::Invalid("Line defaults need finite positions, rotation and speed within ±1e9, and alpha from -255 to 255".into()));
        }
        Ok(())
    }
    pub(crate) fn line(&self, name: &str) -> Result<Value, ChartError> {
        self.validate()?;
        let mut line = ChartDocument::new_line(name);
        for (kind, value) in [
            (EventType::MoveX, self.x),
            (EventType::MoveY, self.y),
            (EventType::Rotate, self.rotation),
            (EventType::Alpha, self.alpha),
            (EventType::Speed, self.speed),
        ] {
            let event = &mut line["eventLayers"][0][kind.key()][0];
            event["start"] = json!(value);
            event["end"] = json!(value);
        }
        Ok(line)
    }
}
