//! Compile authoring geometry into portable native RPE interpolation intervals.
use super::*;
use crate::{Beat, EventType, LineEvent, TempoMap};
use serde_json::Value;

pub(super) fn bake(
    source: &Value,
    kind: EventType,
    curve: &Curve,
    tempo: &TempoMap,
    next_start: Option<Beat>,
    canceled: &impl Fn() -> bool,
) -> Result<Vec<Value>, ChartError> {
    let original = LineEvent::read(kind, source)?;
    original.validate(kind)?;
    let mut segments = Vec::new();
    let delta = original.end - original.start;
    if !delta.is_finite()
        || !(original.start as f32).is_finite()
        || !(original.end as f32).is_finite()
    {
        return Err(invalid("Easing values exceed native playback precision"));
    }
    for segment in curve.segments()? {
        compile(segment, delta, &mut segments, canceled, 0)?;
    }
    let begin = tempo.seconds_at(original.start_time.as_f64());
    let duration = tempo.seconds_at(original.end_time.as_f64()) - begin;
    let mut result = Vec::with_capacity(segments.len() + 1);
    let beat = |x: f64| {
        if x <= 0. {
            Ok(original.start_time)
        } else if x >= 1. {
            Ok(original.end_time)
        } else {
            Beat::from_grid(tempo.beat_at(begin + x * duration), 1_000_000_000)
        }
    };
    for segment in segments {
        if canceled() {
            return Err(invalid("Custom easing preparation canceled"));
        }
        let [a, _, _, d] = segment.points;
        let event = LineEvent {
            start_time: beat(a[0])?,
            end_time: beat(d[0])?,
            start: original.start + delta * a[1],
            end: original.start + delta * d[1],
            constant: original.constant && original.start == original.end,
            link_group: original.link_group,
            ..Default::default()
        };
        if event.end_time <= event.start_time {
            return Err(invalid(
                "Easing nodes are too close for native chart timing",
            ));
        }
        result.push(merge(source, kind, &event)?);
    }
    // A final Hold jumps to the locked (1,1) endpoint. Native RPE has no
    // zero-duration authoring event, so retain that right-hand value in the gap.
    // An untouched event beginning exactly at this boundary keeps its priority.
    if curve.nodes[curve.nodes.len() - 2].mode == Mode::Hold
        && curve.nodes[curve.nodes.len() - 2].y != 1.
        && delta != 0.
        && next_start.is_none_or(|b| b > original.end_time)
    {
        let end = next_start.unwrap_or(original.end_time.checked_add(Beat::ONE)?);
        let event = LineEvent {
            start_time: original.end_time,
            end_time: end,
            start: original.end,
            end: original.end,
            constant: original.constant,
            link_group: original.link_group,
            ..Default::default()
        };
        result.push(merge(source, kind, &event)?);
    }
    Ok(result)
}
fn merge(source: &Value, kind: EventType, event: &LineEvent) -> Result<Value, ChartError> {
    let mut raw = event.merge(kind, Some(source))?;
    let object = raw
        .as_object_mut()
        .ok_or_else(|| invalid("Event must be an object"))?;
    object.shift_remove(RECIPE_KEY);
    object.shift_remove("customEasing");
    if kind == EventType::Speed {
        // Explicitly replace imported curve metadata when authoring a new curve.
        for key in [
            "easingType",
            "easingLeft",
            "easingRight",
            "bezier",
            "bezierPoints",
        ] {
            object.shift_remove(key);
        }
    }
    Ok(raw)
}
// Linear intervals avoid the original parser's quantized Bezier-cache keys,
// which can alias different normalized control points within the same chart.
// The editable curve itself stays lossless in its authoring recipe.
fn compile(
    segment: Segment,
    delta: f64,
    out: &mut Vec<Segment>,
    canceled: &impl Fn() -> bool,
    depth: usize,
) -> Result<(), ChartError> {
    if canceled() {
        return Err(invalid("Custom easing preparation canceled"));
    }
    if out.len() >= crate::tools::MAX_OBJECTS {
        return Err(invalid("The custom easing exceeds 20,000 events"));
    }
    if delta == 0.
        || segment.linear
        || segment.hold
        || segment.chord_error() * delta.abs() <= super::BAKE_TOLERANCE
    {
        out.push(segment);
        return Ok(());
    }
    if depth >= 32 {
        return Err(invalid(
            "The custom easing is too detailed for native intervals",
        ));
    }
    let (a, b) = segment.split(0.5);
    compile(a, delta, out, canceled, depth + 1)?;
    compile(b, delta, out, canceled, depth + 1)
}
