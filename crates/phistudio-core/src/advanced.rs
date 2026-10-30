//! Authoring additions; sparse history and native interpolation remain shared.
use crate::{
    Beat, ChartDocument, ChartError, EventType, LineEvent,
    array_edit::{ArrayEdit, Segment},
    line_properties::FieldChange,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartMetadata {
    pub name: String,
    pub level: String,
    pub composer: String,
    pub charter: String,
    pub offset_ms: f64,
}
// RPE's native META offset is i32 milliseconds. Keep fractional editor values
// precise, but never serialize whole milliseconds as JSON floating-point tokens.
pub(crate) fn offset_value(ms: f64) -> Value {
    if ms.fract() == 0. && (i32::MIN as f64..=i32::MAX as f64).contains(&ms) {
        json!(ms as i32)
    } else {
        json!(ms)
    }
}
impl ChartMetadata {
    pub fn read(doc: &ChartDocument, options: &crate::ProjectOptions) -> Self {
        Self {
            name: options.display_field(doc, "name").into(),
            level: options.display_field(doc, "level").into(),
            composer: options.display_field(doc, "composer").into(),
            charter: options.display_field(doc, "charter").into(),
            offset_ms: doc.as_json()["META"]["offset"].as_f64().unwrap_or(0.),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StoryKind {
    ScaleX,
    ScaleY,
    Color,
    Text,
    Paint,
    Gif,
    Incline,
}
impl StoryKind {
    pub const EDITABLE: [Self; 6] = [
        Self::ScaleX,
        Self::ScaleY,
        Self::Color,
        Self::Text,
        Self::Paint,
        Self::Gif,
    ];
    pub const ALL: [Self; 7] = [
        Self::ScaleX,
        Self::ScaleY,
        Self::Color,
        Self::Text,
        Self::Paint,
        Self::Gif,
        Self::Incline,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::ScaleX => "scaleXEvents",
            Self::ScaleY => "scaleYEvents",
            Self::Color => "colorEvents",
            Self::Text => "textEvents",
            Self::Paint => "paintEvents",
            Self::Gif => "gifEvents",
            Self::Incline => "inclineEvents",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::ScaleX => "Scale X",
            Self::ScaleY => "Scale Y",
            Self::Color => "Color",
            Self::Text => "Text",
            Self::Paint => "Paint",
            Self::Gif => "GIF progress",
            Self::Incline => "Incline",
        }
    }
    pub fn default_value(self) -> Value {
        match self {
            Self::ScaleX | Self::ScaleY => json!(1.),
            Self::Color => json!([255, 255, 255]),
            Self::Text => json!(""),
            _ => json!(0.),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StoryEvent {
    pub start_time: Beat,
    pub end_time: Beat,
    pub start: Value,
    pub end: Value,
    pub easing_type: i32,
    pub easing_left: f64,
    pub easing_right: f64,
    pub bezier: u8,
    pub bezier_points: [f64; 4],
    #[serde(default)]
    pub linkgroup: i32,
}
impl Default for StoryEvent {
    fn default() -> Self {
        Self::new(StoryKind::ScaleX, Beat::ZERO, Beat::ONE)
    }
}
impl StoryEvent {
    pub fn new(kind: StoryKind, start_time: Beat, end_time: Beat) -> Self {
        Self {
            start_time,
            end_time,
            start: kind.default_value(),
            end: kind.default_value(),
            easing_type: 1,
            easing_left: 0.,
            easing_right: 1.,
            bezier: 0,
            bezier_points: [0., 0., 1., 1.],
            linkgroup: 0,
        }
    }
    pub fn validate(&self, kind: StoryKind) -> Result<(), ChartError> {
        LineEvent {
            start_time: self.start_time,
            end_time: self.end_time,
            easing_type: self.easing_type,
            easing_left: self.easing_left,
            easing_right: self.easing_right,
            bezier: self.bezier,
            bezier_points: self.bezier_points,
            ..Default::default()
        }
        .validate(EventType::MoveX)?;
        for value in [&self.start, &self.end] {
            let valid = match kind {
                StoryKind::Color => value.as_array().is_some_and(|v| {
                    v.len() == 3 && v.iter().all(|c| c.as_u64().is_some_and(|n| n <= 255))
                }),
                StoryKind::Text => value.as_str().is_some_and(|s| s.len() <= 16_384),
                _ => value
                    .as_f64()
                    .is_some_and(|v| v.is_finite() && (v as f32).is_finite()),
            };
            if !valid {
                return Err(ChartError::Invalid("Invalid storyboard value".into()));
            }
        }
        Ok(())
    }
    pub(crate) fn merge(
        &self,
        kind: StoryKind,
        before: Option<&Value>,
    ) -> Result<Value, ChartError> {
        self.validate(kind)?;
        let mut value = before.cloned().unwrap_or(json!({}));
        let old = before
            .map(|v| serde_json::from_value::<Self>(v.clone()))
            .transpose()?;
        let old = old.map(|v| serde_json::to_value(v).unwrap());
        for (k, v) in serde_json::to_value(self)?.as_object().unwrap() {
            if old.as_ref().is_none_or(|o| o.get(k) != Some(v)) {
                value[k] = v.clone();
            }
        }
        Ok(value)
    }
}
impl ChartDocument {
    pub fn story_events(
        &self,
        line: usize,
        kind: StoryKind,
    ) -> Result<Vec<StoryEvent>, ChartError> {
        let l = self.root["judgeLineList"]
            .get(line)
            .ok_or_else(|| ChartError::Invalid("Judge line no longer exists".into()))?;
        match l.get("extended").and_then(|e| e.get(kind.key())) {
            None | Some(Value::Null) => Ok(vec![]),
            Some(Value::Array(v)) => v
                .iter()
                .map(|v| serde_json::from_value(v.clone()).map_err(Into::into))
                .collect(),
            _ => Err(ChartError::Invalid(
                "Storyboard track must be an array".into(),
            )),
        }
    }
}
#[derive(Debug)]
pub(crate) enum AdvancedDelta {
    Metadata(Vec<FieldChange>),
    Story(Box<ArrayEdit>),
    StoryBatch(Vec<crate::array_edit::ArrayBatch>),
}
impl AdvancedDelta {
    pub fn apply(&self, doc: &mut ChartDocument, forward: bool) {
        match self {
            Self::Metadata(fields) => {
                for f in fields {
                    f.apply(doc.root["META"].as_object_mut().unwrap(), forward);
                }
            }
            Self::Story(edit) => edit.apply(&mut doc.root, forward),
            Self::StoryBatch(edits) => {
                if forward {
                    for edit in edits {
                        edit.apply(&mut doc.root, true);
                    }
                } else {
                    for edit in edits.iter().rev() {
                        edit.apply(&mut doc.root, false);
                    }
                }
            }
        }
    }
}
pub(crate) fn erase_story(
    doc: &ChartDocument,
    ids: Vec<(usize, StoryKind, usize)>,
) -> Result<Option<AdvancedDelta>, ChartError> {
    if ids.len() > 20_000 {
        return Err(ChartError::Invalid(
            "Select at most 20,000 storyboard events".into(),
        ));
    }
    let mut tracks = std::collections::BTreeMap::<_, std::collections::BTreeSet<_>>::new();
    for (line, kind, index) in ids {
        tracks.entry((line, kind)).or_default().insert(index);
    }
    let mut edits = vec![];
    for ((line, kind), indices) in tracks {
        let count = doc.story_events(line, kind)?.len();
        if indices.iter().any(|i| *i >= count) {
            return Err(ChartError::Invalid(
                "Storyboard event no longer exists".into(),
            ));
        }
        let path = vec![
            Segment::Field("judgeLineList"),
            Segment::Index(line),
            Segment::Field("extended"),
            Segment::Field(kind.key()),
        ];
        edits.push(crate::array_edit::ArrayBatch::prepare(
            &doc.root,
            path,
            &indices.into_iter().collect::<Vec<_>>(),
            vec![],
            false,
            true,
        )?);
    }
    Ok((!edits.is_empty()).then_some(AdvancedDelta::StoryBatch(edits)))
}
pub(crate) fn metadata(
    doc: &ChartDocument,
    meta: ChartMetadata,
) -> Result<Option<AdvancedDelta>, ChartError> {
    if meta.name.trim().is_empty()
        || !meta.offset_ms.is_finite()
        || [&meta.name, &meta.level, &meta.composer, &meta.charter]
            .iter()
            .any(|s| s.len() > 4096)
    {
        return Err(ChartError::Invalid(
            "Use a chart name, finite offset and metadata up to 4,096 bytes per field".into(),
        ));
    }
    let mut fields = Vec::new();
    for (key, value) in [
        ("name", json!(meta.name)),
        ("level", json!(meta.level)),
        ("composer", json!(meta.composer)),
        ("charter", json!(meta.charter)),
        ("offset", offset_value(meta.offset_ms)),
        ("phiStudioMetadataEdited", json!(true)),
    ] {
        let before = doc.root["META"].get(key).cloned();
        if before.as_ref() != Some(&value) {
            fields.push(FieldChange {
                key: key.into(),
                before,
                after: Some(value),
            });
        }
    }
    Ok((!fields.is_empty()).then_some(AdvancedDelta::Metadata(fields)))
}
pub(crate) fn story(
    doc: &ChartDocument,
    line: usize,
    kind: StoryKind,
    index: Option<usize>,
    event: Option<StoryEvent>,
) -> Result<Option<AdvancedDelta>, ChartError> {
    let values = doc.story_events(line, kind)?;
    if index.is_some_and(|i| i >= values.len()) {
        return Err(ChartError::Invalid(
            "Storyboard event no longer exists".into(),
        ));
    }
    let raw = &doc.root["judgeLineList"][line]["extended"][kind.key()];
    let old = index.and_then(|i| raw.get(i));
    if let Some(event) = &event {
        event.validate(kind)?;
        for (i, e) in values.iter().enumerate() {
            if Some(i) != index && event.start_time < e.end_time && e.start_time < event.end_time {
                return Err(ChartError::Invalid(
                    "Storyboard events in one track cannot overlap".into(),
                ));
            }
        }
    }
    let after = event.map(|e| e.merge(kind, old)).transpose()?;
    if old == after.as_ref() {
        return Ok(None);
    }
    let path = vec![
        Segment::Field("judgeLineList"),
        Segment::Index(line),
        Segment::Field("extended"),
        Segment::Field(kind.key()),
    ];
    let edit = ArrayEdit::prepare(
        &doc.root,
        path,
        index.unwrap_or(values.len()),
        index.is_some(),
        after,
        true,
        true,
    )?;
    Ok(Some(AdvancedDelta::Story(Box::new(edit))))
}
