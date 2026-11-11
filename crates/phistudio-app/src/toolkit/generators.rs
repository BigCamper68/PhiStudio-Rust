use super::*;

impl Workbench {
    pub(super) fn profile_values(&self) -> Result<Vec<f64>, ChartError> {
        let selection = self.selection();
        let event_field = matches!(
            self.tools.field,
            ValueField::EventEasing
                | ValueField::EventStart
                | ValueField::EventEnd
                | ValueField::EventBoth
                | ValueField::LinkGroup
        );
        let count = if event_field {
            selection.events.len()
        } else {
            selection.notes.len()
        }
        .max(1);
        let s = &self.tools;
        if count > tools::MAX_OBJECTS
            || !s.cycles.is_finite()
            || !s.disturbance.is_finite()
            || s.cycles <= 0.
        {
            return Err(ChartError::Invalid(
                "Use positive cycles and finite disturbance".into(),
            ));
        }
        let sequence: Vec<f64> = s
            .sequence
            .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
            .filter(|s| !s.is_empty())
            .map(str::parse::<f64>)
            .collect::<Result<_, _>>()
            .map_err(|_| {
                ChartError::Invalid("Enter a numeric sequence separated by commas".into())
            })?;
        if sequence.is_empty() || sequence.len() > 256 || sequence.iter().any(|v| !v.is_finite()) {
            return Err(ChartError::Invalid(
                "A sequence needs 1–256 finite values".into(),
            ));
        }
        if s.multiply_sequence {
            return tools::ValueProfile {
                from: s.values[0],
                to: s.values[1],
                easing: s.easing,
                sequence,
                disturbance: s.disturbance,
                seed: s.seed,
            }
            .values(count, |e, t| {
                f64::from(
                    phistudio_prpr::EventCurve::new(
                        EventType::MoveX,
                        &LineEvent {
                            easing_type: e,
                            end: 1.,
                            ..Default::default()
                        },
                    )
                    .sample(t as f32),
                )
            });
        }
        let curve = phistudio_prpr::EventCurve::new(
            EventType::MoveX,
            &LineEvent {
                end: 1.,
                easing_type: s.easing,
                ..Default::default()
            },
        );
        let mut random = tools::Random(s.seed);
        Ok((0..count)
            .map(|i| {
                let t = if count < 2 {
                    0.
                } else {
                    i as f64 / (count - 1) as f64
                };
                let value = match s.profile {
                    Profile::Sequence => sequence[i % sequence.len()],
                    Profile::Constant => s.values[0],
                    Profile::Ramp => {
                        s.values[0]
                            + (s.values[1] - s.values[0]) * f64::from(curve.sample(t as f32))
                    }
                    Profile::Sine => {
                        s.values[0]
                            + (s.values[1] - s.values[0])
                                * (0.5 - 0.5 * (t * s.cycles * std::f64::consts::TAU).cos())
                    }
                    Profile::Random => {
                        s.values[0] + (s.values[1] - s.values[0]) * random.next_f64()
                    }
                };
                value + (random.next_f64() * 2. - 1.) * s.disturbance
            })
            .collect())
    }

    pub(super) fn generated_notes(&mut self) -> Result<Arc<Vec<phistudio_core::Note>>, ChartError> {
        let s = &self.tools;
        let pattern = tools::NotePattern {
            start: s.start.parse()?,
            end: s.end.parse()?,
            step: s.step.parse()?,
            hold_length: s.hold_length.parse()?,
            kind: s.note_kind,
            from_x: s.pattern_x[0],
            to_x: s.pattern_x[1],
            pattern: s.pattern,
            cycles: s.cycles,
            control: s.controls,
            seed: s.seed,
            lanes: self.timeline.lanes,
            include_endpoints: s.include_ends,
        };
        let easing = s.easing;
        if self.tools.note_preview.as_ref().is_none_or(|cache| {
            cache.pattern != pattern
                || cache.easing != easing
                || cache.defaults != self.options.new_note_defaults
        }) {
            let curve = phistudio_prpr::EventCurve::new(
                EventType::MoveX,
                &LineEvent {
                    end: 1.,
                    easing_type: easing,
                    ..Default::default()
                },
            );
            let result = pattern
                .notes(|t| f64::from(curve.sample(t as f32)))
                .map(|mut notes| {
                    for note in &mut notes {
                        self.options
                            .new_note_defaults
                            .apply_style(note, phistudio_core::NoteCreationPolicy::Regular);
                    }
                    notes
                })
                .map(Arc::new)
                .map_err(|e| e.to_string());
            self.tools.note_preview = Some(NotePreview {
                defaults: self.options.new_note_defaults.clone(),
                pattern,
                easing,
                result,
            });
        }
        self.tools
            .note_preview
            .as_ref()
            .unwrap()
            .result
            .clone()
            .map_err(ChartError::Invalid)
    }
}
