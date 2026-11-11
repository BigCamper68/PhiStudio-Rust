//! Compile authoring geometry into portable native RPE interpolation intervals.
use super::*;
use crate::{Beat, EventType, LineEvent, TempoMap};
use serde_json::Value;

pub(super) fn bake_bound(
    source: &Value,
    kind: EventType,
    tempo: &TempoMap,
    next_start: Option<Beat>,
    canceled: &impl Fn() -> bool,
) -> Result<Vec<Value>, ChartError> {
    let original = LineEvent::read(kind, source)?;
    original.validate(kind)?;
    let binding = original
        .custom_easing
        .as_ref()
        .ok_or_else(|| invalid("The event has no custom easing"))?;
    let [base, end] = binding.scalar_source([original.start, original.end])?;
    let delta = end - base;
    if !delta.is_finite() || !(base as f32).is_finite() || !(end as f32).is_finite() {
        return Err(invalid("Easing values exceed native playback precision"));
    }
    let mut segments = Vec::new();
    let [from, to] = binding.domain;
    let (lo, hi) = (from.min(to), from.max(to));
    let mut clipped = Vec::new();
    for segment in binding.curve.segments()? {
        let left = lo.max(segment.points[0][0]);
        let right = hi.min(segment.points[3][0]);
        if left >= right {
            continue;
        }
        let mut part = segment.slice(segment.parameter_at(left), segment.parameter_at(right))?;
        part.points[0][0] = left;
        part.points[3][0] = right;
        if to < from {
            part.points.reverse();
        }
        for point in &mut part.points {
            point[0] = ((point[0] - from) / (to - from)).clamp(0., 1.);
        }
        clipped.push(part);
    }
    if to < from {
        clipped.reverse();
    }
    for part in clipped {
        compile(part, delta, &mut segments, canceled, 0)?;
    }
    let terminal_value = segments
        .last()
        .map(|s| base + delta * s.points[3][1])
        .ok_or_else(|| invalid("The custom easing source domain is empty"))?;
    let begin = tempo.seconds_at(original.start_time.as_f64());
    let duration = tempo.seconds_at(original.end_time.as_f64()) - begin;
    let mut result = Vec::with_capacity(segments.len() + 1);
    // Keep the full editable curve once, rather than cloning/decoding it into
    // every native fragment. Unknown source fields still survive in the template.
    let mut template = source.clone();
    if let Some(object) = template.as_object_mut() {
        object.shift_remove(CURVE_KEY);
        object.shift_remove(RECIPE_KEY);
    }
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
            start: base + delta * a[1],
            end: base + delta * d[1],
            constant: original.constant && original.start == original.end,
            link_group: original.link_group,
            ..Default::default()
        };
        if event.end_time <= event.start_time {
            return Err(invalid(
                "Easing nodes are too close for native chart timing",
            ));
        }
        result.push(merge(&template, kind, &event)?);
    }
    // A final Hold needs an instantaneous native value key at its endpoint.
    // It belongs only to the projection, not the positive-duration authoring
    // model. Keeping its end time unchanged also retains prpr's terminal clock.
    // An untouched event beginning exactly at this boundary keeps its priority.
    if (terminal_value - original.end).abs() > 1e-9
        && next_start.is_none_or(|b| b > original.end_time)
    {
        let event = LineEvent {
            start_time: original.end_time,
            end_time: original.end_time,
            start: original.end,
            end: original.end,
            constant: original.constant,
            link_group: original.link_group,
            ..Default::default()
        };
        result.push(merge(&template, kind, &event)?);
    }
    Ok(result)
}
fn merge(source: &Value, kind: EventType, event: &LineEvent) -> Result<Value, ChartError> {
    if !(event.start as f32).is_finite() || !(event.end as f32).is_finite() {
        return Err(invalid("Easing values exceed native playback precision"));
    }
    let mut checked = event.clone();
    if checked.start_time == checked.end_time {
        checked.start_time = Beat::ZERO;
        checked.end_time = Beat::ONE;
    }
    let mut raw = checked.merge(
        if kind == EventType::Alpha {
            EventType::MoveX
        } else {
            kind
        },
        Some(source),
    )?;
    if event.start_time == event.end_time {
        raw["startTime"] = serde_json::to_value(event.start_time)?;
        raw["endTime"] = serde_json::to_value(event.end_time)?;
    }
    let object = raw
        .as_object_mut()
        .ok_or_else(|| invalid("Event must be an object"))?;
    object.shift_remove(RECIPE_KEY);
    object.shift_remove(CURVE_KEY);
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
