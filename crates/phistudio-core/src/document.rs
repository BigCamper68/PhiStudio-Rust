use crate::{Beat, BpmChange, TempoMap};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, thiserror::Error)]
pub enum ChartError {
    #[error("{0}")]
    Invalid(String),
    #[error("Invalid chart JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("File operation failed: {0}")]
    Io(#[from] std::io::Error),
}

/// RPE codes differ from official Phigros codes: 2=Hold, 3=Flick, 4=Drag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum NoteKind {
    Tap = 1,
    Hold = 2,
    Flick = 3,
    Drag = 4,
}

impl TryFrom<u8> for NoteKind {
    type Error = ChartError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Tap),
            2 => Ok(Self::Hold),
            3 => Ok(Self::Flick),
            4 => Ok(Self::Drag),
            _ => Err(ChartError::Invalid(format!(
                "Unknown RPE note type {value}"
            ))),
        }
    }
}
impl Serialize for NoteKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}
impl<'de> Deserialize<'de> for NoteKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(u8::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Editable projection. The original JSON object remains in ChartDocument;
/// changing these fields never replaces extensions such as hitsound/control data.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Note {
    #[serde(rename = "type")]
    pub kind: NoteKind,
    pub start_time: Beat,
    pub end_time: Beat,
    #[serde(rename = "positionX")]
    pub x: f64,
    pub above: u8,
    pub is_fake: u8,
    pub alpha: u16,
    pub size: f64,
    pub speed: f64,
    pub visible_time: f64,
    pub y_offset: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judge_area: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tint_hit_effects: Option<[u8; 3]>,
}
impl Default for Note {
    fn default() -> Self {
        Self {
            kind: NoteKind::Tap,
            start_time: Beat::ZERO,
            end_time: Beat::ZERO,
            x: 0.,
            above: 1,
            is_fake: 0,
            alpha: 255,
            size: 1.,
            speed: 1.,
            visible_time: 999999.,
            y_offset: 0.,
            judge_area: None,
            tint_hit_effects: None,
        }
    }
}
impl Note {
    pub fn validate(&self) -> Result<(), ChartError> {
        if self
            .judge_area
            .is_some_and(|v| !v.is_finite() || !(v as f32).is_finite() || v < 0.)
        {
            return Err(ChartError::Invalid(
                "Judge area must be finite and nonnegative".into(),
            ));
        }
        if [
            self.x,
            self.size,
            self.speed,
            self.visible_time,
            self.y_offset,
        ]
        .iter()
        .any(|x| !x.is_finite())
        {
            return Err(ChartError::Invalid("Note values must be finite".into()));
        }
        if self.kind == NoteKind::Hold && self.end_time < self.start_time {
            return Err(ChartError::Invalid("Hold end precedes its start".into()));
        }
        Ok(())
    }
    pub(crate) fn merge_into(&self, original: Option<&Value>) -> Result<Value, ChartError> {
        self.validate()?;
        let mut object = original
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let previous = original.and_then(|v| Self::deserialize(v).ok());
        let mut updated = serde_json::to_value(self)?.as_object().unwrap().clone();
        // New optional controls must not materialize or normalize untouched
        // missing/null extensions when an unrelated note property changes.
        for (key, unchanged) in [
            (
                "judgeArea",
                previous
                    .as_ref()
                    .is_some_and(|n| n.judge_area == self.judge_area),
            ),
            (
                "tintHitEffects",
                previous
                    .as_ref()
                    .is_some_and(|n| n.tint_hit_effects == self.tint_hit_effects),
            ),
        ] {
            if unchanged {
                updated.shift_remove(key);
            } else if !updated.contains_key(key) {
                object.shift_remove(key);
            }
        }
        object.extend(updated);
        Ok(Value::Object(object))
    }
}

#[derive(Clone, Debug)]
pub struct LineSummary {
    pub index: usize,
    pub name: String,
    pub note_count: usize,
}

#[derive(Clone, Debug)]
pub struct ChartDocument {
    pub(crate) root: Value,
    tempo: TempoMap,
}

impl ChartDocument {
    // Shared limit for chart parsing, imports, storage and recovery.
    pub const MAX_JSON_BYTES: usize = 256 * 1024 * 1024;

