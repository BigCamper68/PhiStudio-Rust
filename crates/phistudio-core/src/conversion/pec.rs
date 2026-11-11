use super::*;
use crate::BpmChange;

#[derive(Clone)]
struct Command {
    start: f64,
    end: f64,
    target: f64,
    easing: u8,
    transition: bool,
}
#[derive(Clone, Copy)]
struct Speed {
    beat: f64,
    value: f64,
    normalized: f32,
}
#[derive(Default)]
struct Builder {
    line: Line,
    commands: [Vec<Command>; 4],
    speeds: Vec<Speed>,
}
fn pec_beat(value: f64) -> Result<Beat, ChartError> {
    if value as f32 as f64 == value {
        let exponent = (((value as f32).to_bits() >> 23) & 255) as i32 - 127;
        Beat::from_grid(value, 1_i32 << (23 - exponent).clamp(0, 30))
    } else {
        beat(value)
    }
}
fn event(start: f64, end: f64, a: f64, b: f64, easing: u8) -> Result<Event, ChartError> {
    Ok(Event {
        start_time: pec_beat(start)?,
        end_time: pec_beat(end)?,
        start: a,
        end: b,
        easing,
        normalized: None,
    })
}
fn seconds(bpms: &[BpmChange], beat: f64) -> f64 {
    let mut current = bpms[0];
    let mut elapsed = 0.;
    for next in bpms.iter().skip(1) {
        if next.start.as_f64() > beat {
            break;
        }
        elapsed += (next.start.as_f64() - current.start.as_f64()) * (60. / current.bpm);
        current = *next;
    }
    elapsed + (beat - current.start.as_f64()) * (60. / current.bpm)
}
fn original_heights(builders: &[Builder], bpms: &[BpmChange]) -> Value {
    let mut maximum = f64::NEG_INFINITY;
    for b in builders {
        let mut time = f64::NEG_INFINITY;
        for command in b.commands.iter().flatten() {
            time = time.max(seconds(bpms, command.end));
        }
        for speed in &b.speeds {
            time = time.max(seconds(bpms, speed.beat));
        }
        for note in &b.line.notes {
            time = time.max(seconds(bpms, note.start_time.as_f64()));
        }
        maximum = maximum.max(if time == f64::NEG_INFINITY { 0. } else { time });
    }
    maximum += 1.;
    builders
        .iter()
        .map(|b| {
            if b.speeds.is_empty() {
                return Value::Null;
            }
            let mut frames = Vec::new();
            if seconds(bpms, b.speeds[0].beat) >= 1e-5 {
                frames.push(json!([0., 0.]));
            }
            let (mut height, mut last_time, mut last_speed) = (0., 0., 0_f32);
            for point in &b.speeds {
                let time = seconds(bpms, point.beat);
                height += (time - last_time) * last_speed as f64;
                if !(height as f32).is_finite() {
                    return Value::Null;
                }
                frames.push(json!([time, height as f32 as f64]));
                last_time = time;
                last_speed = point.normalized;
            }
            let tail = (height + (maximum - last_time) * last_speed as f64) as f32;
            if !tail.is_finite() {
                return Value::Null;
            }
            frames.push(json!([maximum, tail as f64]));
            json!(frames)
        })
        .collect()
}
impl Builder {
    fn finish(mut self, index: usize) -> Result<Value, ChartError> {
        self.line.width = Some((3.91_f32 / 6.) as f64 * 2.);
        for (track, mut commands) in self.commands.into_iter().enumerate() {
            commands.sort_by(|a, b| {
                a.end
                    .total_cmp(&b.end)
                    .then_with(|| a.start.total_cmp(&b.start))
            });
            let mut occupied = f64::NEG_INFINITY;
            for command in &mut commands {
                command.start = command.start.max(0.).max(occupied);
                command.end = command.end.max(command.start);
                command.transition &= command.end > command.start;
                occupied = command.end;
            }
            let mut state = if track == 3 { 255. } else { 0. };
            for (i, c) in commands.iter().enumerate() {
                if !c.transition {
                    state = c.target;
                    let followed = commands
                        .get(i + 1)
                        .is_some_and(|n| pec_beat(n.start).ok() == pec_beat(c.start).ok());
                    if !followed {
                        merged(
                            &mut self.line.tracks[track],
                            event(c.start, c.start + MIN_DURATION, state, state, 1)?,
                        );
                    }
                } else {
                    merged(
                        &mut self.line.tracks[track],
                        event(
                            c.start,
                            c.end.max(c.start + MIN_DURATION),
                            state,
                            c.target,
                            c.easing,
                        )?,
                    );
                    state = c.target;
                }
            }
        }
        self.speeds.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        let mut unique: Vec<Speed> = Vec::new();
        for mut point in self.speeds {
            point.beat = point.beat.max(0.);
            if let Some(previous) = unique.last_mut().filter(|p| close(p.beat, point.beat)) {
                *previous = point;
            } else {
                unique.push(point);
            }
        }
        if unique.first().is_none_or(|p| p.beat > 0.) {
            unique.insert(
                0,
                Speed {
                    beat: 0.,
                    value: 0.,
                    normalized: 0.,
                },
            );
        }
        for (i, p) in unique.iter().enumerate() {
            let end = unique.get(i + 1).map_or(p.beat + MIN_DURATION, |n| n.beat);
            let mut e = event(p.beat, end.max(p.beat + MIN_DURATION), p.value, p.value, 1)?;
            e.normalized = Some(p.normalized);
            self.line.tracks[4].push(e);
        }
        Ok(self.line.json(index))
    }
}
struct Fields<'a> {
    values: Vec<&'a str>,
    line: usize,
}
impl<'a> Fields<'a> {
    fn error(&self, message: impl std::fmt::Display) -> ChartError {
        invalid(format!("PEC line {}: {message}", self.line))
    }
    fn count(&self, count: usize, at_least: bool) -> Result<(), ChartError> {
        if self.values.len() == count || at_least && self.values.len() > count {
            Ok(())
        } else {
            Err(self.error(format!(
                "Expected {}{count} fields, got {}",
                if at_least { "at least " } else { "" },
                self.values.len()
            )))
        }
    }
    fn number(&self, index: usize) -> Result<f64, ChartError> {
        self.values
            .get(index)
            .and_then(|s| s.parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .map(|v| v as f64)
            .ok_or_else(|| self.error(format!("Field {} must be a finite number", index + 1)))
    }
    fn integer(&self, index: usize) -> Result<usize, ChartError> {
        self.values
            .get(index)
            .and_then(|s| s.parse::<i32>().ok())
            .filter(|n| *n >= 0)
            .map(|v| v as usize)
            .ok_or_else(|| self.error(format!("Field {} must be a nonnegative integer", index + 1)))
    }
    fn easing(&self, index: usize) -> Result<u8, ChartError> {
        Ok(match self.integer(index)? {
            1..=29 => self.integer(index)? as u8,
            _ => 1,
        })
    }
    fn builder<'b>(&self, builders: &'b mut Vec<Builder>) -> Result<&'b mut Builder, ChartError> {
        let id = self.integer(1)?;
        // Bound sparse IDs before allocating every intervening RPE judge line.
        if id >= 65_536 {
            return Err(self.error("Judge line ID must be below 65536"));
        }
        builders.resize_with((id + 1).max(builders.len()), Builder::default);
        Ok(&mut builders[id])
    }
}
fn rpe_speed(raw: f32) -> f64 {
    const RATIO: f32 = (10_f64 / 45. / 0.83175) as f32;
    let speed = raw / 5.85;
    let mut candidate = speed / RATIO;
    let mut error = ((candidate * RATIO) as f64 - speed as f64).abs();
    for adjacent in [candidate.next_down(), candidate.next_up()] {
        let adjacent_error = ((adjacent * RATIO) as f64 - speed as f64).abs();
        if adjacent_error < error {
            candidate = adjacent;
            error = adjacent_error;
        }
    }
    candidate as f64
}
pub(super) fn convert(source: &str) -> Result<ChartDocument, ChartError> {
    let mut lines = source.lines().enumerate();
    let (offset_line, offset) = lines
        .find(|(_, s)| !s.trim().is_empty())
        .ok_or_else(|| invalid("PEC source is empty"))?;
    let offset = offset
        .trim()
        .trim_start_matches('\u{feff}')
        .parse::<f32>()
        .map_err(|_| invalid("PEC: expected a numeric millisecond offset on the first line"))?;
    let offset = (offset as f64 - 150.).round_ties_even();
    if !offset.is_finite() || offset < i32::MIN as f64 || offset > i32::MAX as f64 {
        return Err(invalid("PEC offset is out of range"));
    }
    let mut builders: Vec<Builder> = Vec::new();
    let mut bpms = Vec::new();
    let mut last: Option<(usize, usize)> = None;
    for (number, text) in lines {
        let text = text.trim();
        if text.is_empty() || text.starts_with("//") {
            continue;
        }
        let fields = Fields {
            values: text.split_whitespace().collect(),
            line: number + 1,
        };
        let command = fields.values[0].to_ascii_lowercase();
        match command.as_str() {
            "bp" => {
                fields.count(3, false)?;
                let bpm = fields.number(2)?;
                if bpm <= 0. {
                    return Err(fields.error("BPM must be positive"));
                }
                bpms.push(BpmChange {
                    start: pec_beat(fields.number(1)?)?,
                    bpm,
                });
            }
            "n1" | "n2" | "n3" | "n4" => {
                let hold = command == "n2";
                fields.count(if hold { 7 } else { 6 }, true)?;
                let kind = match command.as_str() {
                    "n2" => NoteKind::Hold,
                    "n3" => NoteKind::Flick,
                    "n4" => NoteKind::Drag,
                    _ => NoteKind::Tap,
                };
                let start = fields.number(2)?;
                let mut cursor = 3;
                let end = if hold {
                    cursor += 1;
                    fields.number(3)?.max(start + MIN_DURATION)
                } else {
                    start
                };
                let mut note = Note {
                    kind,
                    start_time: pec_beat(start)?,
                    end_time: pec_beat(end)?,
                    x: (fields.number(cursor)? as f32 / 1024.) as f64 * 675.,
                    above: u8::from(fields.integer(cursor + 1)? == 1),
                    ..Note::default()
                };
                let fake = fields.integer(cursor + 2)?;
                if fake > 1 {
                    return Err(fields.error("fake must be 0 or 1"));
                }
                note.is_fake = fake as u8;
                cursor += 3;
                while cursor < fields.values.len() {
                    let marker = fields.values[cursor];
                    let value = fields.number(cursor + 1)?;
                    match marker {
                        "#" => note.speed = value,
                        "&" => note.size = value,
                        _ => return Err(fields.error(format!("Unexpected note suffix {marker}"))),
                    }
                    cursor += 2;
                }
                note.validate()?;
                let id = fields.integer(1)?;
                let b = fields.builder(&mut builders)?;
                last = Some((id, b.line.notes.len()));
                b.line.notes.push(note);
            }
            "#" | "&" => {
                fields.count(2, false)?;
                let (id, index) =
                    last.ok_or_else(|| fields.error(format!("No note before {command}")))?;
                let value = fields.number(1)?;
                let note = &mut builders[id].line.notes[index];
                if command == "#" {
                    note.speed = value;
                } else {
                    note.size = value;
                }
            }
            "cv" => {
                fields.count(4, false)?;
                let raw = fields.number(3)? as f32;
                fields.builder(&mut builders)?.speeds.push(Speed {
                    beat: fields.number(2)?,
                    value: rpe_speed(raw),
                    normalized: raw / 5.85,
                });
            }
            "cp" | "cm" | "cd" | "cr" | "ca" | "cf" => {
                let transition = matches!(command.as_str(), "cm" | "cr" | "cf");
                let movement = matches!(command.as_str(), "cp" | "cm");
                let rotation = matches!(command.as_str(), "cd" | "cr");
                fields.count(
                    if movement {
                        if transition { 7 } else { 5 }
                    } else if rotation {
                        if transition { 6 } else { 4 }
                    } else if transition {
                        5
                    } else {
                        4
                    },
                    false,
                )?;
                let start = fields.number(2)?;
                let cursor = if transition { 4 } else { 3 };
                let end = if transition { fields.number(3)? } else { start };
                let easing = if transition && (movement || rotation) {
                    fields.easing(fields.values.len() - 1)?
                } else {
                    1
                };
                let b = fields.builder(&mut builders)?;
                if movement {
                    let x = (fields.number(cursor)? as f32 / 2048. * 2. - 1.) as f64 * 675.;
                    let y = (fields.number(cursor + 1)? as f32 / 1400. * 2. - 1.) as f64 * 450.;
                    for (track, target) in [(0, x), (1, y)] {
                        b.commands[track].push(Command {
                            start,
                            end,
                            target,
                            easing,
                            transition,
                        });
                    }
                } else {
                    b.commands[if rotation { 2 } else { 3 }].push(Command {
                        start,
                        end,
                        target: fields.number(cursor)?,
                        easing,
                        transition,
                    });
                }
            }
            _ => return Err(fields.error(format!("Unsupported PEC command {}", fields.values[0]))),
        }
    }
    let _ = offset_line;
    if bpms.is_empty() {
        bpms.push(BpmChange {
            start: Beat::ZERO,
            bpm: 120.,
        });
    }
    bpms.sort_by_key(|b| b.start);
    if builders.is_empty() {
        builders.push(Builder::default());
    }
    let heights = original_heights(&builders, &bpms);
    let mut result = root(bpms[0].bpm, offset as i32)?;
    result["META"]["phiPecAlphaExtension"] = json!(true);
    result["BPMList"] = json!(bpms);
    result["judgeLineList"] = builders
        .into_iter()
        .enumerate()
        .map(|(i, b)| b.finish(i))
        .collect::<Result<Value, _>>()?;
    let signature = pec_height_signature(&result)
        .ok_or_else(|| invalid("Unable to record PEC height provenance"))?;
    result["META"]["phiPecHeights"] = json!({"signature":signature,"lines":heights});
    ChartDocument::from_value(result)
}

