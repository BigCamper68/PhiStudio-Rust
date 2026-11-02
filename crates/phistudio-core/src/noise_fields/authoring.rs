//! New keys begin at the sampled channel value. Timeline shifts retain the
//! original dialect's clock: seconds for native points, chart beats for RPE.
use super::*;

pub fn new_key(
    field: &Value,
    track: Track,
    start: Beat,
    end: Beat,
    tempo: &TempoMap,
) -> Result<Value, ChartError> {
    let format = format(field)?;
    if !Track::all(format).contains(&track) {
        return Err(invalid(
            "This track does not belong to the selected Noise Field format",
        ));
    }
    let center = center(field)?;
    let time = tempo.seconds_at(start.as_f64());
    if track.native(format) {
        let mut rows: Vec<_> = keys(field, track)?.iter().collect();
        rows.sort_by(|a, b| {
            a["time"]
                .as_f64()
                .unwrap_or(0.)
                .total_cmp(&b["time"].as_f64().unwrap_or(0.))
        });
        let i = rows.partition_point(|v| v["time"].as_f64().unwrap_or(0.) as f32 <= time as f32);
        let previous = i.checked_sub(1).map(|i| rows[i]);
        let next = rows.get(i).copied();
        let value_key = match track {
            Track::Move => "endPosition",
            Track::Scale => "scale",
            _ => "rotation",
        };
        let mut value = match track {
            Track::Move => json!({"x":center[0],"y":center[1]}),
            Track::Scale => json!({"x":1.,"y":1.}),
            _ => json!(0.),
        };
        if let Some(previous) = previous {
            value = previous[value_key].clone();
            if let Some(next) = next {
                let a = number(&previous["time"])? as f32;
                let b = number(&next["time"])? as f32;
                let progress = if b <= a {
                    1.
                } else {
                    ((time as f32 - a) / (b - a)).clamp(0., 1.)
                };
                let sample = |a: &Value, b: &Value, ease: &str| -> Result<Value, ChartError> {
                    let (a, b) = (number(a)? as f32, number(b)? as f32);
                    Ok(json!(
                        a + (b - a)
                            * native::ease(previous[ease].as_i64().unwrap_or(0) as i32, progress)
                    ))
                };
                if track == Track::Rotate {
                    value = sample(&value, &next[value_key], "easeType")?;
                } else {
                    for (axis, ease) in [("x", "easeTypeX"), ("y", "easeTypeY")] {
                        value[axis] = sample(&value[axis], &next[value_key][axis], ease)?;
                    }
                }
            }
        }
        let mut key = json!({"time":time,value_key:value});
        if track.anchored() {
            key["anchor"] = previous
                .and_then(|v| v.get("anchor"))
                .cloned()
                .unwrap_or_else(|| json!({"x":center[0],"y":center[1]}));
        }
        for name in if track == Track::Rotate {
            &["easeType"][..]
        } else {
            &["easeTypeX", "easeTypeY"][..]
        } {
            key[*name] = previous
                .and_then(|v| v.get(*name))
                .cloned()
                .unwrap_or(json!(0));
        }
        Ok(key)
    } else if track == Track::Active {
        Ok(json!({"startTime":start,"endTime":end,"readyFlash":0.5}))
    } else {
        let (value, anchor) = if track == Track::Color {
            (json!(Field::new(field, tempo)?.color_at(time)), center)
        } else {
            let (v, anchor) = next::authoring_value(field, track, time, tempo)?;
            (json!(v), anchor)
        };
        let mut key = json!({"startTime":start,"endTime":end,"start":value,"end":value,"easingType":1,"easingLeft":0.,"easingRight":1.,"bezier":0,"bezierPoints":[0.,0.,1.,1.],"inst":0,"linkgroup":0});
        if track.anchored() {
            key["anchor"] = json!({"x":anchor[0],"y":anchor[1]});
        }
        Ok(key)
    }
}

pub fn shift_key(
    key: &Value,
    track: Track,
    format: Format,
    from: Beat,
    to: Beat,
    tempo: &TempoMap,
) -> Result<Value, ChartError> {
    let mut value = key.clone();
    if from == to {
        return Ok(value);
    }
    if track.native(format) {
        value["time"] = json!(
            number(&key["time"])? + tempo.seconds_at(to.as_f64()) - tempo.seconds_at(from.as_f64())
        );
    } else {
        let delta = to.checked_sub(from)?;
        for name in ["startTime", "endTime"] {
            let at: Beat = serde_json::from_value(key[name].clone())?;
            value[name] = json!(at.checked_add(delta)?);
        }
    }
    validate_key(&value, track, format)?;
    Ok(value)
}

/// Native intervals use two real point keys; other tracks have one interval key.
pub fn new_key_interval(
    field: &Value,
    track: Track,
    start: Beat,
    end: Beat,
    tempo: &TempoMap,
) -> Result<Vec<Value>, ChartError> {
    if end <= start {
        return Err(invalid("Noise Field animation must have positive duration"));
    }
    let first = new_key(field, track, start, end, tempo)?;
    if track.native(format(field)?) {
        let last = new_key(field, track, end, end, tempo)?;
        Ok(vec![first, last])
    } else {
        Ok(vec![first])
    }
}

/// Outer lifetime handles keep native phase markers inside the field.
pub fn resize_field(
    field: &Value,
    phase: usize,
    at: Beat,
    tick: Beat,
    tempo: &TempoMap,
) -> Result<Value, ChartError> {
    let f = format(field)?;
    if phase >= PHASES.len()
        || (f == Format::Next && phase != 0 && phase != 3)
        || tick <= Beat::ZERO
    {
        return Err(invalid("Invalid Noise Field resize handle"));
    }
    let start = beat(field, PHASES[0], f, tempo)?;
    let end = beat(field, PHASES[3], f, tempo)?;
    let at = match phase {
        0 => at.min(end.checked_sub(tick)?),
        3 => at.max(start.checked_add(tick)?),
        1 => at.clamp(start, beat(field, PHASES[2], f, tempo)?.min(end).max(start)),
        _ => at.clamp(beat(field, PHASES[1], f, tempo)?.max(start).min(end), end),
    };
    let mut value = field.clone();
    put_beat(&mut value, PHASES[phase], at, f, tempo)?;
    if f == Format::Native && (phase == 0 || phase == 3) {
        let a = if phase == 0 { at } else { start };
        let b = if phase == 3 { at } else { end };
        let old = if phase == 0 { start } else { end };
        for inner in [1, 2] {
            let time = beat(field, PHASES[inner], f, tempo)?;
            let time = if time == old { at } else { time.clamp(a, b) };
            put_beat(&mut value, PHASES[inner], time, f, tempo)?;
        }
    }
    validate(&value)?;
    Ok(value)
}
