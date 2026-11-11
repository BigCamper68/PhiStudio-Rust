//! Native RPE note sounds and judge-line distance control curves.
use crate::{ChartDocument, ChartError, NoteId, line_properties::FieldChange};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const SOUND_GAIN_KEY: &str = "phiStudioSoundGain";
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NoteSoundGain {
    pub source: Option<String>,
    pub rendered: String,
    pub volume: f64,
}
impl NoteSoundGain {
    pub(crate) fn validate(&self) -> Result<(), ChartError> {
        if !self.volume.is_finite() || !(0. ..1.).contains(&self.volume) {
            return Err(ChartError::Invalid(
                "Note sound volume must be between 0 and 100%".into(),
            ));
        }
        validate_hitsound(&self.rendered)?;
        if builtin_hitsound(&self.rendered) || self.source.as_deref() == Some(&self.rendered) {
            return Err(ChartError::Invalid(
                "Choose a rendered hitsound resource".into(),
            ));
        }
        if let Some(source) = &self.source {
            validate_hitsound(source)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlKind {
    Position,
    Size,
    Alpha,
    Y,
}
impl ControlKind {
    pub const ALL: [Self; 4] = [Self::Position, Self::Size, Self::Alpha, Self::Y];
    pub fn key(self) -> &'static str {
        match self {
            Self::Position => "posControl",
            Self::Size => "sizeControl",
            Self::Alpha => "alphaControl",
            Self::Y => "yControl",
        }
    }
    pub fn value_key(self) -> &'static str {
        match self {
            Self::Position => "pos",
            Self::Size => "size",
            Self::Alpha => "alpha",
            Self::Y => "y",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Position => "Position",
            Self::Size => "Size",
            Self::Alpha => "Alpha",
            Self::Y => "Y distance",
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ControlPoint {
    pub source: Option<usize>,
    pub x: f64,
    pub value: f64,
    pub easing: i32,
}
impl ChartDocument {
    pub fn note_sound_gain(&self, id: NoteId) -> Option<NoteSoundGain> {
        let raw = self.note_values(id.line)?.get(id.index)?;
        let gain: NoteSoundGain = serde_json::from_value(raw.get(SOUND_GAIN_KEY)?.clone()).ok()?;
        (gain.validate().is_ok()
            && raw.get("hitsound").and_then(Value::as_str) == Some(gain.rendered.as_str()))
        .then_some(gain)
    }
    pub fn note_hitsound(&self, id: NoteId) -> Result<Option<&str>, ChartError> {
        let raw = self
            .note_values(id.line)
            .and_then(|v| v.get(id.index))
            .ok_or_else(|| ChartError::Invalid("Note no longer exists".into()))?;
        Ok(raw
            .get("hitsound")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty()))
    }
    pub fn control_points(
        &self,
        line: usize,
        kind: ControlKind,
    ) -> Result<Vec<ControlPoint>, ChartError> {
        let raw = self.root["judgeLineList"]
            .get(line)
            .ok_or_else(|| ChartError::Invalid("Judge line no longer exists".into()))?;
        let values = match raw.get(kind.key()) {
            None | Some(Value::Null) => return Ok(vec![]),
            Some(Value::Array(values)) => values,
            _ => return Err(ChartError::Invalid("Control curve must be an array".into())),
        };
        values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                if !v.is_object() {
                    return Err(ChartError::Invalid("Control points must be objects".into()));
                }
                let point = ControlPoint {
                    source: Some(i),
                    x: v.get("x").and_then(Value::as_f64).unwrap_or(0.),
                    value: v
                        .get(kind.value_key())
                        .and_then(Value::as_f64)
                        .unwrap_or(1.),
                    easing: v.get("easing").and_then(Value::as_i64).unwrap_or(1) as i32,
                };
                validate_point(&point)?;
                Ok(point)
            })
            .collect()
    }
}
fn validate_point(point: &ControlPoint) -> Result<(), ChartError> {
    if !point.x.is_finite()
        || !point.value.is_finite()
        || !(point.value as f32).is_finite()
        || !(1..=29).contains(&point.easing)
    {
        return Err(ChartError::Invalid(
            "Control points need finite distance/value and easing 1–29".into(),
        ));
    }
    Ok(())
}
pub fn validate_hitsound(reference: &str) -> Result<String, ChartError> {
    let reference = reference.trim().replace('\\', "/");
    if reference.is_empty()
        || reference.len() > 1024
        || reference.starts_with('/')
        || reference.contains(':')
        || reference.contains('\0')
        || reference
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err(ChartError::Invalid(
            "Choose a project-relative hitsound path".into(),
        ));
    }
    Ok(reference)
}
pub fn builtin_hitsound(reference: &str) -> bool {
    matches!(reference, "tap.mp3" | "flick.mp3" | "drag.mp3")
}

