use super::*;
use crate::{EventCurve, speed_curve::NativeModel};
const MAX_SEGMENTS: usize = 1024;
const TOLERANCE: f64 = 0.15;
fn time(raw: &Value, key: &str) -> Result<Beat> {
    Ok(serde_json::from_value(raw[key].clone())?)
}
fn curved(raw: &Value) -> bool {
    raw["bezier"].as_u64().unwrap_or(0) != 0 || raw["easingType"].as_i64().unwrap_or(1) > 1
}
fn speed_at(raw: &Value, beat: Beat, tempo: &TempoMap) -> Result<f64> {
    let event = LineEvent::read(EventType::Speed, raw)?;
    if beat >= event.end_time {
        return Ok(event.end);
    }
    if raw["easingType"].as_i64() == Some(0) {
        return Ok(event.start);
    }
    let a = tempo.seconds_at(event.start_time.as_f64());
    let b = tempo.seconds_at(event.end_time.as_f64());
    let f = if b > a {
        ((tempo.seconds_at(beat.as_f64()) - a) / (b - a)).clamp(0., 1.)
    } else {
        1.
    };
    Ok(event.start + (event.end - event.start) * f)
}
fn target_distance(native: &mut NativeModel, points: &[SpeedCurvePoint], beat: f64) -> f64 {
    let i = points
        .partition_point(|p| p.beat.as_f64() <= beat)
        .saturating_sub(1)
        .min(points.len() - 2);
    let a = &points[i];
    let b = &points[i + 1];
    let u = ((beat - a.beat.as_f64()) / (b.beat.as_f64() - a.beat.as_f64())).clamp(0., 1.);
    if let Some(easing) = a.easing {
        let event = LineEvent {
            easing_type: easing,
            ..Default::default()
        };
        let curve = EventCurve::new(EventType::Rotate, &event);
        let low = f64::from(curve.sample(0.));
        let high = f64::from(curve.sample(1.));
        let fraction = (f64::from(curve.sample(u as f32)) - low) / (high - low);
        return a.distance + (b.distance - a.distance) * fraction;
    }
    let lo = native.distance(a.source_beat.as_f64());
    let hi = native.distance(b.source_beat.as_f64());
    let source = a.source_beat.as_f64() + (b.source_beat.as_f64() - a.source_beat.as_f64()) * u;
    let value = native.distance(source);
    if (hi - lo).abs() > 1e-6 {
        a.distance + (b.distance - a.distance) * (value - lo) / (hi - lo)
    } else {
        value + (a.distance - lo) * (1. - u) + (b.distance - hi) * u
    }
}
fn target_floor(
    native: &mut NativeModel,
    points: &[SpeedCurvePoint],
    layer: usize,
    beat: f64,
) -> Result<f64> {
    if native.source.note.kind == NoteKind::Hold && beat >= native.source.note.start_time.as_f64() {
        return Ok(native.layer_floor(layer, beat));
    }
    let distance = target_distance(native, points, beat);
    let raw = native.inverse(distance, beat)?;
    let others = (0..native.height.len())
        .filter(|i| *i != layer)
        .map(|i| native.layer_floor(i, beat))
        .sum::<f64>();
    let result = native.hit_floor - raw + native.source.note.y_offset - others;
    ensure!(
        result.is_finite(),
        "The requested speed curve is not finite"
    );
    Ok(result)
}
struct Segment {
    a: Beat,
    b: Beat,
    start: f64,
    end: f64,
}
fn fit_interval(
    native: &mut NativeModel,
    points: &[SpeedCurvePoint],
    layer: usize,
    a: Beat,
    b: Beat,
    out: &mut Vec<Segment>,
    depth: usize,
) -> Result<()> {
    ensure!(
        out.len() < MAX_SEGMENTS && depth <= 26,
        "The curve exceeds the fitting budget; simplify its shape or range"
    );
    let t0 = native.tempo.seconds_at(a.as_f64());
    let t1 = native.tempo.seconds_at(b.as_f64());
    let dt = t1 - t0;
    ensure!(
        dt > 1e-10,
        "Speed curve intervals must have positive duration"
    );
    let sample = |native: &mut NativeModel, f: f64| {
        let beat = native.tempo.beat_at(t0 + dt * f);
        target_floor(native, points, layer, beat)
    };
    let f0 = sample(native, 0.)?;
    let fm = sample(native, 0.5)?;
    let f1 = sample(native, 1.)?;
    // The legacy parser samples speed at end_time - 1e-4 before integrating.
    // Constant cells avoid cumulative area errors and its near-zero slope cases.
    let quadratic = if native.modern {
        2. * (f0 + f1 - 2. * fm)
    } else {
        0.
    };
    let linear = f1 - f0 - quadratic;
    let mut error = 0_f64;
    for u in [0.125, 0.25, 0.375, 0.625, 0.75, 0.875] {
        error = error.max((sample(native, u)? - (f0 + linear * u + quadratic * u * u)).abs());
    }
    let minimum_dt = if native.modern {
        prpr::core::EPS * 8.
    } else {
        2e-4
    };
    if error > TOLERANCE && depth < 24 && dt > minimum_dt * 2. {
        let middle = Beat::from_grid(native.tempo.beat_at((t0 + t1) * 0.5), 1_000_000_000)?;
        ensure!(
            middle > a && middle < b,
            "The speed curve needs more precision than native event times allow"
        );
        fit_interval(native, points, layer, a, middle, out, depth + 1)?;
        fit_interval(native, points, layer, middle, b, out, depth + 1)?;
    } else {
        // Native prpr drops intervals shorter than EPS. Keep endpoint area exact
        // instead of producing invisible fragments around singular easing slopes.
        let ratio = 100. / prpr::core::HEIGHT_RATIO;
        let start = linear / dt / ratio;
        let end = (linear + 2. * quadratic) / dt / ratio;
        ensure!(
            start.is_finite() && end.is_finite() && start.abs() <= 1e7 && end.abs() <= 1e7,
            "Generated speed is outside the supported range"
        );
        out.push(Segment { a, b, start, end });
    }
    Ok(())
}
fn generated(template: Option<&Value>, a: Beat, b: Beat, start: f64, end: f64) -> Value {
    let mut raw = template.cloned().unwrap_or(json!({}));
    raw["startTime"] = json!(a);
    raw["endTime"] = json!(b);
    raw["start"] = json!(start);
    raw["end"] = json!(end);
    raw["easingType"] = json!(1);
    raw["easingLeft"] = json!(0.);
    raw["easingRight"] = json!(1.);
    raw["bezier"] = json!(0);
    raw["bezierPoints"] = json!([0., 0., 0., 0.]);
    raw["phiStudioConstant"] = json!(start == end);
    raw
}
fn splice(
    native: &NativeModel,
    layer: usize,
    range: [Beat; 2],
    segments: Vec<Segment>,
) -> Result<Vec<Value>> {
    let [a, b] = range;
    let mut originals = native.source.layers[layer]
        .iter()
        .map(|raw| Ok((time(raw, "startTime")?, time(raw, "endTime")?, raw)))
        .collect::<Result<Vec<_>>>()?;
    originals.sort_by_key(|e| e.0);
    for pair in originals.windows(2) {
        if pair[0].1 > pair[1].0 && pair[1].0 < b && pair[0].1 > a {
            return Err(anyhow!(
                "Resolve overlapping speed events in this range before editing the curve"
            ));
        }
    }
    let mut result = Vec::new();
    let mut right_boundary = false;
    for &(start, end, raw) in &originals {
        if end <= a || start >= b {
            result.push(raw.clone());
            if start == b {
                right_boundary = true;
            }
            continue;
        }
        ensure!(
            raw["linkgroup"].as_i64().unwrap_or(0) == 0,
            "Unlink speed events in this range before editing the curve"
        );
        if start < a {
            ensure!(
                !curved(raw),
                "Move the range start to this nonlinear event's start before editing"
            );
            let mut left = raw.clone();
            left["endTime"] = json!(a);
            left["end"] = json!(speed_at(raw, a, &native.tempo)?);
            result.push(left);
        }
        if end > b {
            ensure!(
                !curved(raw),
                "Move the range end to this nonlinear event's end before editing"
            );
            let mut right = raw.clone();
            right["startTime"] = json!(b);
            right["start"] = json!(speed_at(raw, b, &native.tempo)?);
            result.push(right);
            right_boundary = true;
        }
    }
    for segment in segments {
        let middle = (segment.a.as_f64() + segment.b.as_f64()) * 0.5;
        let template = originals
            .iter()
            .rev()
            .find(|e| e.0.as_f64() <= middle)
            .map(|e| e.2);
        result.push(generated(
            template,
            segment.a,
            segment.b,
            segment.start,
            segment.end,
        ));
    }
    if !right_boundary {
        let previous = originals.iter().rev().find(|e| e.0 <= b).map(|e| e.2);
        let speed = previous
            .map(|e| speed_at(e, b, &native.tempo))
            .transpose()?
            .unwrap_or(0.);
        let mut end = b.checked_add(Beat::new(0, 1, 1920)?)?;
        if let Some(next) = originals.iter().find(|e| e.0 > b) {
            end = end.min(next.0);
        }
        result.push(generated(previous, b, end, speed, speed));
    }
    result.sort_by_key(|raw| time(raw, "startTime").unwrap_or(Beat::ZERO));
    ensure!(
        result.len() <= MAX_SPEED_EVENTS,
        "Generated speed events exceed the track budget"
    );
    Ok(result)
}
/// Fit a distance draft to ordinary linear RPE speed events. Native rendering
/// validates the result before it can become one reversible document command.
pub async fn fit_speed_curve(
    source: &SpeedCurveSource,
    layer: usize,
    profile: &SpeedCurveProfile,
    mut points: Vec<SpeedCurvePoint>,
    modern: bool,
) -> Result<SpeedCurveEdit> {
    ensure!(
        layer < 4
            && layer < source.layers.len()
            && points.len() >= 2
            && points.len() <= MAX_CURVE_POINTS,
        "Select an editable layer and at least two curve points"
    );
    points.sort_by_key(|p| p.beat);
    ensure!(
        points.first().map(|p| p.beat) == Some(profile.range[0])
            && points.last().map(|p| p.beat) == Some(profile.range[1])
            && points.windows(2).all(|p| p[0].beat < p[1].beat),
        "Curve points must stay ordered inside the work range"
    );
    ensure!(
        points.iter().all(|p| p.distance.is_finite()
            && p.distance.abs() <= 1e9
            && p.easing.is_none_or(|e| (1..=29).contains(&e))),
        "Use finite distances and easing 1–29"
    );
    let mut native = NativeModel::new(source, modern, profile.range[1]).await?;
    for fixed in profile.points.iter().filter(|p| p.fixed) {
        ensure!(
            points
                .iter()
                .any(|p| p.beat == fixed.beat && (p.distance - fixed.distance).abs() < 0.001),
            "Range endpoints and the judgment anchor are fixed"
        );
    }
    let unchanged = points.iter().all(|p| {
        p.beat == p.source_beat
            && p.easing.is_none()
            && (native.distance(p.beat.as_f64()) - p.distance).abs() < 0.05
    });
    let events = if unchanged {
        source.layers[layer].clone()
    } else {
        let mut cuts = std::collections::BTreeSet::from([profile.range[0], profile.range[1]]);
        cuts.extend(points.iter().map(|p| p.beat));
        let changes: Vec<BpmChange> = serde_json::from_value(source.bpm.clone())?;
        cuts.extend(changes.into_iter().map(|p| p.start));
        for events in &source.layers {
            for event in events {
                cuts.insert(time(event, "startTime")?);
                cuts.insert(time(event, "endTime")?);
            }
        }
        let cuts = cuts
            .into_iter()
            .filter(|b| *b >= profile.range[0] && *b <= profile.range[1])
            .collect::<Vec<_>>();
        let mut fitted = Vec::new();
        for cut in cuts.windows(2) {
            fit_interval(&mut native, &points, layer, cut[0], cut[1], &mut fitted, 0)?;
        }
        let events = splice(&native, layer, profile.range, fitted)?;
        let mut candidate = source.clone();
        candidate.layers[layer] = events.clone();
        let mut parsed = NativeModel::new(&candidate, modern, profile.range[1]).await?;
        for point in &points {
            ensure!(
                (parsed.distance(point.beat.as_f64()) - point.distance).abs() <= 1.,
                "Native playback cannot place this curve point accurately"
            );
        }
        for i in 0..=512 {
            let beat = profile.range[0].as_f64()
                + (profile.range[1].as_f64() - profile.range[0].as_f64()) * i as f64 / 512.;
            let target =
                if source.note.kind == NoteKind::Hold && beat >= source.note.start_time.as_f64() {
                    native.distance(beat)
                } else {
                    target_distance(&mut native, &points, beat)
                };
            ensure!(
                (parsed.distance(beat) - target).abs() <= 1.,
                "Native playback cannot represent this curve accurately; simplify its shape or range"
            );
        }
        events
    };
    for point in &mut points {
        point.source_beat = point.beat;
    }
    let recipe = SpeedCurveRecipe {
        version: 1,
        signature: speed_signature(&events),
        context: format!("{}:{modern}", source.curve_context(layer)),
        note: source.note.clone(),
        range: profile.range,
        points,
    };
    Ok(SpeedCurveEdit {
        id: source.id,
        layer,
        expected: source.fingerprint.clone(),
        events,
        recipe,
    })
}
