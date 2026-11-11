//! Authoring projections for Java PhiStudio's five standard event tracks.
use crate::array_edit::Segment;
use crate::{Beat, ChartDocument, ChartError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum EventType {
    MoveX,
    MoveY,
    Rotate,
    Alpha,
    Speed,
}
impl EventType {
    pub const ALL: [Self; 5] = [
        Self::MoveX,
        Self::MoveY,
        Self::Rotate,
        Self::Alpha,
        Self::Speed,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::MoveX => "moveXEvents",
            Self::MoveY => "moveYEvents",
            Self::Rotate => "rotateEvents",
            Self::Alpha => "alphaEvents",
            Self::Speed => "speedEvents",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::MoveX => "Move X",
            Self::MoveY => "Move Y",
            Self::Rotate => "Rotate",
            Self::Alpha => "Alpha",
            Self::Speed => "Speed",
        }
    }
    pub fn range(self) -> (f64, f64) {
        match self {
            Self::MoveX => (-675., 675.),
            Self::MoveY => (-450., 450.),
            Self::Rotate => (-180., 180.),
            Self::Alpha => (0., 255.),
            Self::Speed => (-20., 20.),
        }
    }
    pub fn default_value(self) -> f64 {
        match self {
            Self::Alpha => 255.,
            Self::Speed => 10.,
            _ => 0.,
        }
    }
    pub(crate) fn path(self, line: usize, layer: usize) -> Vec<Segment> {
        use Segment::*;
        vec![
            Field("judgeLineList"),
            Index(line),
            Field("eventLayers"),
            Index(layer),
            Field(self.key()),
        ]
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EventId {
    pub line: usize,
    pub layer: usize,
    pub kind: EventType,
    pub index: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LineEvent {
    #[serde(rename = "phiStudioConstant", default)]
    pub constant: bool,
    pub start_time: Beat,
    pub end_time: Beat,
    pub start: f64,
    pub end: f64,
    pub easing_type: i32,
    pub easing_left: f64,
    pub easing_right: f64,
    pub bezier: u8,
    pub bezier_points: [f64; 4],
    #[serde(
        rename = "phiStudioCustomEasing",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub custom_easing: Option<Box<crate::custom_easing::Binding>>,
    #[serde(rename = "linkgroup")]
    pub link_group: i32,
}
impl Default for LineEvent {
    fn default() -> Self {
        Self {
            constant: false,
            start_time: Beat::ZERO,
            end_time: Beat::ONE,
            start: 0.,
            end: 0.,
            easing_type: 1,
            easing_left: 0.,
            easing_right: 1.,
            bezier: 0,
            bezier_points: [0.; 4],
            custom_easing: None,
            link_group: 0,
        }
    }
}
impl LineEvent {
    pub fn read(kind: EventType, value: &Value) -> Result<Self, ChartError> {
        if !value.is_object() {
            return Err(ChartError::Invalid("Event must be an object".into()));
        }
        let mut data = std::borrow::Cow::Borrowed(value);
        if data.get("start").is_none() {
            data.to_mut()["start"] = json!(kind.default_value());
        }
        if data.get("end").is_none() {
            data.to_mut()["end"] = data["start"].clone();
        }
        if kind == EventType::Speed {
            // Imported speed easing is renderer metadata, retained in the raw tree.
            data.to_mut()["easingType"] = json!(1);
            data.to_mut()["easingLeft"] = json!(0.);
            data.to_mut()["easingRight"] = json!(1.);
            data.to_mut()["bezier"] = json!(0);
            data.to_mut()["bezierPoints"] = json!([0., 0., 0., 0.]);
        }
        Ok(Self::deserialize(data.as_ref())?)
    }
    pub fn validate(&self, kind: EventType) -> Result<(), ChartError> {
        let fail = |s: &str| Err(ChartError::Invalid(s.into()));
        if self.constant && self.start != self.end {
            return fail("Constant event endpoints must match");
        }
        if self.start_time < Beat::ZERO {
            return fail("Event start must not be negative");
        }
        if self.end_time <= self.start_time {
            return fail("Event end must be after its start");
        }
        if [self.start, self.end, self.easing_left, self.easing_right]
            .iter()
            .chain(self.bezier_points.iter())
            .any(|v| !v.is_finite())
        {
            return fail("Event values must be finite");
        }
        if let Some(binding) = &self.custom_easing {
            binding.validate_scalar_endpoints([self.start, self.end])?;
        }
        if kind == EventType::Alpha
            && self.custom_easing.is_none()
            && [self.start, self.end]
                .iter()
                .any(|v| !(-255. ..=255.).contains(v))
        {
            return fail("Event alpha must be between -255 and 255");
        }
        if !(1..=29).contains(&self.easing_type)
            || (kind == EventType::Speed && self.easing_type != 1)
        {
            return fail("Event easing must be 1–29; speed editing uses Linear");
        }
        if self.easing_left < 0. || self.easing_right > 1. || self.easing_left > self.easing_right {
            return fail("Easing bounds must satisfy 0 ≤ left ≤ right ≤ 1");
        }
        if self.link_group < 0 {
            return fail("Event link group must not be negative");
        }
        Ok(())
    }
    pub(crate) fn merge(&self, kind: EventType, raw: Option<&Value>) -> Result<Value, ChartError> {
        let mut next = self.clone();
        let previous = raw.map(|v| Self::read(kind, v)).transpose()?;
        if let Some(before) = &previous
            && next.custom_easing == before.custom_easing
            && let Some(binding) = &next.custom_easing
        {
            next.custom_easing = Some(Box::new(
                binding.rebase([before.start, before.end], [next.start, next.end])?,
            ));
        }
        if next.constant
            && let Some(binding) = &mut next.custom_easing
        {
            binding.source = Some([json!(next.start), json!(next.start)]);
        }
        next.validate(kind)?;
        let before = previous.as_ref().map(serde_json::to_value).transpose()?;
        let mut result = raw.cloned().unwrap_or_else(|| json!({}));
        if next.custom_easing.is_none()
            && previous.as_ref().is_some_and(|e| e.custom_easing.is_some())
        {
            result
                .as_object_mut()
                .unwrap()
                .shift_remove(crate::custom_easing::CURVE_KEY);
        }
        for (key, value) in serde_json::to_value(&next)?.as_object().unwrap() {
            if kind == EventType::Speed
                && matches!(
                    key.as_str(),
                    "easingType" | "easingLeft" | "easingRight" | "bezier" | "bezierPoints"
                )
            {
                continue;
            }
            if before.as_ref().is_none_or(|p| p[key] != *value) {
                result[key] = value.clone();
            }
        }
        Ok(result)
    }
}
impl ChartDocument {
    pub fn event_layer_count(&self, line: usize) -> usize {
        self.root["judgeLineList"][line]["eventLayers"]
            .as_array()
            .map_or(0, Vec::len)
    }
    pub fn event_values(
        &self,
        line: usize,
        layer: usize,
        kind: EventType,
    ) -> Result<&[Value], ChartError> {
        if line >= self.line_count() {
            return Err(ChartError::Invalid("Judge line no longer exists".into()));
        }
        let layers = &self.root["judgeLineList"][line]["eventLayers"];
        if !layers.is_array() && !layers.is_null() {
            return Err(ChartError::Invalid("eventLayers must be an array".into()));
        }
        let object = &layers[layer];
        if !object.is_object() && !object.is_null() {
            return Err(ChartError::Invalid(
                "Event layer must be an object or null".into(),
            ));
        }
        match &object[kind.key()] {
            Value::Array(v) => Ok(v),
            Value::Null => Ok(&[]),
            _ => Err(ChartError::Invalid("Event track must be an array".into())),
        }
    }
    pub fn event(&self, id: EventId) -> Result<LineEvent, ChartError> {
        let raw = self
            .event_values(id.line, id.layer, id.kind)?
            .get(id.index)
            .ok_or_else(|| ChartError::Invalid("Event no longer exists".into()))?;
        LineEvent::read(id.kind, raw)
    }
    pub fn events(
        &self,
        line: usize,
        layer: usize,
        kind: EventType,
    ) -> Result<Vec<(usize, LineEvent)>, ChartError> {
        self.event_values(line, layer, kind)?
            .iter()
            .enumerate()
            .map(|(i, v)| Ok((i, LineEvent::read(kind, v)?)))
            .collect()
    }
    pub fn event_candidate(
        &self,
        line: usize,
        layer: usize,
        kind: EventType,
        start: Beat,
        end: Beat,
    ) -> Result<LineEvent, ChartError> {
        let previous = self
            .events(line, layer, kind)?
            .into_iter()
            .map(|(_, v)| v)
            .filter(|v| v.end_time <= start)
            .max_by_key(|v| v.end_time);
        let value = previous.as_ref().map_or(kind.default_value(), |v| v.end);
        Ok(LineEvent {
            start_time: start,
            end_time: end,
            start: value,
            end: value,
            easing_type: if kind == EventType::Speed {
                1
            } else {
                previous.map_or(1, |v| v.easing_type)
            },
            ..Default::default()
        })
    }
    pub(crate) fn validate_event(
        &self,
        line: usize,
        layer: usize,
        kind: EventType,
        event: &LineEvent,
        ignored: Option<usize>,
    ) -> Result<(), ChartError> {
        if layer > 3 {
            return Err(ChartError::Invalid(
                "Only event layers 0–3 are editable".into(),
            ));
        }
        event.validate(kind)?;
        for (i, other) in self.events(line, layer, kind)? {
            if Some(i) != ignored
                && event.start_time < other.end_time
                && event.end_time > other.start_time
            {
                return Err(ChartError::Invalid(
                    "Event overlaps another event in this track".into(),
                ));
            }
        }
        Ok(())
    }
}
