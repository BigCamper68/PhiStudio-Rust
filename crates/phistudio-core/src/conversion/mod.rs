//! Editable RPE conversion follows the pinned Java PhiStudio converters.
//! Existing RPE documents retain their original tree, including unknown fields.
mod official;
mod pec;
use crate::{Beat, ChartDocument, ChartError, Note, NoteKind};
pub use pec::pec_height_signature;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(crate) const PRECISION: i32 = 1_000_000;
pub(crate) const MIN_DURATION: f64 = 1. / PRECISION as f64;
pub(crate) const TRACKS: [&str; 5] = [
    "moveXEvents",
    "moveYEvents",
    "rotateEvents",
    "alphaEvents",
    "speedEvents",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ChartFormat {
    #[default]
    Rpe,
    OfficialPhigros,
    LegacyPec,
}
impl ChartFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rpe => "RPE",
            Self::OfficialPhigros => "Phigros",
            Self::LegacyPec => "PEC",
        }
    }
}
pub struct DecodedChart {
    pub document: ChartDocument,
    pub format: ChartFormat,
}
fn invalid(message: impl Into<String>) -> ChartError {
    ChartError::Invalid(message.into())
}

pub fn decode_chart(source: &str) -> Result<DecodedChart, ChartError> {
    if source.len() > ChartDocument::MAX_JSON_BYTES {
        return Err(invalid("Chart exceeds 256 MiB"));
    }
    let source = source
        .trim_start()
        .trim_start_matches('\u{feff}')
        .trim_start();
    let (document, format) = if source.starts_with('{') {
        let root: Value = serde_json::from_str(source)?;
        if root.get("META").is_some_and(Value::is_object)
            && root.get("BPMList").is_some_and(Value::is_array)
        {
            (ChartDocument::from_value(root)?, ChartFormat::Rpe)
        } else if root.get("formatVersion").is_some()
            && root.get("judgeLineList").is_some_and(Value::is_array)
        {
            (official::convert(root)?, ChartFormat::OfficialPhigros)
        } else {
            return Err(invalid(
                "Unsupported chart JSON: expected RPE or Phigros formatVersion 1 or 3",
            ));
        }
    } else {
        (pec::convert(source)?, ChartFormat::LegacyPec)
    };
    Ok(DecodedChart { document, format })
}

pub(crate) fn looks_like_chart(prefix: &[u8]) -> bool {
    let text = String::from_utf8_lossy(prefix);
    let text = text
        .trim_start()
        .trim_start_matches('\u{feff}')
        .trim_start();
    if text.starts_with('{') {
        return true;
    }
    let mut lines = text.lines();
    if !lines
        .next()
        .is_some_and(|s| s.trim().parse::<f32>().is_ok_and(f32::is_finite))
    {
        return false;
    }
    let mut bpm = false;
    let mut command = false;
    for line in lines.take(256) {
        match line.split_whitespace().next().unwrap_or("") {
            "bp" => bpm = true,
            "n1" | "n2" | "n3" | "n4" | "cp" | "cm" | "cd" | "cr" | "ca" | "cf" | "cv" => {
                command = true
            }
            _ => {}
        }
    }
    bpm && command
}

#[derive(Clone)]
struct Event {
    start_time: Beat,
    end_time: Beat,
    start: f64,
    end: f64,
    easing: u8,
    normalized: Option<f32>,
}
impl Event {
    fn json(&self, speed: bool) -> Value {
        let mut value = json!({"startTime":self.start_time,"endTime":self.end_time,"start":self.start,"end":self.end,"linkgroup":0});
        if speed {
            if let Some(normalized) = self.normalized {
                value["phiPecSpeed"] =
                    json!({"normalized":normalized as f64,"start":self.start,"end":self.end});
            }
        } else {
            value.as_object_mut().unwrap().extend(json!({"easingType":self.easing,"easingLeft":0.,"easingRight":1.,"bezier":0,"bezierPoints":[0.,0.,0.,0.]}).as_object().unwrap().clone());
        }
        value
    }
}
#[derive(Default)]
struct Line {
    notes: Vec<Note>,
    tracks: [Vec<Event>; 5],
    width: Option<f64>,
}
impl Line {
    fn json(mut self, index: usize) -> Value {
        self.notes.sort_by_key(|n| n.start_time);
        let mut layer = serde_json::Map::new();
        for (index, mut events) in self.tracks.into_iter().enumerate() {
            if index != 4 {
                events.sort_by_key(|e| (e.start_time, e.end_time));
            }
            if !events.is_empty() {
                layer.insert(
                    TRACKS[index].into(),
                    events.iter().map(|e| e.json(index == 4)).collect(),
                );
            }
        }
        let mut line = json!({"Group":0,"Name":format!("Line {index}"),"Texture":"line.png","bpmfactor":1.,"father":-1,"isCover":1,"zOrder":0,
            "numOfNotes":self.notes.iter().filter(|n| n.kind != NoteKind::Hold).count(),"notes":self.notes,"eventLayers":[layer]});
        if let Some(width) = self.width {
            let event = Event {
                start_time: Beat::ZERO,
                end_time: Beat::ONE,
                start: width,
                end: width,
                easing: 1,
                normalized: None,
            };
            line["extended"] = json!({"scaleXEvents":[event.json(false)]});
        }
        line
    }
}
fn root(bpm: f64, offset: i32) -> Result<Value, ChartError> {
    let mut root = ChartDocument::empty("Untitled", bpm)?.as_json().clone();
    root["META"]["offset"] = json!(offset);
    root["META"]["id"] = json!("");
    root["META"]["backgroundDim"] = json!(0.6);
    Ok(root)
}
fn close(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-8 * left.abs().max(right.abs()).max(1.)
}
fn merged(target: &mut Vec<Event>, event: Event) {
    if let Some(previous) = target.last_mut() {
        let a = previous.end_time.as_f64() - previous.start_time.as_f64();
        let b = event.end_time.as_f64() - event.start_time.as_f64();
        let slope_a = if a <= 0. {
            0.
        } else {
            (previous.end - previous.start) / a
        };
        let slope_b = if b <= 0. {
            0.
        } else {
            (event.end - event.start) / b
        };
        if previous.easing == 1
            && event.easing == 1
            && close(previous.end_time.as_f64(), event.start_time.as_f64())
            && close(previous.end, event.start)
            && close(slope_a, slope_b)
        {
            previous.end_time = event.end_time;
            previous.end = event.end;
            return;
        }
    }
    target.push(event);
}
fn finite(value: f64, label: &str) -> Result<f64, ChartError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(invalid(format!("{label} must be finite")))
    }
}
fn beat(value: f64) -> Result<Beat, ChartError> {
    Beat::from_grid(value, PRECISION)
}
