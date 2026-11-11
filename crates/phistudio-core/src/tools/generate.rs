use super::*;

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Pattern {
    Line,
    Staircase,
    Zigzag,
    Sine,
    Bezier,
    Random,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NotePattern {
    pub start: Beat,
    pub end: Beat,
    pub step: Beat,
    pub hold_length: Beat,
    pub kind: NoteKind,
    pub from_x: f64,
    pub to_x: f64,
    pub pattern: Pattern,
    pub cycles: f64,
    pub control: [f64; 2],
    pub seed: u64,
    pub lanes: usize,
    pub include_endpoints: bool,
}

impl NotePattern {
    pub fn notes(&self, mut easing: impl FnMut(f64) -> f64) -> Result<Vec<Note>, ChartError> {
        if self.start < Beat::ZERO
            || self.end <= self.start
            || self.step <= Beat::ZERO
            || !self.cycles.is_finite()
            || self.cycles <= 0.
            || ![self.from_x, self.to_x, self.control[0], self.control[1]]
                .iter()
                .all(|v| v.is_finite())
            || self.lanes < 2
        {
            return Err(invalid(
                "Use an ordered nonnegative range, positive step/cycles and finite positions",
            ));
        }
        let duration = self.end.checked_sub(self.start)?;
        let estimate = (duration.as_f64() / self.step.as_f64()).floor() + 1.;
        if estimate > MAX_OBJECTS as f64 {
            return Err(invalid(
                "The pattern exceeds 20,000 notes; increase its step",
            ));
        }
        let mut random = Random(self.seed);
        let mut notes = Vec::new();
        let mut beat = self.start;
        while beat <= self.end {
            let t = beat.checked_sub(self.start)?.as_f64() / duration.as_f64();
            if self.include_endpoints || (beat != self.start && beat != self.end) {
                let phase = t * self.cycles;
                let v = match self.pattern {
                    Pattern::Line => easing(t),
                    Pattern::Staircase => {
                        let f = if phase > 0. && phase.fract().abs() < 1e-12 {
                            1.
                        } else {
                            phase.fract()
                        };
                        (f * (self.lanes - 1) as f64).floor() / (self.lanes - 1) as f64
                    }
                    Pattern::Zigzag => 1. - (phase.rem_euclid(1.) * 2. - 1.).abs(),
                    Pattern::Sine => 0.5 - 0.5 * (phase * std::f64::consts::TAU).cos(),
                    Pattern::Bezier => {
                        let u = 1. - t;
                        let x = u * u * u * self.from_x
                            + 3. * u * u * t * self.control[0]
                            + 3. * u * t * t * self.control[1]
                            + t * t * t * self.to_x;
                        if self.from_x == self.to_x {
                            0.
                        } else {
                            (x - self.from_x) / (self.to_x - self.from_x)
                        }
                    }
                    Pattern::Random => random.next_f64(),
                };
                let x = if self.pattern == Pattern::Bezier {
                    let u = 1. - t;
                    u * u * u * self.from_x
                        + 3. * u * u * t * self.control[0]
                        + 3. * u * t * t * self.control[1]
                        + t * t * t * self.to_x
                } else {
                    self.from_x + (self.to_x - self.from_x) * v
                };
                let end_time = if self.kind == NoteKind::Hold {
                    if self.hold_length <= Beat::ZERO {
                        return Err(invalid("Hold length must be positive"));
                    }
                    beat.checked_add(self.hold_length)?
                } else {
                    beat
                };
                let note = Note {
                    kind: self.kind,
                    start_time: beat,
                    end_time,
                    x,
                    ..Note::default()
                };
                note.validate()?;
                notes.push(note);
            }
            beat = beat.checked_add(self.step)?;
        }
        if notes.is_empty() {
            return Err(invalid("The pattern contains no intermediate notes"));
        }
        Ok(notes)
    }
}

pub fn generate_notes(
    doc: &ChartDocument,
    line: usize,
    notes: Vec<Note>,
) -> Result<ToolEdit, ChartError> {
    let items = notes
        .into_iter()
        .map(|n| {
            Ok(Object {
                line,
                event: None,
                value: n.merge_into(None)?,
            })
        })
        .collect::<Result<_, ChartError>>()?;
    plan(doc, &Selection::default(), items, "Generate notes")
}
pub fn create_note(doc: &ChartDocument, line: usize, note: Note) -> Result<ToolEdit, ChartError> {
    plan(
        doc,
        &Selection::default(),
        vec![Object {
            line,
            event: None,
            value: note.merge_into(None)?,
        }],
        "Add note",
    )
}

#[derive(Clone, Debug)]
pub struct Random(pub u64);

impl Random {
    pub fn next_f64(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1_u64 << 53) as f64
    }
}

pub(crate) fn generated_event_edit(
    doc: &ChartDocument,
    remove: &[EventId],
    additions: &[(usize, usize, EventType, Value)],
    label: &'static str,
) -> Result<ToolEdit, ChartError> {
    let items = additions
        .iter()
        .map(|(line, layer, kind, value)| Object {
            line: *line,
            event: Some((*layer, *kind)),
            value: value.clone(),
        })
        .collect();
    let selected = Selection {
        notes: vec![],
        events: remove.to_vec(),
    };
    let result = plan(doc, &selected, items, label)?;
    result.validate_preview(doc)?;
    Ok(result)
}

pub fn generate_motion(
    doc: &ChartDocument,
    line: usize,
    layer: usize,
    start: Beat,
    end: Beat,
    step: Beat,
    mut point: impl FnMut(f64) -> [f64; 2],
) -> Result<ToolEdit, ChartError> {
    if start < Beat::ZERO || end <= start || step <= Beat::ZERO {
        return Err(invalid(
            "Motion needs an ordered nonnegative range and positive step",
        ));
    }
    if ((end.as_f64() - start.as_f64()) / step.as_f64()).ceil() * 2. > MAX_OBJECTS as f64 {
        return Err(invalid(
            "The motion exceeds 20,000 events; increase its step",
        ));
    }
    let mut items = Vec::new();
    let mut beat = start;
    while beat < end {
        let next = beat.checked_add(step)?.min(end);
        let fraction = |b: Beat| {
            b.checked_sub(start).unwrap().as_f64() / end.checked_sub(start).unwrap().as_f64()
        };
        let a = point(fraction(beat));
        let b = point(fraction(next));
        for (axis, kind) in [EventType::MoveX, EventType::MoveY].into_iter().enumerate() {
            let event = LineEvent {
                start_time: beat,
                end_time: next,
                start: a[axis],
                end: b[axis],
                ..Default::default()
            };
            items.push(Object {
                line,
                event: Some((layer, kind)),
                value: event.merge(kind, None)?,
            });
        }
        beat = next;
    }
    plan(doc, &Selection::default(), items, "Generate motion")
}
