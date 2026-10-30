use super::*;

impl Workbench {
    pub(super) fn event_tools_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(crate::i18n::tr("Event tools")).strong());
        if ui.button(crate::i18n::tr("Clone events…")).clicked() {
            self.open_clone();
        }
        if ui
            .button(crate::i18n::tr("Synchronize X/Y timing in this layer"))
            .clicked()
        {
            let edit = tools::synchronize_xy(
                self.editor.document(),
                self.line,
                self.events.layer,
                phistudio_prpr::slice_event,
            );
            self.apply_tool(edit);
            if self
                .editor
                .document()
                .validate_xy_layer(self.line, self.events.layer)
                .is_ok()
            {
                self.options.xy_link = true;
            }
        }
        let selection = match self.timing_selection() {
            Ok(s) => s,
            Err(e) => {
                ui.colored_label(flick(), crate::i18n::tr(e.to_string()));
                return;
            }
        };
        ui.small(RichText::new(crate::i18n::tr("Split and subdivision retain the original prpr curve, including Bezier and easing windows. Split fractions account for BPM changes.")).color(muted()));
        if ui
            .button(crate::i18n::tr("Split selected events at cursor"))
            .clicked()
        {
            let edit = Beat::from_grid(self.beat, self.division).and_then(|beat| {
                tools::subdivide_events(
                    self.editor.document(),
                    &selection,
                    2,
                    Some(beat),
                    phistudio_prpr::slice_event,
                )
            });
            self.apply_tool(edit);
        }
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Equal beat intervals"));
            ui.add(crate::numeric::Value::new(&mut self.tools.parts).clamp_range(2..=256));
        });
        if ui
            .button(crate::i18n::tr("Subdivide selected events"))
            .clicked()
        {
            self.apply_tool(tools::subdivide_events(
                self.editor.document(),
                &selection,
                self.tools.parts,
                None,
                phistudio_prpr::slice_event,
            ));
        }
        if ui
            .button(crate::i18n::tr("Join continuous linear events"))
            .clicked()
        {
            self.apply_tool(tools::join_events(self.editor.document(), &selection));
        }
        if ui
            .button(crate::i18n::tr("Connect start values to previous ends"))
            .clicked()
        {
            self.apply_tool(tools::connect_events(
                self.editor.document(),
                &self.selection(),
            ));
        }
        if ui
            .button(crate::i18n::tr("Swap start / end values"))
            .clicked()
        {
            self.transform_selection(Transform::ReverseValues);
        }
        if ui
            .button(crate::i18n::tr("Reverse events in time"))
            .clicked()
        {
            self.transform_selection(Transform::ReverseTime);
        }
        easing(ui, &mut self.tools.easing);
        if ui
            .button(crate::i18n::tr("Apply easing to selection"))
            .clicked()
        {
            self.transform_selection(Transform::Easing(self.tools.easing));
        }
        ui.separator();
        ui.checkbox(
            &mut self.tools.curve_bezier,
            crate::i18n::tr("Use Bezier for the selection"),
        );
        number(ui, "Easing left", &mut self.tools.curve_window[0]);
        number(ui, "Easing right", &mut self.tools.curve_window[1]);
        if self.tools.curve_bezier {
            for (i, label) in ["Control 1 X", "Control 1 Y", "Control 2 X", "Control 2 Y"]
                .into_iter()
                .enumerate()
            {
                number(ui, label, &mut self.tools.curve_points[i]);
            }
        }
        if ui
            .button(crate::i18n::tr("Apply curve / window to selection"))
            .clicked()
        {
            self.transform_selection(Transform::Curve {
                easing: self.tools.easing,
                window: self.tools.curve_window,
                bezier: self.tools.curve_bezier.then_some(self.tools.curve_points),
            });
        }
        if ui
            .button(crate::i18n::tr("Select matching X/Y intervals"))
            .clicked()
        {
            self.select_xy_pairs();
        }
        ui.small(RichText::new(crate::i18n::tr("Event tracks reject overlaps. Join requires matching metadata and slopes, so the resulting motion stays the same.")).color(muted()));
    }
}
