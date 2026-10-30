use crate::{ChartDocument, ChartError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttachedUi {
    Pause,
    ComboNumber,
    Combo,
    Score,
    Bar,
    Name,
    Level,
}
impl AttachedUi {
    pub const ALL: [Self; 7] = [
        Self::Pause,
        Self::ComboNumber,
        Self::Combo,
        Self::Score,
        Self::Bar,
        Self::Name,
        Self::Level,
    ];
    pub fn parse(value: &Value) -> Option<Self> {
        let text = value
            .as_str()?
            .trim()
            .to_ascii_lowercase()
            .replace(['_', '-', ' '], "");
        Self::ALL.into_iter().find(|v| v.key() == text)
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::ComboNumber => "combonumber",
            Self::Combo => "combo",
            Self::Score => "score",
            Self::Bar => "bar",
            Self::Name => "name",
            Self::Level => "level",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Pause => "Pause",
            Self::ComboNumber => "Combo number",
            Self::Combo => "Combo label",
            Self::Score => "Score",
            Self::Bar => "Progress bar",
            Self::Name => "Chart name",
            Self::Level => "Level",
        }
    }
}
pub fn rotation_inheritance(value: &Value) -> bool {
    value
        .as_bool()
        .unwrap_or_else(|| value.as_f64().is_some_and(|n| n.trunc() != 0.))
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LineProperties {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Texture")]
    pub texture: String,
    pub anchor: [f64; 2],
    #[serde(rename = "Group")]
    pub group: i32,
    #[serde(rename = "bpmfactor")]
    pub bpm_factor: f64,
    pub father: i32,
    #[serde(rename = "rotateWithFather")]
    pub rotate_with_father: bool,
    #[serde(rename = "isCover")]
    pub cover: u8,
    #[serde(rename = "zOrder")]
    pub z_order: i32,
    #[serde(rename = "attachUI")]
    pub attach_ui: Option<AttachedUi>,
}
impl Default for LineProperties {
    fn default() -> Self {
        Self {
            name: "Line".into(),
            texture: "line.png".into(),
            anchor: [0.5; 2],
            group: 0,
            bpm_factor: 1.,
            father: -1,
            rotate_with_father: false,
            cover: 1,
            z_order: 0,
            attach_ui: None,
        }
    }
}
impl LineProperties {
    fn read(raw: &Value) -> Result<Self, ChartError> {
        let mut value = raw.clone();
        value["anchor"] = json!([
            raw["anchor"][0].as_f64().unwrap_or(0.5),
            raw["anchor"][1].as_f64().unwrap_or(0.5)
        ]);
        value["rotateWithFather"] = json!(rotation_inheritance(&raw["rotateWithFather"]));
        value["attachUI"] = serde_json::to_value(AttachedUi::parse(&raw["attachUI"]))?;
        Ok(serde_json::from_value(value)?)
    }
    pub(crate) fn validate(&self, doc: &ChartDocument, index: usize) -> Result<(), ChartError> {
        if !self
            .anchor
            .iter()
            .all(|v| v.is_finite() && (*v as f32).is_finite())
        {
            return Err(ChartError::Invalid(
                "Line anchor coordinates must be finite".into(),
            ));
        }
        if self.name.trim().is_empty() || self.texture.trim().is_empty() {
            return Err(ChartError::Invalid(
                "Line name and texture must not be empty".into(),
            ));
        }
        if self.group < 0 || !self.bpm_factor.is_finite() || self.bpm_factor <= 0. {
            return Err(ChartError::Invalid(
                "Line group must be nonnegative and BPM factor positive".into(),
            ));
        }
        let mut parent = self.father;
        let mut seen = vec![false; doc.line_count()];
        while parent != -1 {
            if parent < 0
                || parent as usize >= seen.len()
                || parent as usize == index
                || seen[parent as usize]
            {
                return Err(ChartError::Invalid(
                    "Invalid judge-line parent or parent cycle".into(),
                ));
            }
            seen[parent as usize] = true;
            parent = doc.root["judgeLineList"][parent as usize]["father"]
                .as_i64()
                .unwrap_or(-1) as i32;
        }
        Ok(())
    }
    pub(crate) fn changes(
        &self,
        doc: &ChartDocument,
        index: usize,
    ) -> Result<Vec<FieldChange>, ChartError> {
        let before = doc.line_properties(index)?;
        self.validate(doc, index)?;
        let old = serde_json::to_value(before)?;
        let new = serde_json::to_value(self)?;
        Ok(new
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, v)| old[*k] != **v)
            .map(|(key, value)| FieldChange {
                key: key.clone(),
                before: doc.root["judgeLineList"][index].get(key).cloned(),
                after: if key == "attachUI" && value.is_null() {
                    None
                } else {
                    Some(value.clone())
                },
            })
            .collect())
    }
}
#[derive(Debug)]
pub(crate) struct FieldChange {
    pub key: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}
impl FieldChange {
    pub fn apply(&self, object: &mut serde_json::Map<String, Value>, forward: bool) {
        let value = if forward { &self.after } else { &self.before };
        if let Some(value) = value {
            object.insert(self.key.clone(), value.clone());
        } else {
            object.shift_remove(&self.key);
        }
    }
}
impl ChartDocument {
    pub fn line_properties(&self, index: usize) -> Result<LineProperties, ChartError> {
        let raw = self.root["judgeLineList"]
            .get(index)
            .ok_or_else(|| ChartError::Invalid("Judge line no longer exists".into()))?;
        LineProperties::read(raw)
    }
    pub fn bpm_changes(&self) -> Vec<(usize, crate::BpmChange)> {
        self.root["BPMList"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, v)| (i, serde_json::from_value(v.clone()).expect("validated BPM")))
            .collect()
    }
}