#[derive(Debug)]
pub(crate) enum ExtraDelta {
    Sounds(Vec<(NoteId, FieldChange)>),
    Control { line: usize, field: FieldChange },
}
impl ExtraDelta {
    pub fn apply(&self, doc: &mut ChartDocument, forward: bool) {
        match self {
            Self::Sounds(fields) => {
                for (id, field) in fields {
                    field.apply(
                        doc.root["judgeLineList"][id.line]["notes"][id.index]
                            .as_object_mut()
                            .unwrap(),
                        forward,
                    );
                }
            }
            Self::Control { line, field } => field.apply(
                doc.root["judgeLineList"][*line].as_object_mut().unwrap(),
                forward,
            ),
        }
    }
}
pub(crate) fn sound_delta(
    doc: &ChartDocument,
    ids: Vec<NoteId>,
    reference: Option<String>,
) -> Result<Option<ExtraDelta>, ChartError> {
    let after = reference
        .map(|s| validate_hitsound(&s).map(Value::String))
        .transpose()?;
    let ids: BTreeSet<_> = ids.into_iter().collect();
    if ids.is_empty() {
        return Err(ChartError::Invalid("Select notes first".into()));
    }
    let mut fields = Vec::new();
    for id in ids {
        doc.note(id.line, id.index)?;
        let raw = &doc.root["judgeLineList"][id.line]["notes"][id.index];
        if !raw.is_object() {
            return Err(ChartError::Invalid("Note must be an object".into()));
        }
        let before = raw.get("hitsound").cloned();
        if before != after {
            fields.push((
                id,
                FieldChange {
                    key: "hitsound".into(),
                    before,
                    after: after.clone(),
                },
            ));
            if let Some(before) = raw.get(SOUND_GAIN_KEY) {
                fields.push((
                    id,
                    FieldChange {
                        key: SOUND_GAIN_KEY.into(),
                        before: Some(before.clone()),
                        after: None,
                    },
                ));
            }
        }
    }
    Ok((!fields.is_empty()).then_some(ExtraDelta::Sounds(fields)))
}
pub(crate) fn volume_delta(
    doc: &ChartDocument,
    id: NoteId,
    gain: Option<NoteSoundGain>,
) -> Result<Option<ExtraDelta>, ChartError> {
    doc.note(id.line, id.index)?;
    let (reference, metadata) = if let Some(gain) = gain {
        gain.validate()?;
        (
            Some(gain.rendered.clone()),
            Some(serde_json::to_value(gain)?),
        )
    } else if let Some(old) = doc.note_sound_gain(id) {
        (old.source, None)
    } else {
        return Ok(None);
    };
    let raw = &doc.root["judgeLineList"][id.line]["notes"][id.index];
    let fields = [
        ("hitsound", reference.map(Value::String)),
        (SOUND_GAIN_KEY, metadata),
    ]
    .into_iter()
    .filter_map(|(key, after)| {
        let before = raw.get(key).cloned();
        (before != after).then(|| {
            (
                id,
                FieldChange {
                    key: key.into(),
                    before,
                    after,
                },
            )
        })
    })
    .collect::<Vec<_>>();
    Ok((!fields.is_empty()).then_some(ExtraDelta::Sounds(fields)))
}
pub(crate) fn control_delta(
    doc: &ChartDocument,
    line: usize,
    kind: ControlKind,
    points: Option<Vec<ControlPoint>>,
) -> Result<Option<ExtraDelta>, ChartError> {
    let raw = doc.root["judgeLineList"]
        .get(line)
        .ok_or_else(|| ChartError::Invalid("Judge line no longer exists".into()))?;
    let before = raw.get(kind.key()).cloned();
    let after = if let Some(mut points) = points {
        if points.len() > 1024 {
            return Err(ChartError::Invalid(
                "Use at most 1,024 control points".into(),
            ));
        }
        let mut sources = BTreeSet::new();
        for point in &points {
            validate_point(point)?;
            if let Some(index) = point.source
                && !sources.insert(index)
            {
                return Err(ChartError::Invalid(
                    "A control point cannot reuse the same source twice".into(),
                ));
            }
        }
        points.sort_by(|a, b| a.x.total_cmp(&b.x));
        if points.windows(2).any(|p| p[0].x == p[1].x) {
            return Err(ChartError::Invalid(
                "Control distances must be distinct".into(),
            ));
        }
        // prpr recognizes two points starting at (value=1,easing=1) as the
        // legacy default sentinel. The first incoming easing has no segment;
        // use its otherwise unused value to serialize an authored active curve.
        if points.len() == 2
            && points[0].easing == 1
            && (points[0].value - 1.).abs() < 1e-4
            && points.iter().any(|p| p.value != 1.)
        {
            points[0].easing = 2;
        }
        let mut values = Vec::new();
        for point in points {
            let mut value = if let Some(source) = point.source {
                before
                    .as_ref()
                    .and_then(Value::as_array)
                    .and_then(|a| a.get(source))
                    .filter(|v| v.is_object())
                    .cloned()
                    .ok_or_else(|| ChartError::Invalid("Control point no longer exists".into()))?
            } else {
                json!({})
            };
            value["x"] = json!(point.x);
            value[kind.value_key()] = json!(point.value);
            value["easing"] = json!(point.easing);
            values.push(value);
        }
        Some(Value::Array(values))
    } else {
        None
    };
    if before == after {
        return Ok(None);
    }
    Ok(Some(ExtraDelta::Control {
        line,
        field: FieldChange {
            key: kind.key().into(),
            before,
            after,
        },
    }))
}
