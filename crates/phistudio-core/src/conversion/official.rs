use super::*;
fn number(value: &Value, key: &str, default: Option<f64>) -> Result<f64, ChartError> {
    let raw = value
        .get(key)
        .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()));
    finite(
        raw.or(default)
            .ok_or_else(|| invalid(format!("Phigros: missing numeric {key}")))?,
        key,
    )
}
fn entries<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}
fn add(
    target: &mut Vec<Event>,
    source: &Value,
    scale: f64,
    mut a: f64,
    b: f64,
) -> Result<(), ChartError> {
    let mut start = number(source, "startTime", None)? * scale;
    let end = number(source, "endTime", None)? * scale;
    if end < 0. {
        return Ok(());
    }
    if start < 0. && end > start {
        a += (b - a) * ((0. - start) / (end - start));
        start = 0.;
    }
    let start = start.max(0.);
    merged(
        target,
        Event {
            start_time: beat(start)?,
            end_time: beat(end.max(start + MIN_DURATION))?,
            start: a,
            end: b,
            easing: 1,
            normalized: None,
        },
    );
    Ok(())
}
pub(super) fn convert(source: Value) -> Result<ChartDocument, ChartError> {
    let version = number(&source, "formatVersion", None)?;
    if version != 1. && version != 3. {
        return Err(invalid(format!(
            "Unsupported Phigros formatVersion {version}; expected 1 or 3"
        )));
    }
    let lines = source["judgeLineList"]
        .as_array()
        .ok_or_else(|| invalid("Phigros: missing judgeLineList"))?;
    let first = lines
        .first()
        .ok_or_else(|| invalid("Phigros chart has no judge lines"))?;
    let base_bpm = number(first, "bpm", None)?;
    if base_bpm <= 0. {
        return Err(invalid("Phigros line BPM must be positive"));
    }
    let offset = (number(&source, "offset", Some(0.))? * 1000.).round_ties_even();
    if offset < i32::MIN as f64 || offset > i32::MAX as f64 {
        return Err(invalid("Phigros offset is out of range"));
    }
    let mut result = root(base_bpm, offset as i32)?;
    let mut output = Vec::new();
    for (index, raw) in lines.iter().enumerate() {
        let bpm = number(raw, "bpm", None)?;
        if bpm <= 0. {
            return Err(invalid(format!(
                "Phigros line {index}: BPM must be positive"
            )));
        }
        let scale = base_bpm / bpm / 32.;
        let mut line = Line::default();
        for (key, above) in [("notesAbove", 1), ("notesBelow", 0)] {
            for value in entries(raw, key) {
                let kind = match number(value, "type", None)? {
                    1. => NoteKind::Tap,
                    2. => NoteKind::Drag,
                    3. => NoteKind::Hold,
                    4. => NoteKind::Flick,
                    other => return Err(invalid(format!("Unknown Phigros note type {other}"))),
                };
                let start = number(value, "time", None)? * scale;
                let hold = if kind == NoteKind::Hold {
                    number(value, "holdTime", Some(0.))? * scale
                } else {
                    0.
                };
                if start < 0. {
                    continue;
                }
                line.notes.push(Note {
                    kind,
                    above,
                    start_time: beat(start)?,
                    end_time: beat(if kind == NoteKind::Hold {
                        (start + hold).max(start + MIN_DURATION)
                    } else {
                        start
                    })?,
                    x: number(value, "positionX", Some(0.))? * (675. * 18. / 160.),
                    speed: if kind == NoteKind::Hold {
                        1.
                    } else {
                        number(value, "speed", Some(1.))?
                    },
                    ..Note::default()
                });
            }
        }
        for value in entries(raw, "judgeLineMoveEvents") {
            let a = number(value, "start", None)?;
            let b = number(value, "end", None)?;
            let (x1, x2, y1, y2) = if version == 1. {
                let positive_mod = |n: f64| {
                    let rem = n % 1000.;
                    if rem < 0. { rem + 1000. } else { rem }
                };
                (
                    ((a / 1000.).trunc() / 880. - 0.5) * 1350.,
                    ((b / 1000.).trunc() / 880. - 0.5) * 1350.,
                    (positive_mod(a) / 520. - 0.5) * 900.,
                    (positive_mod(b) / 520. - 0.5) * 900.,
                )
            } else {
                (
                    (a - 0.5) * 1350.,
                    (b - 0.5) * 1350.,
                    (number(value, "start2", Some(0.5))? - 0.5) * 900.,
                    (number(value, "end2", Some(0.5))? - 0.5) * 900.,
                )
            };
            add(&mut line.tracks[0], value, scale, x1, x2)?;
            add(&mut line.tracks[1], value, scale, y1, y2)?;
        }
        for (key, track, multiplier) in [
            ("judgeLineRotateEvents", 2, -1.),
            ("judgeLineDisappearEvents", 3, 255.),
        ] {
            for value in entries(raw, key) {
                add(
                    &mut line.tracks[track],
                    value,
                    scale,
                    number(value, "start", None)? * multiplier,
                    number(value, "end", None)? * multiplier,
                )?;
            }
        }
        for value in entries(raw, "speedEvents") {
            let speed = number(value, "value", None)? * 4.5;
            add(&mut line.tracks[4], value, scale, speed, speed)?;
        }
        output.push(line.json(index));
    }
    result["judgeLineList"] = json!(output);
    if let Some(noise) = source.get("blockAreaList") {
        if !noise.is_array() {
            return Err(invalid("blockAreaList must be an array"));
        }
        result["blockAreaList"] = noise.clone();
    }
    ChartDocument::from_value(result)
}
