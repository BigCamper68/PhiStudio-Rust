use super::*;

impl Workbench {
    pub(super) fn generator_range_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Use selection bounds")).clicked() {
                self.selection_bounds();
            }
            if ui.button(crate::i18n::tr("Start at cursor")).clicked() {
                self.tools.start = Beat::from_grid(self.transport.beat, self.timeline.division)
                    .unwrap_or(Beat::ZERO)
                    .to_string();
            }
        });
        beat_field(ui, "Start beat", &mut self.tools.start);
        beat_field(ui, "End beat", &mut self.tools.end);
        beat_field(ui, "Step in beats", &mut self.tools.step);
    }

    pub(super) fn note_generator_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(crate::i18n::tr("Note generator")).strong());
        self.generator_range_ui(ui);
        choice(
            ui,
            "note-pattern",
            &mut self.tools.pattern,
            &[
                (tools::Pattern::Line, "Curve / stream"),
                (tools::Pattern::Staircase, "Staircase"),
                (tools::Pattern::Zigzag, "Zigzag"),
                (tools::Pattern::Sine, "Sine wave"),
                (tools::Pattern::Bezier, "Cubic Bezier"),
                (tools::Pattern::Random, "Seeded random"),
            ],
        );
        choice(ui, "generate-kind", &mut self.tools.note_kind, &KINDS);
        number(ui, "From X", &mut self.tools.pattern_x[0]);
        number(ui, "To X", &mut self.tools.pattern_x[1]);
        if self.tools.pattern == tools::Pattern::Bezier {
            number(ui, "Control 1 X", &mut self.tools.controls[0]);
            number(ui, "Control 2 X", &mut self.tools.controls[1]);
        }
        easing(ui, &mut self.tools.easing);
        number(ui, "Cycles", &mut self.tools.cycles);
        if self.tools.note_kind == NoteKind::Hold {
            beat_field(ui, "Hold length", &mut self.tools.hold_length);
        }
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Random seed"));
            ui.add(crate::numeric::Value::new(&mut self.tools.seed));
        });
        ui.checkbox(
            &mut self.tools.include_ends,
            crate::i18n::tr("Include endpoints"),
        );
        match self.generated_notes() {
            Ok(notes) => {
                ui.small({
                    let __i18n_arg_0 = &(notes.len());
                    let __i18n_arg_1 = &(self.line);
                    crate::i18n::format(
                        "Preview: {0} notes on line {1}",
                        &[__i18n_arg_0.to_string(), __i18n_arg_1.to_string()],
                    )
                });
                pattern_preview(ui, &notes);
                if ui.button(crate::i18n::tr("Generate notes")).clicked() {
                    self.apply_tool(tools::generate_notes(
                        self.editor.document(),
                        self.line,
                        notes.as_ref().clone(),
                    ));
                }
            }
            Err(e) => {
                ui.colored_label(flick(), crate::i18n::tr(e.to_string()));
            }
        }
    }

    pub(super) fn motion_generator_ui(&mut self, ui: &mut egui::Ui) {
        self.trajectory_ui(ui);
    }
}
