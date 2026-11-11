//! Distance graphs sample upstream prpr height animations, including both speed modes.
pub(crate) mod clock;
mod fitting;
use crate::ControlCurve;
use anyhow::{Result, anyhow, ensure};
use phistudio_core::{
    Beat, BpmChange, ChartDocument, EventType, LineEvent, NoteKind, TempoMap, speed_curve::*,
};
use prpr::core::{AnimFloat, ChartExtra};
use serde_json::{Value, json};
#[derive(Clone, Debug)]
pub struct SpeedCurveProfile {
    pub range: [Beat; 2],
    pub points: Vec<SpeedCurvePoint>,
    pub samples: Vec<[f64; 2]>,
    pub judgment: f64,
    pub judgment_distance: f64,
    pub bpm_markers: Vec<f64>,
    pub speeds: Vec<(Beat, f64, f64)>,
}
struct NativeModel {
    source: SpeedCurveSource,
    modern: bool,
    tempo: TempoMap,
    height: Vec<AnimFloat>,
    control: ControlCurve,
    hit_floor: f64,
}
impl NativeModel {
    async fn new(source: &SpeedCurveSource, modern: bool, limit: Beat) -> Result<Self> {
        ensure!(
            source.note.speed.abs() <= 1e7 && source.note.y_offset.abs() <= 1e9,
            "Note speed or Y offset exceeds the editable graph range"
        );
        let changes: Vec<BpmChange> = serde_json::from_value(source.bpm.clone())?;
        let tempo = TempoMap::new(changes)?;
        let mut root = ChartDocument::empty("Speed curve", 120.)?.as_json().clone();
        root["BPMList"] = source.bpm.clone();
        root["META"] = source.meta.clone();
        let mut line = root["judgeLineList"][0].clone();
        line["notes"] = json!([]);
        line["father"] = json!(-1);
        let end = if let Some(horizon) = source.native_horizon {
            ensure!(
                horizon.is_finite() && horizon >= 1.,
                "The native preview clock is invalid"
            );
            Beat::from_grid(tempo.beat_at(horizon - 1.), 1_000_000_000)?
        } else {
            limit.max(source.note.end_time).checked_add(Beat::ONE)?
        };
        let sentinel =
            json!({"startTime":[0,0,1],"endTime":end,"start":255,"end":255,"easingType":1});
        let mut height = Vec::new();
        for events in &source.layers {
            let mut alpha = vec![sentinel.clone()];
            // The upstream Bezier registry is populated by non-speed tracks.
            // Register speed controls in this private, visually unused parser copy.
            for event in events.iter().filter(|v| {
                v["bezier"].as_u64().unwrap_or(0) != 0
                    && v.get(phistudio_core::custom_easing::CURVE_KEY)
                        .is_none_or(Value::is_null)
            }) {
                let mut definition = sentinel.clone();
                definition["bezier"] = event["bezier"].clone();
                definition["bezierPoints"] = event["bezierPoints"].clone();
                alpha.push(definition);
            }
            line["eventLayers"] = json!([{"speedEvents":events,"alphaEvents":alpha}]);
            root["judgeLineList"] = json!([line]);
            let mut fs = crate::assets::ChartAssets::new(None)?;
            let projected = ChartDocument::from_value(root.clone())?.render_json()?;
            let mut parsed =
                prpr::parse::parse_rpe(&projected, &mut fs, ChartExtra::default(), modern).await?;
            height.push(parsed.lines.remove(0).height);
        }
        let mut model = Self {
            source: source.clone(),
            modern,
            tempo,
            height,
            control: ControlCurve::new(&source.control),
            hit_floor: 0.,
        };
        model.hit_floor = model.floor(source.note.start_time.as_f64());
        Ok(model)
    }
    fn speed(&self, layer: usize, beat: f64) -> f64 {
        let time = self.tempo.seconds_at(beat);
        let mut current = Some(&self.height[layer]);
        let mut sum = 0.;
        while let Some(anim) = current {
            let index = anim
                .keyframes
                .partition_point(|k| k.time <= time)
                .saturating_sub(1);
            if let (Some(a), Some(b)) = (anim.keyframes.get(index), anim.keyframes.get(index + 1)) {
                let dt = b.time - a.time;
                if dt > 0. {
                    let fraction = ((time - a.time) / dt).clamp(0., 1.) as f32;
                    let value =
                        f64::from(b.value - a.value) * f64::from(a.tween.derivative(fraction)) / dt;
                    if value.is_finite() {
                        sum += value;
                    }
                }
            }
            current = anim.next.as_deref();
        }
        sum * 4.5 * prpr::core::HEIGHT_RATIO
    }
    fn layer_floor(&mut self, layer: usize, beat: f64) -> f64 {
        let time = self.tempo.seconds_at(beat);
        self.height[layer].set_time(time);
        f64::from(self.height[layer].now()) * 450.
    }
    fn floor(&mut self, beat: f64) -> f64 {
        (0..self.height.len())
            .map(|layer| self.layer_floor(layer, beat))
            .sum()
    }
    fn raw(&mut self, beat: f64) -> f64 {
        self.hit_floor - self.floor(beat) + self.source.note.y_offset
    }
    fn project(&mut self, raw: f64) -> f64 {
        let control = f64::from(self.control.sample(raw));
        // Note::render applies Y Control to the floor distance; translation
        // (the note's Y offset) remains a separate, speed-scaled term.
        let offset = self.source.note.y_offset;
        ((raw - offset) * control + offset)
            * self.source.note.speed
            * if self.source.note.above == 1 { 1. } else { -1. }
    }
    fn distance(&mut self, beat: f64) -> f64 {
        let raw = if self.source.note.kind == NoteKind::Hold
            && beat >= self.source.note.start_time.as_f64()
        {
            self.source.note.y_offset
        } else {
            self.raw(beat)
        };
        self.project(raw)
    }
    fn inverse(&mut self, distance: f64, beat: f64) -> Result<f64> {
        ensure!(
            self.source.note.speed.abs() > 1e-10,
            "A note with zero speed has no editable distance curve"
        );
        let mut raw = self.raw(beat);
        for _ in 0..40 {
            let error = self.project(raw) - distance;
            if error.abs() < 0.001 {
                return Ok(raw);
            }
            let step = (raw.abs() * 1e-5).max(0.01);
            let derivative = (self.project(raw + step) - self.project(raw - step)) / (2. * step);
            ensure!(
                derivative.is_finite() && derivative.abs() > 1e-10,
                "Y control is not invertible at this distance"
            );
            raw -= (error / derivative).clamp(-1e6, 1e6);
            ensure!(
                raw.is_finite() && raw.abs() < 1e9,
                "The requested distance cannot be represented"
            );
        }
        Err(anyhow!("Y control cannot produce the requested distance"))
    }
}
pub async fn speed_curve_profile(
    source: &SpeedCurveSource,
    layer: usize,
    range: [Beat; 2],
    modern: bool,
) -> Result<SpeedCurveProfile> {
    ensure!(
        layer < 4 && layer < source.layers.len() && range[0] >= Beat::ZERO && range[1] > range[0],
        "Choose an ordered speed curve range and layer 0–3"
    );
    let mut native = NativeModel::new(source, modern, range[1]).await?;
    let mut times = std::collections::BTreeSet::from([range[0], range[1]]);
    let recipes = source.saved_recipes(layer);
    let context = format!("{}:{modern}", source.curve_context(layer));
    for recipe in &recipes {
        times.extend(recipe.points.iter().map(|p| p.beat));
    }
    let mut track = source.layers[layer]
        .iter()
        .map(|value| Ok((LineEvent::read(EventType::Speed, value)?, value)))
        .collect::<Result<Vec<_>>>()?;
    track.sort_by_key(|(event, _)| event.start_time);
    let mut previous = None;
    for (event, value) in track {
        let declared = |beat: Beat| {
            recipes.iter().all(|r| {
                beat < r.range[0] || beat > r.range[1] || r.points.iter().any(|p| p.beat == beat)
            })
        };
        let transition = previous.is_some_and(|end: f64| (end - event.start).abs() > 1e-7);
        if declared(event.start_time)
            && (transition
                || event.start != event.end
                || value.get("phiStudioSpeedNode").and_then(Value::as_bool) == Some(true))
        {
            times.insert(event.start_time);
        }
        if declared(event.end_time) && event.start != event.end {
            times.insert(event.end_time);
        }
        previous = Some(event.end);
    }
    let changes: Vec<BpmChange> = serde_json::from_value(source.bpm.clone())?;
    let bpm_markers = changes.into_iter().map(|p| p.start.as_f64()).collect();
    times.insert(source.note.start_time);
    let times = times
        .into_iter()
        .filter(|t| *t >= range[0] && *t <= range[1])
        .collect::<Vec<_>>();
    ensure!(
        times.len() <= MAX_CURVE_POINTS,
        "The visible range has too many speed boundaries; choose a smaller range"
    );
    let points: Vec<SpeedCurvePoint> = times
        .into_iter()
        .map(|beat| {
            let easing = recipes
                .iter()
                .filter(|r| r.context == context)
                .find_map(|r| {
                    r.points
                        .iter()
                        .find(|p| p.beat == beat)
                        .and_then(|p| p.easing)
                });
            SpeedCurvePoint {
                beat,
                source_beat: beat,
                distance: native.distance(beat.as_f64()),
                easing,
                fixed: beat == range[0]
                    || beat == range[1]
                    || beat == source.note.start_time
                    || (source.note.kind == NoteKind::Hold && beat > source.note.start_time),
            }
        })
        .collect();
    let samples: Vec<[f64; 2]> = (0..=512)
        .map(|i| {
            let beat =
                range[0].as_f64() + (range[1].as_f64() - range[0].as_f64()) * i as f64 / 512.;
            [beat, native.distance(beat)]
        })
        .collect();
    ensure!(
        samples
            .iter()
            .all(|p| p[1].is_finite() && p[1].abs() <= 1e9)
            && points
                .iter()
                .all(|p| p.distance.is_finite() && p.distance.abs() <= 1e9),
        "The curve exceeds the editable distance range; choose a smaller work range"
    );
    let speeds = points
        .iter()
        .map(|p| {
            (
                p.beat,
                (0..native.height.len())
                    .map(|i| native.speed(i, p.beat.as_f64()))
                    .sum(),
                native.speed(layer, p.beat.as_f64()),
            )
        })
        .collect();
    let judgment_distance = native.distance(source.note.start_time.as_f64());
    Ok(SpeedCurveProfile {
        range,
        points,
        samples,
        judgment: source.note.start_time.as_f64(),
        judgment_distance,
        bpm_markers,
        speeds,
    })
}
pub use fitting::fit_speed_curve;

#[cfg(test)]
mod tests;
