//! Retain the original parser's global terminal clock without scanning charts in the panel.
use phistudio_core::{Beat, ChartDocument};
use serde_json::Value;
pub(crate) fn native_horizon(document: &ChartDocument) -> f64 {
    let mut last = Beat::ZERO;
    let mut visit = |values: &Value| {
        if let Some(values) = values.as_array() {
            for value in values {
                if let Some(end) = value
                    .get("endTime")
                    .and_then(|v| serde_json::from_value::<Beat>(v.clone()).ok())
                {
                    last = last.max(end);
                }
            }
        }
    };
    if let Some(lines) = document.as_json()["judgeLineList"].as_array() {
        for line in lines {
            visit(&line["notes"]);
            if let Some(layers) = line["eventLayers"].as_array() {
                for layer in layers {
                    for key in ["alphaEvents", "moveXEvents", "moveYEvents", "rotateEvents"] {
                        visit(&layer[key]);
                    }
                }
            }
            for key in ["scaleXEvents", "scaleYEvents", "textEvents"] {
                visit(&line["extended"][key]);
            }
        }
    }
    document.tempo().seconds_at(last.as_f64()).max(0.) + 1.
}
impl crate::Preview {
    pub fn speed_curve_horizon(&self) -> f64 {
        self.speed_curve_horizon
    }
}
