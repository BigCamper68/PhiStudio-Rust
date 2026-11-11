use super::*;
use std::collections::BTreeSet;
impl Snapshot {
    pub fn prepare(
        self,
        revision: u64,
        points: &[TracePoint],
        mut slice: impl FnMut(EventType, &LineEvent, f64, f64) -> Result<LineEvent, ChartError>,
        canceled: impl Fn() -> bool,
    ) -> Result<Edit, ChartError> {
        if self.has_conflicts() && !self.spec.replace {
            return Err(invalid(
                "Destination events overlap the tracing range. Enable Replace events in range to apply the preview.",
            ));
        }
        let points = sampling::simplify(points, &self.spec, &canceled)?;
        let begin = self.tempo.seconds_at(self.spec.range[0].as_f64());
        let finish = self.tempo.seconds_at(self.spec.range[1].as_f64());
        if points
            .first()
            .is_none_or(|p| (p.seconds - begin).abs() > 1e-7)
            || points
                .last()
                .is_none_or(|p| (p.seconds - finish).abs() > 1e-7)
        {
            return Err(invalid(
                "Native samples do not cover the full tracing range",
            ));
        }
        let mut additions = Vec::new();
        let beat = |time| Beat::from_grid(self.tempo.beat_at(time), 1_000_000_000);
        for pair in points.windows(2) {
            if pair[1].break_before {
                continue;
            }
            let start = beat(pair[0].seconds)?;
            let end = beat(pair[1].seconds)?;
            if end <= start {
                continue;
            }
            for (axis, kind) in [EventType::MoveX, EventType::MoveY, EventType::Rotate]
                .into_iter()
                .enumerate()
            {
                if kind == EventType::Rotate && !self.spec.rotation {
                    continue;
                }
                let event = LineEvent {
                    start_time: start,
                    end_time: end,
                    start: pair[0].values[axis],
                    end: pair[1].values[axis],
                    ..Default::default()
                };
                additions.push((self.line, self.spec.layer, kind, event.merge(kind, None)?));
            }
        }
        // A genuine jump at the final boundary needs a right-hand constant
        // continuation. Ordinary RPE events require positive duration; stop
        // before the next untouched event, whose own value retains priority.
        if points.last().is_some_and(|p| p.break_before) {
            let last = points.last().unwrap();
            for (axis, kind) in [EventType::MoveX, EventType::MoveY, EventType::Rotate]
                .into_iter()
                .enumerate()
            {
                if (kind == EventType::Rotate && !self.spec.rotation)
                    || self.right_continuations[axis]
                    || (last.values[axis] - points[points.len() - 2].values[axis]).abs()
                        <= if axis == 2 {
                            self.spec.rotation_tolerance
                        } else {
                            self.spec.position_tolerance
                        }
                {
                    continue;
                }
                let event = LineEvent {
                    start_time: self.spec.range[1],
                    end_time: match self.next_events[axis] {
                        Some(next) => next,
                        None => self.spec.range[1].checked_add(Beat::ONE)?,
                    },
                    start: last.values[axis],
                    end: last.values[axis],
                    ..Default::default()
                };
                additions.push((self.line, self.spec.layer, kind, event.merge(kind, None)?));
            }
        }
        for existing in &self.existing {
            let event = &existing.event;
            let start = self.tempo.seconds_at(event.start_time.as_f64());
            let duration = self.tempo.seconds_at(event.end_time.as_f64()) - start;
            for (a, b) in [
                (event.start_time, self.spec.range[0]),
                (self.spec.range[1], event.end_time),
            ] {
                if a >= b {
                    continue;
                }
                let mut retained = slice(
                    existing.id.kind,
                    event,
                    (self.tempo.seconds_at(a.as_f64()) - start) / duration,
                    (self.tempo.seconds_at(b.as_f64()) - start) / duration,
                )?;
                retained.start_time = a;
                retained.end_time = b;
                additions.push((
                    self.line,
                    self.spec.layer,
                    existing.id.kind,
                    retained.merge(existing.id.kind, Some(&existing.raw))?,
                ));
            }
        }
        if additions.is_empty() || additions.len() > tools::MAX_OBJECTS {
            return Err(invalid(
                "The trace exceeds 20,000 events. Shorten the range or increase the tolerance.",
            ));
        }
        let recipe_change = if let Some(before) = self.recipes {
            let removed: BTreeSet<_> = self
                .existing
                .iter()
                .map(|e| Ok((e.id.kind, serde_json::to_string(&e.raw)?)))
                .collect::<Result<_, ChartError>>()?;
            before.as_array().and_then(|list| {
                let retained: Vec<_> = list
                    .iter()
                    .filter(|recipe| {
                        recipe["layer"].as_u64() != Some(self.spec.layer as u64)
                            || !recipe["generated"].as_array().is_some_and(|generated| {
                                generated.iter().any(|event| {
                                    serde_json::from_value::<EventType>(event["kind"].clone())
                                        .ok()
                                        .zip(serde_json::to_string(&event["value"]).ok())
                                        .is_some_and(|key| removed.contains(&key))
                                })
                            })
                    })
                    .cloned()
                    .collect();
                (retained.len() != list.len()).then(|| {
                    (
                        before.clone(),
                        (!retained.is_empty()).then_some(Value::Array(retained)),
                    )
                })
            })
        } else {
            None
        };
        Ok(Edit {
            revision,
            line_count: self.line_count,
            line: self.line,
            new_line: self.spec.destination.is_none(),
            range: self.spec.range,
            remove: self.existing.into_iter().map(|e| e.id).collect(),
            additions,
            recipe_change,
        })
    }
}
