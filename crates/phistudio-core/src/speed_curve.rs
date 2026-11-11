//! Note-distance curve authoring changes only native speed events on one line/layer.
use crate::{
    Beat, ChartDocument, ChartError, ControlKind, ControlPoint, EventType, LineEvent, Note, NoteId,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub const RECIPE_KEY: &str = "phiStudioSpeedCurves";
pub const MAX_SPEED_EVENTS: usize = 20_000;
pub const MAX_CURVE_POINTS: usize = 256;
#[derive(Clone, Debug)]
pub struct SpeedCurveSource {
    pub id: NoteId,
    pub note: Note,
    pub bpm: Value,
    pub meta: Value,
    pub layers: Vec<Vec<Value>>,
    pub control: Vec<ControlPoint>,
    pub recipes: Value,
    pub fingerprint: String,
    /// Terminal clock of the current native preview, supplied by the runtime.
    pub native_horizon: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpeedCurvePoint {
    pub beat: Beat,
    pub distance: f64,
    /// The original curve's coordinate, retained while a draft moves in time.
    pub source_beat: Beat,
    /// None retains the original shape. Values 1–29 shape distance, not speed.
    pub easing: Option<i32>,
    pub fixed: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpeedCurveRecipe {
    pub version: u32,
    pub signature: String,
    #[serde(default)]
    pub context: String,
    pub note: Note,
    pub range: [Beat; 2],
    pub points: Vec<SpeedCurvePoint>,
}
#[derive(Clone, Debug)]
pub struct SpeedCurveEdit {
    pub id: NoteId,
    pub layer: usize,
    pub expected: String,
    pub events: Vec<Value>,
    pub recipe: SpeedCurveRecipe,
}
pub fn speed_signature(events: &[Value]) -> String {
    let mut hash = Sha256::new();
    // Values originate in the preserved document; a changed field invalidates hints.
    for event in events {
        hash.update(event.to_string().as_bytes());
        hash.update([0]);
    }
    format!("{:x}", hash.finalize())
}
impl SpeedCurveSource {
    pub fn capture(doc: &ChartDocument, id: NoteId) -> Result<Self, ChartError> {
        let note = doc.note(id.line, id.index)?;
        if doc.as_json()["META"]["phiPecHeights"]["signature"]
            .as_str()
            .is_some_and(|stored| {
                crate::pec_height_signature(doc.as_json()).as_deref() == Some(stored)
            })
        {
            return Err(ChartError::Invalid(
                "Speed curves require native speed events; this chart has a PEC height override"
                    .into(),
            ));
        }
        let mut layers = Vec::new();
        let count = doc.as_json()["judgeLineList"][id.line]["eventLayers"]
            .as_array()
            .map_or(4, Vec::len)
            .max(4);
        if count > 16 {
            return Err(ChartError::Invalid(
                "Speed curves support up to 16 source layers and edit layers 0–3".into(),
            ));
        }
        let mut total = 0;
        for layer in 0..count {
            let events = doc.event_values(id.line, layer, EventType::Speed)?;
            total += events.len();
            if total > MAX_SPEED_EVENTS {
                return Err(ChartError::Invalid(
                    "The selected speed track exceeds 20,000 events".into(),
                ));
            }
            layers.push(events.to_vec());
        }
        let bpm = doc.as_json()["BPMList"].clone();
        let meta = json!({"offset":0,"RPEVersion":doc.as_json()["META"].get("RPEVersion").cloned().unwrap_or(json!(140))});
        let control = doc.control_points(id.line, ControlKind::Y)?;
        let recipes = doc.as_json()["judgeLineList"][id.line]
            .get(RECIPE_KEY)
            .cloned()
            .unwrap_or(Value::Null);
        let mut hash = Sha256::new();
        hash.update(serde_json::to_vec(&note)?);
        hash.update(bpm.to_string().as_bytes());
        hash.update(meta.to_string().as_bytes());
        for layer in &layers {
            hash.update(speed_signature(layer).as_bytes());
        }
        for p in &control {
            hash.update(format!("{}:{}:{};", p.x, p.value, p.easing).as_bytes());
        }
        let fingerprint = format!("{:x}", hash.finalize());
        Ok(Self {
            id,
            note,
            bpm,
            meta,
            layers,
            control,
            recipes,
            fingerprint,
            native_horizon: None,
        })
    }
    pub fn curve_context(&self, layer: usize) -> String {
        let mut hash = Sha256::new();
        hash.update(self.bpm.to_string().as_bytes());
        hash.update(self.meta.to_string().as_bytes());
        hash.update(
            format!(
                "{:?}",
                (
                    &self.note.kind,
                    self.note.start_time,
                    self.note.end_time,
                    self.note.speed,
                    self.note.above,
                    self.note.y_offset
                )
            )
            .as_bytes(),
        );
        hash.update(format!("{:?}", self.native_horizon).as_bytes());
        for (index, events) in self.layers.iter().enumerate() {
            if index != layer {
                hash.update(speed_signature(events).as_bytes());
            }
        }
        for point in &self.control {
            hash.update(format!("{}:{}:{};", point.x, point.value, point.easing).as_bytes());
        }
        format!("{:x}", hash.finalize())
    }
    pub fn saved_recipes(&self, layer: usize) -> Vec<SpeedCurveRecipe> {
        let Some(raw) = self.recipes.get(layer.to_string()) else {
            return Vec::new();
        };
        let values = raw
            .as_array()
            .map_or_else(|| vec![raw], |a| a.iter().take(256).collect());
        let Some(events) = self.layers.get(layer) else {
            return Vec::new();
        };
        let signature = speed_signature(events);
        values
            .into_iter()
            .filter_map(|raw| serde_json::from_value::<SpeedCurveRecipe>(raw.clone()).ok())
            .filter(|r| {
                r.version == 1
                    && r.signature == signature
                    && r.points.len() >= 2
                    && r.points.len() <= MAX_CURVE_POINTS
                    && r.range[0] >= Beat::ZERO
                    && r.range[1] > r.range[0]
                    && r.points.first().is_some_and(|p| p.beat == r.range[0])
                    && r.points.last().is_some_and(|p| p.beat == r.range[1])
                    && r.points.windows(2).all(|p| p[0].beat < p[1].beat)
                    && r.points.iter().all(|p| {
                        p.distance.is_finite()
                            && p.distance.abs() <= 1e9
                            && p.easing.is_none_or(|e| (1..=29).contains(&e))
                    })
            })
            .collect()
    }
    pub fn recipe(&self, layer: usize) -> Option<SpeedCurveRecipe> {
        self.saved_recipes(layer)
            .into_iter()
            .find(|r| r.note == self.note)
    }
}
#[derive(Debug)]
pub(crate) struct SpeedCurveDelta {
    line: usize,
    layer: usize,
    before_layer: Option<Value>,
    after_layer: Value,
    container: Option<bool>,
    before_len: usize,
    before_recipes: Option<Value>,
    after_recipes: Value,
}
impl SpeedCurveDelta {
    pub fn apply(&self, doc: &mut ChartDocument, forward: bool) {
        let line = &mut doc.root["judgeLineList"][self.line];
        if forward {
            if !line["eventLayers"].is_array() {
                line["eventLayers"] = json!([]);
            }
            if let Some(layers) = line["eventLayers"].as_array_mut() {
                while layers.len() <= self.layer {
                    layers.push(Value::Null);
                }
                layers[self.layer] = self.after_layer.clone();
            }
        } else if self.container == Some(true) {
            if let Some(layers) = line["eventLayers"].as_array_mut() {
                if let Some(before) = &self.before_layer {
                    layers[self.layer] = before.clone();
                }
                layers.truncate(self.before_len);
            }
        } else if self.container == Some(false) {
            line["eventLayers"] = Value::Null;
        } else if let Some(object) = line.as_object_mut() {
            object.shift_remove("eventLayers");
        }
        if let Some(value) = if forward {
            Some(&self.after_recipes)
        } else {
            self.before_recipes.as_ref()
        } {
            line[RECIPE_KEY] = value.clone();
        } else if let Some(object) = line.as_object_mut() {
            object.shift_remove(RECIPE_KEY);
        }
    }
}
pub(crate) fn prepare(
    doc: &ChartDocument,
    edit: SpeedCurveEdit,
) -> Result<Option<SpeedCurveDelta>, ChartError> {
    let fail = |s: &str| ChartError::Invalid(s.into());
    if edit.layer > 3
        || edit.events.len() > MAX_SPEED_EVENTS
        || (edit.recipe.points.len() < 2 || edit.recipe.points.len() > MAX_CURVE_POINTS)
    {
        return Err(fail("Invalid speed curve size or layer"));
    }
    let source = SpeedCurveSource::capture(doc, edit.id)?;
    if source.fingerprint != edit.expected {
        return Err(fail(
            "The chart changed. Reload the speed curve before applying",
        ));
    }
    if edit.recipe.version != 1
        || edit.recipe.signature != speed_signature(&edit.events)
        || edit.recipe.range[0] < Beat::ZERO
        || edit.recipe.range[1] <= edit.recipe.range[0]
    {
        return Err(fail("Invalid speed curve recipe"));
    }
    for point in &edit.recipe.points {
        if !point.distance.is_finite()
            || point.distance.abs() > 1e9
            || point.beat < edit.recipe.range[0]
            || point.beat > edit.recipe.range[1]
            || point.easing.is_some_and(|e| !(1..=29).contains(&e))
        {
            return Err(fail(
                "Curve points need finite distances, ordered times and easing 1–29",
            ));
        }
    }
    if edit
        .recipe
        .points
        .windows(2)
        .any(|p| p[0].beat >= p[1].beat)
    {
        return Err(fail("Speed curve point times must be distinct"));
    }
    for raw in &edit.events {
        let event = LineEvent::read(EventType::Speed, raw)?;
        if event.end_time < event.start_time
            || (event.start_time < Beat::ZERO && event.end_time > edit.recipe.range[0])
            || !event.start.is_finite()
            || !event.end.is_finite()
            || event.start.abs() > 1e7
            || event.end.abs() > 1e7
        {
            return Err(fail("Invalid generated speed event"));
        }
    }
    let raw = &doc.root["judgeLineList"][edit.id.line];
    let layer = &raw["eventLayers"][edit.layer];
    if !layer.is_object() && !layer.is_null() {
        return Err(fail("Invalid event layer"));
    }
    let before_events = layer.get("speedEvents").cloned();
    let after_events = Value::Array(edit.events);
    let before_recipes = raw.get(RECIPE_KEY).cloned();
    let mut after_recipes = before_recipes
        .clone()
        .filter(Value::is_object)
        .unwrap_or(json!({}));
    let range = edit.recipe.range;
    let signature = edit.recipe.signature.clone();
    let mut groups = Vec::new();
    for old in source.saved_recipes(edit.layer) {
        if old.range[1] <= range[0] || old.range[0] >= range[1] {
            let mut old = old;
            old.signature = signature.clone();
            groups.push(old);
            continue;
        }
        for kept in [
            [old.range[0], range[0].min(old.range[1])],
            [range[1].max(old.range[0]), old.range[1]],
        ] {
            if kept[1] <= kept[0] {
                continue;
            }
            let mut recipe = old.clone();
            recipe.range = kept;
            recipe.signature = signature.clone();
            recipe
                .points
                .retain(|p| p.beat >= kept[0] && p.beat <= kept[1]);
            for beat in kept {
                if !recipe.points.iter().any(|p| p.beat == beat) {
                    recipe.points.push(SpeedCurvePoint {
                        beat,
                        source_beat: beat,
                        distance: 0.,
                        easing: None,
                        fixed: false,
                    });
                }
            }
            recipe.points.sort_by_key(|p| p.beat);
            for p in &mut recipe.points {
                p.easing = None;
            }
            groups.push(recipe);
        }
    }
    groups.push(edit.recipe);
    groups.sort_by_key(|r| r.range[0]);
    if groups.len() > 256 {
        return Err(fail(
            "The speed curve region library is full; reuse an existing range",
        ));
    }
    after_recipes[edit.layer.to_string()] = serde_json::to_value(groups)?;
    if before_events.as_ref() == Some(&after_events)
        && before_recipes.as_ref() == Some(&after_recipes)
    {
        return Ok(None);
    }
    let before_layer = raw["eventLayers"]
        .as_array()
        .and_then(|a| a.get(edit.layer))
        .cloned();
    let mut after_layer = before_layer
        .clone()
        .filter(Value::is_object)
        .unwrap_or(json!({}));
    after_layer["speedEvents"] = after_events;
    let container = raw.get("eventLayers").map(Value::is_array);
    let before_len = raw["eventLayers"].as_array().map_or(0, Vec::len);
    Ok(Some(SpeedCurveDelta {
        line: edit.id.line,
        layer: edit.layer,
        before_layer,
        after_layer,
        container,
        before_len,
        before_recipes,
        after_recipes,
    }))
}
