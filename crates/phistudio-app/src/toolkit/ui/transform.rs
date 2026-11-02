use super::*;

impl Workbench {
    pub(super) fn transform_tools_ui(&mut self, ui: &mut egui::Ui) {
        let selection = match self.timing_selection() {
            Ok(s) => s,
            Err(e) => {
                ui.colored_label(flick(), crate::i18n::tr(e.to_string()));
                return;
            }
        };
        ui.label(RichText::new(crate::i18n::tr("Transform selection")).strong());
        ui.label(crate::i18n::tr("X scaling anchor"));
        choice(
            ui,
            "x-scale-anchor",
            &mut self.tools.scale_anchor,
            &[
                (crate::selection_scaling::Anchor::First, "First note"),
                (crate::selection_scaling::Anchor::Center, "Selection center"),
                (crate::selection_scaling::Anchor::Last, "Last note"),
            ],
        );
        ui.small(crate::i18n::tr("In Select mode, drag the square handles above selected notes to scale X. Release to apply; Escape or a second finger cancels."));
        ui.horizontal_wrapped(|ui| {
            if ui.button("←").clicked() {
                self.nudge(0, -1);
            }
            if ui.button("→").clicked() {
                self.nudge(0, 1);
            }
            if ui.button(crate::i18n::tr("Earlier")).clicked() {
                self.nudge(-1, 0);
            }
            if ui.button(crate::i18n::tr("Later")).clicked() {
                self.nudge(1, 0);
            }
            if ui.button(crate::i18n::tr("Quantize")).clicked() {
                self.transform_selection(Transform::Quantize(self.timeline.division));
            }
        });
        ui.small({
            let __i18n_arg_0 = &(self.timeline.division);
            crate::i18n::format(
                "Nudge: one lane or 1/{0} beat. Quantize uses the current Grid division.",
                &[__i18n_arg_0.to_string()],
            )
        });
        beat_field(ui, "Shift beats", &mut self.tools.shift);
        number(ui, "Shift note X", &mut self.tools.shift_x);
        if ui.button(crate::i18n::tr("Apply shift")).clicked() {
            let edit = self.tools.shift.parse::<Beat>().and_then(|beats| {
                tools::transform(
                    self.editor.document(),
                    &selection,
                    Transform::Shift {
                        beats,
                        x: self.tools.shift_x,
                    },
                )
            });
            self.apply_tool(edit);
        }
        number(ui, "Mirror center X", &mut self.tools.mirror);
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Mirror X · M")).clicked() {
                self.transform_selection(Transform::MirrorX(self.tools.mirror));
            }
            if ui.button(crate::i18n::tr("Reverse time · R")).clicked() {
                self.transform_selection(Transform::ReverseTime);
            }
            if ui.button(crate::i18n::tr("Flip above / below")).clicked() {
                self.transform_selection(Transform::FlipSide);
            }
            if ui.button(crate::i18n::tr("Align start beats")).clicked() {
                self.transform_selection(Transform::AlignTime);
            }
        });
        beat_field(ui, "Time scale", &mut self.tools.factor);
        if ui
            .button(crate::i18n::tr("Scale from selection start"))
            .clicked()
        {
            let edit = self.tools.factor.parse::<Beat>().and_then(|factor| {
                tools::transform(
                    self.editor.document(),
                    &selection,
                    Transform::ScaleTime(factor),
                )
            });
            self.apply_tool(edit);
        }
        ui.separator();
        number(ui, "Arrange from X", &mut self.tools.pattern_x[0]);
        number(ui, "Arrange to X", &mut self.tools.pattern_x[1]);
        if ui
            .button(crate::i18n::tr("Distribute note X evenly"))
            .clicked()
        {
            self.transform_selection(Transform::ArrangeX {
                from: self.tools.pattern_x[0],
                to: self.tools.pattern_x[1],
            });
        }
        choice(ui, "convert-kind", &mut self.tools.note_kind, &KINDS);
        beat_field(ui, "Hold length", &mut self.tools.hold_length);
        if ui.button(crate::i18n::tr("Convert note type")).clicked() {
            let edit = self
                .tools
                .hold_length
                .parse::<Beat>()
                .and_then(|hold_length| {
                    tools::transform(
                        self.editor.document(),
                        &selection,
                        Transform::NoteType {
                            kind: self.tools.note_kind,
                            hold_length,
                        },
                    )
                });
            self.apply_tool(edit);
        }
        ui.horizontal_wrapped(|ui| {
            for (label, operation) in [
                ("Above", Transform::Side(1)),
                ("Below", Transform::Side(2)),
                ("Make real", Transform::Fake(false)),
                ("Make Fake", Transform::Fake(true)),
            ] {
                if ui.button(crate::i18n::tr(label)).clicked() {
                    self.transform_selection(operation);
                }
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Destination line"));
            ui.add(
                crate::numeric::Value::new(&mut self.tools.target_line)
                    .clamp_range(0..=self.editor.document().line_count() - 1),
            );
            ui.label(crate::i18n::tr("Layer"));
            ui.add(crate::numeric::Value::new(&mut self.tools.target_layer).clamp_range(0..=3));
        });
        ui.horizontal(|ui| {
            for (label, copy) in [
                ("Move to line / layer", false),
                ("Copy to line / layer", true),
            ] {
                if ui.button(crate::i18n::tr(label)).clicked() {
                    self.apply_tool(tools::relocate(
                        self.editor.document(),
                        &selection,
                        self.tools.target_line,
                        self.tools.target_layer,
                        copy,
                    ));
                }
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Additional copies"));
            ui.add(crate::numeric::Value::new(&mut self.tools.repeat_count).clamp_range(1..=4096));
        });
        beat_field(ui, "Repeat every", &mut self.tools.repeat_spacing);
        if ui.button(crate::i18n::tr("Repeat selection")).clicked() {
            let edit = self
                .tools
                .repeat_spacing
                .parse::<Beat>()
                .and_then(|spacing| {
                    tools::repeat(
                        self.editor.document(),
                        &selection,
                        self.tools.repeat_count,
                        spacing,
                    )
                });
            self.apply_tool(edit);
        }
    }

    pub(super) fn value_tools_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(crate::i18n::tr("Batch values")).strong());
        choice(
            ui,
            "value-field",
            &mut self.tools.field,
            &[
                (ValueField::X, "Note X"),
                (ValueField::Size, "Note size"),
                (ValueField::Speed, "Note speed"),
                (ValueField::Alpha, "Note alpha"),
                (ValueField::VisibleTime, "Note visible time"),
                (ValueField::YOffset, "Note Y offset"),
                (ValueField::EventEasing, "Event easing"),
                (ValueField::EventStart, "Event start value"),
                (ValueField::EventEnd, "Event end value"),
                (ValueField::EventBoth, "Both event values"),
                (ValueField::LinkGroup, "Event link group"),
            ],
        );
        choice(
            ui,
            "value-mode",
            &mut self.tools.mode,
            &[
                (ValueMode::Set, "Set to"),
                (ValueMode::Add, "Add"),
                (ValueMode::Multiply, "Multiply by"),
                (ValueMode::Min, "Limit maximum to"),
                (ValueMode::Max, "Limit minimum to"),
            ],
        );
        choice(
            ui,
            "value-profile",
            &mut self.tools.profile,
            &[
                (Profile::Ramp, "Eased ramp"),
                (Profile::Constant, "Constant"),
                (Profile::Sequence, "Repeating sequence"),
                (Profile::Sine, "Sine wave"),
                (Profile::Random, "Seeded random"),
            ],
        );
        number(ui, "From / value", &mut self.tools.values[0]);
        number(ui, "To", &mut self.tools.values[1]);
        easing(ui, &mut self.tools.easing);
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Sequence"));
            ui.add(egui::TextEdit::singleline(&mut self.tools.sequence).desired_width(220.));
        });
        number(ui, "Cycles", &mut self.tools.cycles);
        number(ui, "Random disturbance ±", &mut self.tools.disturbance);
        ui.checkbox(
            &mut self.tools.multiply_sequence,
            crate::i18n::tr("RPE profile: (eased ramp + variation) × multipliers"),
        );
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Random seed"));
            ui.add(crate::numeric::Value::new(&mut self.tools.seed));
        });
        ui.small(RichText::new(crate::i18n::tr("Values follow start-beat order. Equal-beat objects keep source order. Every field change retains custom hitsound/control data.")).color(muted()));
        match self.profile_values() {
            Ok(values) => {
                ui.small({
                    let __i18n_arg_0 = &(values
                        .iter()
                        .take(6)
                        .map(|v| format!("{v:.2}"))
                        .collect::<Vec<_>>()
                        .join(", "));
                    crate::i18n::format("Preview: {0}", std::slice::from_ref(__i18n_arg_0))
                });
                if ui.button(crate::i18n::tr("Apply batch values")).clicked() {
                    self.apply_tool(tools::batch_values(
                        self.editor.document(),
                        &self.selection(),
                        self.tools.field,
                        self.tools.mode,
                        &values,
                    ));
                }
            }
            Err(e) => {
                ui.colored_label(flick(), crate::i18n::tr(e.to_string()));
            }
        }
    }
}