/// Java-compatible signature for the timing/speed inputs of preserved PEC floors.
/// Returning None prevents stale or malformed provenance from affecting rendering.
pub fn pec_height_signature(root: &Value) -> Option<String> {
    fn beat_hash(v: &Value) -> Option<u64> {
        let beat: Beat = serde_json::from_value(v.clone()).ok()?;
        let h = beat
            .triple()
            .into_iter()
            .fold(1_i32, |h, n| h.wrapping_mul(31).wrapping_add(n));
        Some(h as i64 as u64)
    }
    fn push(h: &mut u64, value: u64) {
        *h = h.wrapping_mul(31).wrapping_add(value);
    }
    let mut h = 17;
    let bpms = root.get("BPMList")?.as_array()?;
    push(&mut h, bpms.len() as u64);
    for bpm in bpms {
        push(&mut h, beat_hash(&bpm["startTime"])?);
        push(&mut h, bpm["bpm"].as_f64()?.to_bits());
    }
    let lines = root.get("judgeLineList")?.as_array()?;
    push(&mut h, lines.len() as u64);
    for line in lines {
        let notes = line
            .get("notes")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        push(&mut h, notes.len() as u64);
        for note in notes {
            push(&mut h, beat_hash(&note["startTime"])?);
            push(&mut h, beat_hash(&note["endTime"])?);
            push(&mut h, note["type"].as_u64()?.checked_sub(1)?);
        }
        let layers = line.get("eventLayers")?.as_array()?;
        push(&mut h, layers.len() as u64);
        for layer in layers {
            for (index, key) in TRACKS.iter().enumerate() {
                let events = layer
                    .get(key)
                    .and_then(Value::as_array)
                    .map_or(&[][..], Vec::as_slice);
                push(&mut h, events.len() as u64);
                for event in events {
                    push(&mut h, beat_hash(&event["startTime"])?);
                    push(&mut h, beat_hash(&event["endTime"])?);
                    if index == 4 {
                        if event
                            .get(crate::custom_easing::CURVE_KEY)
                            .is_some_and(|v| !v.is_null())
                        {
                            return None;
                        }
                        for key in ["start", "end"] {
                            let value = event[key].as_f64()?;
                            push(&mut h, if value == 0. { 0 } else { value.to_bits() });
                        }
                        push(
                            &mut h,
                            event.get("easingType").and_then(Value::as_u64).unwrap_or(1),
                        );
                        let stamp = if let Some(pec) = event.get("phiPecSpeed") {
                            let normalized = pec["normalized"].as_f64()?;
                            let a = pec["start"].as_f64()?;
                            let b = pec["end"].as_f64()?;
                            if normalized != normalized as f32 as f64 || !normalized.is_finite() {
                                return None;
                            }
                            let seed = (1_i64 + (normalized as f32).to_bits() as i32 as i64) as u64;
                            seed.wrapping_mul(31)
                                .wrapping_add(a.to_bits())
                                .wrapping_mul(31)
                                .wrapping_add(b.to_bits())
                        } else {
                            0
                        };
                        push(&mut h, stamp);
                    }
                }
            }
        }
        for key in [
            "scaleXEvents",
            "scaleYEvents",
            "colorEvents",
            "paintEvents",
            "textEvents",
            "inclineEvents",
            "gifEvents",
        ] {
            let events = line
                .get("extended")
                .and_then(|v| v.get(key))
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice);
            push(&mut h, events.len() as u64);
            for event in events {
                push(&mut h, beat_hash(&event["startTime"])?);
                push(&mut h, beat_hash(&event["endTime"])?);
            }
        }
    }
    Some(format!("{h:x}"))
}