    pub fn parse(json: &str) -> Result<Self, ChartError> {
        if json.len() > Self::MAX_JSON_BYTES {
            return Err(ChartError::Invalid("Chart JSON exceeds 256 MiB".into()));
        }
        Self::from_value(serde_json::from_str(json)?)
    }

    fn validate_root(root: &Value) -> Result<(), ChartError> {
        if !root.is_object() || !root.get("META").is_some_and(Value::is_object) {
            return Err(ChartError::Invalid(
                "Expected an RPE chart object with META".into(),
            ));
        }
        Ok(())
    }

    pub fn from_value(mut root: Value) -> Result<Self, ChartError> {
        Self::validate_root(&root)?;
        crate::noise_fields::export::restore(&mut root)?;
        Self::from_edit_value(root)
    }

    /// Validate an already editable tree without running import conversions.
    pub(crate) fn from_edit_value(root: Value) -> Result<Self, ChartError> {
        let invalid = |message: &str| ChartError::Invalid(message.into());
        Self::validate_root(&root)?;
        let changes: Vec<BpmChange> = serde_json::from_value(
            root.get("BPMList")
                .cloned()
                .ok_or_else(|| invalid("Missing BPMList"))?,
        )?;
        let tempo = TempoMap::new(changes)?;
        let lines = root
            .get("judgeLineList")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("Missing judgeLineList"))?;
        if lines.is_empty() {
            return Err(invalid("At least one judge line is required"));
        }
        for (i, line) in lines.iter().enumerate() {
            if !line.is_object() {
                return Err(invalid(&format!("Line {i} is not an object")));
            }
            if let Some(notes) = line.get("notes").filter(|x| !x.is_null()) {
                let notes = notes
                    .as_array()
                    .ok_or_else(|| invalid(&format!("Line {i}: notes must be an array")))?;
                for (n, value) in notes.iter().enumerate() {
                    if !value.is_object() {
                        return Err(invalid(&format!("Line {i}, note {n}: expected an object")));
                    }
                    let note = Note::deserialize(value)
                        .map_err(|e| invalid(&format!("Line {i}, note {n}: {e}")))?;
                    note.validate()
                        .map_err(|e| invalid(&format!("Line {i}, note {n}: {e}")))?;
                }
            }
        }
        // prpr traverses parent lines recursively, so reject cycles before it sees them.
        let mut parents = Vec::with_capacity(lines.len());
        for (i, line) in lines.iter().enumerate() {
            let parent = match line.get("father").filter(|x| !x.is_null()) {
                Some(value) => value
                    .as_i64()
                    .ok_or_else(|| invalid(&format!("Line {i}: father must be an integer")))?,
                None => -1,
            };
            if parent < -1 || parent >= lines.len() as i64 {
                return Err(invalid(&format!("Line {i}: invalid parent {parent}")));
            }
            parents.push((parent >= 0).then_some(parent as usize));
        }
        let mut state = vec![0_u8; lines.len()];
        for i in 0..lines.len() {
            if state[i] == 2 {
                continue;
            }
            let mut chain = Vec::new();
            let mut current = Some(i);
            while let Some(id) = current {
                if state[id] == 1 {
                    return Err(invalid("Judge-line parent cycle"));
                }
                if state[id] == 2 {
                    break;
                }
                state[id] = 1;
                chain.push(id);
                current = parents[id];
            }
            for id in chain {
                state[id] = 2;
            }
        }
        Ok(Self { root, tempo })
    }

    pub fn empty(name: &str, bpm: f64) -> Result<Self, ChartError> {
        Self::from_value(json!({
            "META": {"RPEVersion": 123, "name": name, "composer": "", "charter": "", "level": "", "offset": 0, "song": "", "background": ""},
            "BPMList": [{"bpm": bpm, "startTime": [0,0,1]}],
            "judgeLineGroup": ["Default"], "judgeLineList": [Self::new_line("Line 0")]
        }))
    }

    pub(crate) fn new_line(name: &str) -> Value {
        let event = |value: f64| json!({"startTime":[0,0,1],"endTime":[1,0,1],"start":value,"end":value,"easingType":1,"easingLeft":0.,"easingRight":1.,"bezier":0,"bezierPoints":[0.,0.,0.,0.],"linkgroup":0});
        json!({"Name":name,"Texture":"line.png","Group":0,"father":-1,"isCover":1,"bpmfactor":1.,"zOrder":0,"numOfNotes":0,"notes":[],
            "eventLayers":[{"alphaEvents":[event(255.)],"moveXEvents":[event(0.)],"moveYEvents":[event(0.)],"rotateEvents":[event(0.)],"speedEvents":[event(10.)]}],"extended":{}})
    }

    pub fn to_json(&self) -> Result<String, ChartError> {
        Ok(serde_json::to_string_pretty(&self.serialization_value())?)
    }
    /// Repair integral float offsets from earlier editor versions on output,
    /// preserving the document/history and borrowing ordinary charts unchanged.
    pub(crate) fn serialization_value(&self) -> std::borrow::Cow<'_, Value> {
        if let Some(ms) = self.root["META"]["offset"].as_f64() {
            let offset = crate::advanced::offset_value(ms);
            if offset != self.root["META"]["offset"] {
                let mut root = self.root.clone();
                root["META"]["offset"] = offset;
                return std::borrow::Cow::Owned(root);
            }
        }
        std::borrow::Cow::Borrowed(&self.root)
    }
    pub fn as_json(&self) -> &Value {
        &self.root
    }
    pub fn with_plugin_data_export(
        &self,
        mode: phistudio_sdk::PluginDataExport,
    ) -> Result<std::borrow::Cow<'_, Self>, ChartError> {
        use std::borrow::Cow;
        match crate::plugin_data::export_value(&self.root, mode)? {
            Cow::Borrowed(_) => Ok(Cow::Borrowed(self)),
            Cow::Owned(root) => Ok(Cow::Owned(Self {
                root,
                tempo: self.tempo.clone(),
            })),
        }
    }
    pub fn tempo(&self) -> &TempoMap {
        &self.tempo
    }
    pub(crate) fn set_tempo(&mut self, tempo: TempoMap) {
        self.tempo = tempo;
    }
    pub fn name(&self) -> &str {
        self.root["META"]["name"].as_str().unwrap_or("Untitled")
    }
    pub fn offset_seconds(&self) -> f64 {
        self.root["META"]["offset"].as_f64().unwrap_or(0.) / 1000.
    }
    pub fn line_count(&self) -> usize {
        self.root["judgeLineList"].as_array().unwrap().len()
    }
    pub fn note_count(&self) -> usize {
        (0..self.line_count())
            .map(|i| self.note_values(i).map_or(0, |n| n.len()))
            .sum()
    }
    pub fn lines(&self) -> Vec<LineSummary> {
        self.root["judgeLineList"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, l)| LineSummary {
                index: i,
                name: l["Name"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Line {i}")),
                note_count: self.note_values(i).map_or(0, |n| n.len()),
            })
            .collect()
    }
    pub fn note(&self, line: usize, index: usize) -> Result<Note, ChartError> {
        let value = self
            .note_values(line)
            .and_then(|x| x.get(index))
            .ok_or_else(|| ChartError::Invalid("Note no longer exists".into()))?;
        Ok(Note::deserialize(value)?)
    }
    pub fn notes(&self, line: usize) -> Vec<(usize, Note)> {
        self.iter_notes(line).collect()
    }
    /// Decode notes lazily in source order without copying JSON extensions.
    pub fn iter_notes(&self, line: usize) -> impl Iterator<Item = (usize, Note)> + '_ {
        self.note_values(line)
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, n)| (i, Note::deserialize(n).expect("validated note")))
    }
    pub(crate) fn note_values(&self, line: usize) -> Option<&Vec<Value>> {
        self.root["judgeLineList"]
            .get(line)?
            .get("notes")?
            .as_array()
    }

    /// Only the render copy is ordered for prpr. Saving preserves source order.
    pub fn render_json(&self) -> Result<String, ChartError> {
        Ok(serde_json::to_string(&self.render_value()?)?)
    }
    pub fn render_value(&self) -> Result<Value, ChartError> {
        let mut root = self.root.clone();
        root["BPMList"].as_array_mut().unwrap().sort_by_key(|x| {
            serde_json::from_value::<Beat>(x["startTime"].clone()).expect("validated tempo")
        });
        Ok(root)
    }
}
