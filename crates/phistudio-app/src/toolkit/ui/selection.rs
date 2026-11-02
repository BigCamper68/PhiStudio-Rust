use super::*;

impl Workbench {
    pub(super) fn selection_tools_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (label, scope) in [
                ("All notes", Scope::Notes),
                ("All events", Scope::Events),
                ("Both", Scope::Both),
            ] {
                if ui.button(crate::i18n::tr(label)).clicked() {
                    self.select_scope(scope, false);
                }
            }
            if ui.button(crate::i18n::tr("Invert")).clicked() {
                self.select_scope(self.tools.scope, true);
            }
            if ui.button(crate::i18n::tr("Clear")).clicked() {
                self.select(None);
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(
                    self.timeline.tool == Tool::BoxSelect,
                    crate::i18n::tr("Box selection · B"),
                )
                .clicked()
            {
                self.set_tool(Tool::BoxSelect);
            }
            ui.checkbox(
                &mut self.tools.additive,
                crate::i18n::tr("Add to selection"),
            );
        });
        ui.small(RichText::new(crate::i18n::tr("Drag Box across either grid. Add toggles individual objects; Shift/Ctrl also toggles on desktop. The beat ruler scrolls while Box is active.")).color(muted()));
        ui.separator();
        ui.label(RichText::new(crate::i18n::tr("Select by range")).strong());
        choice(
            ui,
            "select-scope",
            &mut self.tools.scope,
            &[
                (Scope::Notes, "Notes"),
                (Scope::Events, "Events"),
                (Scope::Both, "Notes and events"),
            ],
        );
        beat_field(ui, "From beat", &mut self.tools.start);
        beat_field(ui, "To beat", &mut self.tools.end);
        number(ui, "Minimum X", &mut self.tools.x_range[0]);
        number(ui, "Maximum X", &mut self.tools.x_range[1]);
        choice(
            ui,
            "select-kind",
            &mut self.tools.filter_kind,
            &[
                (None, "Any note type"),
                (Some(NoteKind::Tap), "Tap"),
                (Some(NoteKind::Hold), "Hold"),
                (Some(NoteKind::Flick), "Flick"),
                (Some(NoteKind::Drag), "Drag"),
            ],
        );
        choice(
            ui,
            "select-side",
            &mut self.tools.filter_side,
            &[
                (None, "Either side"),
                (Some(1), "Above"),
                (Some(2), "Below"),
            ],
        );
        choice(
            ui,
            "select-fake",
            &mut self.tools.filter_fake,
            &[
                (None, "Real and Fake"),
                (Some(false), "Real notes"),
                (Some(true), "Fake notes"),
            ],
        );
        choice(
            ui,
            "select-track",
            &mut self.tools.filter_track,
            &[
                (None, "Any event track"),
                (Some(EventType::MoveX), "Move X"),
                (Some(EventType::MoveY), "Move Y"),
                (Some(EventType::Rotate), "Rotate"),
                (Some(EventType::Alpha), "Alpha"),
                (Some(EventType::Speed), "Speed"),
            ],
        );
        if ui.button(crate::i18n::tr("Select range")).clicked() {
            self.select_range();
        }
        if ui
            .button(crate::i18n::tr("Remove exact duplicate notes"))
            .clicked()
        {
            self.apply_tool(tools::remove_duplicates(
                self.editor.document(),
                &self.selection(),
            ));
        }
    }
}
