use super::*;

impl Workbench {
    pub(crate) fn open_chart_properties(&mut self) {
        self.advanced.metadata = Some((
            self.editor.revision(),
            ChartMetadata::read(self.editor.document(), &self.project_options),
        ));
    }

    pub(crate) fn advanced_windows(&mut self, ctx: &egui::Context) {
        if self.show_projects {
            return;
        }
        self.clone_window(ctx);
        self.shortcuts_window(ctx);
        if let Some((revision, mut meta)) = self.advanced.metadata.take() {
            let mut open = true;
            let mut apply = false;
            window("Chart properties", ctx)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.set_enabled(!self.modal_open());
                    egui::ScrollArea::vertical()
                        .id_source("metadata-scroll")
                        .max_height(ui.available_height().max(90.))
                        .touch_show(ui, |ui| {
                            for (label, text) in [
                                ("Name", &mut meta.name),
                                ("Level", &mut meta.level),
                                ("Composer", &mut meta.composer),
                                ("Charter", &mut meta.charter),
                            ] {
                                ui.label(crate::i18n::tr(label));
                                ui.text_edit_singleline(text);
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.label(crate::i18n::tr("Chart offset (ms)"));
                                ui.add(
                                    crate::numeric::Value::new(&mut meta.offset_ms)
                                        .speed(1.)
                                        .max_decimals(3),
                                );
                            });
                            ui.small({
                                let __i18n_arg_0 = &(self.project_options.manifest_offset_ms);
                                let __i18n_arg_1 = &(meta.offset_ms
                                    + self.project_options.manifest_offset_ms as f64);
                                crate::i18n::format(
                                    "Package offset: {0} ms · Effective offset: {1} ms",
                                    &[__i18n_arg_0.to_string(), format!("{:.3}", __i18n_arg_1)],
                                )
                            });
                            apply = ui
                                .button(crate::i18n::tr("Apply chart properties"))
                                .clicked();
                        });
                });
            if apply {
                if revision == self.editor.revision() {
                    self.commit_authoring(Command::EditMetadata(meta.clone()));
                } else {
                    self.error =
                        Some("Chart changed. Reopen Chart properties before applying.".into());
                }
            }
            if open && !apply {
                self.advanced.metadata = Some((revision, meta));
            }
        }
        if self.advanced.monitor {
            let mut open = true;
            window("Line monitor", ctx).open(&mut open).show(ctx, |ui| {
                ui.set_enabled(!self.modal_open());
                egui::ScrollArea::vertical().id_source("line-monitor")
                    .max_height(ui.available_height().max(90.)).touch_show(ui, |ui| {
                        for (i, line) in self.line_summaries().iter().enumerate() {
                            if let Some(state) = self.advanced.line_states.get(i)
                                && ui.selectable_label(i == self.line, { let __i18n_arg_0 = &(i); let __i18n_arg_1 = &(line.name); let __i18n_arg_2 = &(state.x); let __i18n_arg_3 = &(state.y); let __i18n_arg_4 = &(state.rotation); let __i18n_arg_5 = &(state.alpha); let __i18n_arg_6 = &(state.remaining); crate::i18n::format("L{0} · {1} · X {2} Y {3} · {4}° · Alpha {5} · {6} remaining", &[__i18n_arg_0.to_string(), __i18n_arg_1.to_string(), format!("{:.1}", __i18n_arg_2), format!("{:.1}", __i18n_arg_3), format!("{:.1}", __i18n_arg_4), format!("{:.1}", __i18n_arg_5), __i18n_arg_6.to_string()]) }).clicked() {
                                self.line = i;
                                self.select(None);
                            }
                        }
                    });
            });
            self.advanced.monitor = open;
        }
        if self.advanced.history {
            let mut open = true;
            window("History", ctx).open(&mut open).show(ctx, |ui| {
                ui.set_enabled(!self.modal_open());
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.editor.can_undo(),
                            egui::Button::new(crate::i18n::tr("Undo")),
                        )
                        .clicked()
                    {
                        self.history_step(false);
                    }
                    if ui
                        .add_enabled(
                            self.editor.can_redo(),
                            egui::Button::new(crate::i18n::tr("Redo")),
                        )
                        .clicked()
                    {
                        self.history_step(true);
                    }
                });
                ui.checkbox(
                    &mut self.options.history_jump,
                    crate::i18n::tr("Jump to changed objects on Undo / Redo"),
                );
                egui::ScrollArea::vertical()
                    .id_source("history-scroll")
                    .max_height(ui.available_height().max(90.))
                    .touch_show(ui, |ui| {
                        for (i, (label, applied)) in self.editor.history_rows().iter().enumerate() {
                            ui.label(format!(
                                "{} {} · {}",
                                crate::i18n::tr(if *applied { "Done" } else { "Redo" }),
                                i + 1,
                                crate::i18n::tr(label)
                            ));
                        }
                    });
            });
            self.advanced.history = open;
        }
    }
}
