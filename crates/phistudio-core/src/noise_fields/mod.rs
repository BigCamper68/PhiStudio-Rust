//! Noise Field authoring retains each source dialect and its unknown JSON fields.
mod authoring;
pub mod native;
mod next;
pub use authoring::{new_key, new_key_interval, resize_field, shift_key};
mod runtime;
mod transactions;
pub use runtime::{Field, curve};
pub mod export;
use crate::{Beat, ChartDocument, ChartError, TempoMap};
pub use next::{NextArea, Sample};
use serde_json::{Value, json};
pub(crate) use transactions::{Delta, prepare};

pub const LIST: &str = "blockAreaList";
pub const COLOR: &str = "phiStudioColor";
pub const COLOR_EVENTS: &str = "phiStudioColorEvents";
pub const DEFAULT_COLOR: [u8; 3] = [255, 84, 84];
pub const PHASES: [&str; 4] = ["appearTime", "enableTime", "disableTime", "disappearTime"];
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    Native,
    Next,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Track {
    #[default]
    Move,
    Scale,
    Rotate,
    Active,
    MoveX,
    MoveY,
    ScaleX,
    ScaleY,
    Color,
}
impl Track {
    pub fn key(self) -> &'static str {
        match self {
            Self::Move => "moveEvents",
            Self::Scale => "scaleEvents",
            Self::Rotate => "rotateEvents",
            Self::Active => "activeIntervals",
            Self::MoveX => "moveXEvents",
            Self::MoveY => "moveYEvents",
            Self::ScaleX => "scaleXEvents",
            Self::ScaleY => "scaleYEvents",
            Self::Color => COLOR_EVENTS,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Move => "Move",
            Self::Scale => "Scale",
            Self::Rotate => "Rotate",
            Self::Active => "Activation",
            Self::MoveX => "Move X",
            Self::MoveY => "Move Y",
            Self::ScaleX => "Scale X",
            Self::ScaleY => "Scale Y",
            Self::Color => "Color",
        }
    }
    pub fn all(format: Format) -> &'static [Self] {
        match format {
            Format::Native => &[Self::Move, Self::Scale, Self::Rotate, Self::Color],
            Format::Next => &[
                Self::Active,
                Self::MoveX,
                Self::MoveY,
                Self::Rotate,
                Self::ScaleX,
                Self::ScaleY,
                Self::Color,
            ],
        }
    }
    pub fn anchored(self) -> bool {
        matches!(
            self,
            Self::Scale | Self::Rotate | Self::ScaleX | Self::ScaleY
        )
    }
    pub fn native(self, format: Format) -> bool {
        format == Format::Native && self != Self::Color
    }
}
#[derive(Clone, Debug)]
pub enum Action {
    Insert {
        index: usize,
        value: Value,
    },
    Update {
        index: usize,
        value: Value,
    },
    Delete(usize),
    Move {
        from: usize,
        to: usize,
    },
    Key {
        field: usize,
        track: Track,
        index: Option<usize>,
        value: Option<Value>,
    },
    Options(Value),
}
pub(super) fn invalid(message: impl Into<String>) -> ChartError {
    ChartError::Invalid(message.into())
}
pub fn fields(doc: &ChartDocument) -> Result<&[Value], ChartError> {
    match doc.as_json().get(LIST) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(rows)) => Ok(rows),
        _ => Err(invalid("Noise Fields must be an array")),
    }
}
pub fn format(value: &Value) -> Result<Format, ChartError> {
    let native =
        value.get("bottomLeftPercentage").is_some() || value.get("topRightPercentage").is_some();
    let next = value.get("bottomLeft").is_some() || value.get("topRight").is_some();
    match (native, next) {
        (true, false) => Ok(Format::Native),
        (false, true) => Ok(Format::Next),
        _ => Err(invalid(
            "Choose either native or rpe-next Noise Field geometry",
        )),
    }
}
pub fn color(value: &Value) -> [u8; 3] {
    rgb(&value[COLOR]).unwrap_or(DEFAULT_COLOR)
}
pub fn rgb(value: &Value) -> Option<[u8; 3]> {
    let a = value.as_array().filter(|a| a.len() == 3)?;
    Some([
        a[0].as_u64()?.try_into().ok()?,
        a[1].as_u64()?.try_into().ok()?,
        a[2].as_u64()?.try_into().ok()?,
    ])
}
pub fn name(value: &Value) -> &str {
    let key = if value.get("bottomLeft").is_some() {
        "Name"
    } else {
        "phiStudioName"
    };
    value[key]
        .as_str()
        .or_else(|| value["phiStudioName"].as_str())
        .unwrap_or("")
}
pub fn keys(value: &Value, track: Track) -> Result<&[Value], ChartError> {
    match value.get(track.key()) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(rows)) => Ok(rows),
        _ => Err(invalid(format!("{} must be an array", track.key()))),
    }
}
pub fn time(value: &Value, key: &str, format: Format, tempo: &TempoMap) -> Result<f64, ChartError> {
    if format == Format::Native {
        number(&value[key])
    } else {
        Ok(tempo.seconds_at(serde_json::from_value::<Beat>(value[key].clone())?.as_f64()))
    }
}
pub fn beat(
    value: &Value,
    key: &str,
    format: Format,
    tempo: &TempoMap,
) -> Result<Beat, ChartError> {
    if format == Format::Native {
        Beat::from_grid(tempo.beat_at(number(&value[key])?), 1_000_000_000)
    } else {
        Ok(serde_json::from_value(value[key].clone())?)
    }
}
pub fn put_beat(
    value: &mut Value,
    key: &str,
    beat: Beat,
    format: Format,
    tempo: &TempoMap,
) -> Result<(), ChartError> {
    // Unchanged UI values must not round native seconds or imported beat triples.
    if self::beat(value, key, format, tempo).ok() != Some(beat) {
        value[key] = if format == Format::Native {
            json!(tempo.seconds_at(beat.as_f64()))
        } else {
            json!(beat)
        };
    }
    Ok(())
}
pub fn number(value: &Value) -> Result<f64, ChartError> {
    value
        .as_f64()
        .filter(|v| v.is_finite() && (*v as f32).is_finite())
        .ok_or_else(|| invalid("Noise Field numbers must be finite"))
}
pub fn bounds(value: &Value) -> Result<[[f64; 2]; 2], ChartError> {
    let names = if format(value)? == Format::Native {
        ["bottomLeftPercentage", "topRightPercentage"]
    } else {
        ["bottomLeft", "topRight"]
    };
    Ok([point(&value[names[0]])?, point(&value[names[1]])?])
}
fn point(value: &Value) -> Result<[f64; 2], ChartError> {
    Ok([number(&value["x"])?, number(&value["y"])?])
}
pub fn center(value: &Value) -> Result<[f64; 2], ChartError> {
    let [a, b] = bounds(value)?;
    Ok([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5])
}
pub fn create(start: Beat, end: Beat, format: Format, subtract: bool, tempo: &TempoMap) -> Value {
    let mut value = if format == Format::Native {
        let (a, b) = (
            tempo.seconds_at(start.as_f64()),
            tempo.seconds_at(end.as_f64()),
        );
        json!({"bottomLeftPercentage":{"x":0.25,"y":0.25},"topRightPercentage":{"x":0.75,"y":0.75},"appearTime":a,"enableTime":a+(b-a).min(1.)*0.5,"disableTime":b,"disappearTime":b,"isSubtract":subtract,"moveEvents":[],"scaleEvents":[],"rotateEvents":[]})
    } else {
        json!({"Name":"","bottomLeft":{"x":-250.,"y":-150.},"topRight":{"x":250.,"y":150.},"appearTime":start,"disappearTime":end,"isInvert":subtract,"activeIntervals":[{"startTime":start,"endTime":end,"readyFlash":0.5}],"moveXEvents":[],"moveYEvents":[],"rotateEvents":[],"scaleXEvents":[],"scaleYEvents":[]})
    };
    if format == Format::Native {
        value["phiStudioName"] = json!("");
    }
    value
}
pub fn shifted(field: &Value, from: Beat, to: Beat, tempo: &TempoMap) -> Result<Value, ChartError> {
    let format = format(field)?;
    if from == to {
        return Ok(field.clone());
    }
    let mut result = field.clone();
    let delta = to.checked_sub(from)?;
    for key in if format == Format::Native {
        &PHASES[..]
    } else {
        &["appearTime", "disappearTime"][..]
    } {
        if format == Format::Native {
            result[*key] = json!(
                number(&field[*key])? + tempo.seconds_at(to.as_f64())
                    - tempo.seconds_at(from.as_f64())
            );
        } else {
            result[*key] =
                json!(serde_json::from_value::<Beat>(field[*key].clone())?.checked_add(delta)?);
        }
    }
    for track in Track::all(format) {
        if let Some(rows) = result[track.key()].as_array_mut() {
            for row in rows {
                if track.native(format) {
                    row["time"] = json!(
                        number(&row["time"])? + tempo.seconds_at(to.as_f64())
                            - tempo.seconds_at(from.as_f64())
                    );
                } else {
                    for key in ["startTime", "endTime"] {
                        row[key] = json!(
                            serde_json::from_value::<Beat>(row[key].clone())?.checked_add(delta)?
                        );
                    }
                }
            }
        }
    }
    validate(&result)?;
    Ok(result)
}
pub fn validate(field: &Value) -> Result<(), ChartError> {
    let f = format(field)?;
    bounds(field)?;
    if field.get(COLOR).is_some_and(|v| rgb(v).is_none()) {
        return Err(invalid("Noise Field color must be three RGB bytes"));
    }
    for key in ["phiStudioName", "Name"] {
        if field
            .get(key)
            .is_some_and(|v| v.as_str().is_none_or(|s| s.len() > 4096))
        {
            return Err(invalid("Noise Field name must be text up to 4096 bytes"));
        }
    }
    let boolean = if f == Format::Native {
        "isSubtract"
    } else {
        "isInvert"
    };
    if field.get(boolean).is_some_and(|v| !v.is_boolean()) {
        return Err(invalid("Noise Field subtraction must be boolean"));
    }
    if f == Format::Native {
        for key in PHASES {
            number(&field[key])?;
        }
        if number(&field["appearTime"])? > number(&field["disappearTime"])?
            || number(&field["enableTime"])? > number(&field["disableTime"])?
        {
            return Err(invalid("Noise Field phase endpoints are reversed"));
        }
    } else {
        let a: Beat = serde_json::from_value(field["appearTime"].clone())?;
        let b: Beat = serde_json::from_value(field["disappearTime"].clone())?;
        if a > b {
            return Err(invalid("Noise Field lifetime endpoints are reversed"));
        }
    }
    for track in Track::all(f) {
        let rows = keys(field, *track)?;
        if rows.len() > 200_000 {
            return Err(invalid("Noise Field track exceeds 200000 keys"));
        }
        for key in rows {
            validate_key(key, *track, f)?;
        }
        if f == Format::Next || *track == Track::Color {
            let mut spans: Vec<_> = rows
                .iter()
                .map(|v| {
                    Ok((
                        serde_json::from_value::<Beat>(v["startTime"].clone())?,
                        serde_json::from_value::<Beat>(v["endTime"].clone())?,
                        v,
                    ))
                })
                .collect::<Result<_, ChartError>>()?;
            spans.sort_by_key(|v| v.0);
            for pair in spans.windows(2) {
                if pair[1].0 < pair[0].1
                    && !(track.anchored()
                        && pair[0].0 == pair[0].1
                        && pair[0].2["transformStep"] == true)
                {
                    return Err(invalid("Noise Field intervals in one track cannot overlap"));
                }
            }
        }
    }
    Ok(())
}
fn validate_key(key: &Value, track: Track, format: Format) -> Result<(), ChartError> {
    if !key.is_object() {
        return Err(invalid("Noise Field key must be an object"));
    }
    if track.native(format) {
        number(&key["time"])?;
        if track == Track::Rotate {
            number(&key["rotation"])?;
        } else {
            point(
                &key[if track == Track::Move {
                    "endPosition"
                } else {
                    "scale"
                }],
            )?;
        }
        for k in if track == Track::Rotate {
            &["easeType"][..]
        } else {
            &["easeTypeX", "easeTypeY"][..]
        } {
            let e = key[*k].as_i64().unwrap_or(0);
            if !(0..=14).contains(&e) {
                return Err(invalid("Native Noise Field easing must be 0..14"));
            }
        }
    } else {
        let a: Beat = serde_json::from_value(key["startTime"].clone())?;
        let b: Beat = serde_json::from_value(key["endTime"].clone())?;
        if a > b {
            return Err(invalid("Noise Field key endpoints are reversed"));
        }
        if track == Track::Active {
            if let Some(v) = key.get("readyFlash")
                && !v.is_boolean()
            {
                number(v)?;
            }
        } else {
            for k in ["start", "end"] {
                if track == Track::Color {
                    if rgb(&key[k]).is_none() {
                        return Err(invalid("Noise Field color keys require RGB values"));
                    }
                } else {
                    number(&key[k])?;
                }
            }
            let e = key["easingType"].as_i64().unwrap_or(1);
            if !(1..=29).contains(&e) {
                return Err(invalid("RPE easing must be 1..29"));
            }
            let (l, r) = (
                key["easingLeft"].as_f64().unwrap_or(0.),
                key["easingRight"].as_f64().unwrap_or(1.),
            );
            if !l.is_finite() || !r.is_finite() || l < 0. || r > 1. || l >= r {
                return Err(invalid("Use an easing window within 0..1"));
            }
            if flag(&key["bezier"]) {
                let p = key["bezierPoints"]
                    .as_array()
                    .filter(|p| p.len() == 4)
                    .ok_or_else(|| invalid("Bezier requires four control values"))?;
                for v in p {
                    number(v)?;
                }
                for i in [0, 2] {
                    if !(0. ..=1.).contains(&number(&p[i])?) {
                        return Err(invalid("Bezier control X must be within 0..1"));
                    }
                }
            }
            if flag(&key["inst"]) && key["start"] != key["end"] {
                return Err(invalid("Instant key endpoints must match"));
            }
            for k in ["transformStart", "transformEnd"] {
                if let Some(v) = key.get(k) {
                    number(v)?;
                }
            }
            if flag(&key["transformStep"]) && a != b {
                return Err(invalid("Transform steps must have zero duration"));
            }
        }
    }
    if track.anchored() && (format == Format::Native || key.get("anchor").is_some()) {
        point(&key["anchor"])?;
    }
    Ok(())
}
pub fn flag(value: &Value) -> bool {
    value.as_bool().unwrap_or_else(|| value.as_i64() == Some(1))
}
