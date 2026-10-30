use crate::ChartDocument;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Package display/playback options are separate from authored RPE META.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectOptions {
    pub music: Option<String>,
    pub illustration: Option<String>,
    pub display: BTreeMap<String, String>,
    pub authored: BTreeMap<String, String>,
    pub manifest_offset_ms: i64,
    pub background_dim: Option<f64>,
    pub use_rpe_170_speed: bool,
    pub note_uniform_scale: bool,
    pub hold_partial_cover: bool,
    pub force_aspect_ratio: bool,
    pub use_attach_ui_fix: Option<bool>,
    pub aspect_ratio: Option<f32>,
}
impl ProjectOptions {
    pub fn display_field<'a>(&'a self, document: &'a ChartDocument, key: &str) -> &'a str {
        let authored = document.as_json()["META"]
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or(if key == "name" { "Untitled" } else { "" });
        if document.as_json()["META"]["phiStudioMetadataEdited"].as_bool() == Some(true) {
            return authored;
        }
        if self.authored.get(key).is_some_and(|v| v == authored) {
            self.display.get(key).map_or(authored, String::as_str)
        } else {
            authored
        }
    }
    pub fn background_dim(&self, document: &ChartDocument) -> f32 {
        let value = match document.as_json()["META"].get("backgroundDim") {
            Some(value) => value.as_f64(),
            None => self.background_dim,
        };
        value
            .filter(|v| v.is_finite() && (0. ..=1.).contains(v))
            .unwrap_or(0.6) as f32
    }
    pub fn music_offset_seconds(&self, document: &ChartDocument) -> f64 {
        document.offset_seconds() + self.manifest_offset_ms as f64 / 1000.
    }
}
