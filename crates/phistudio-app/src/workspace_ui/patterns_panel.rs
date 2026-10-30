use super::*;

impl Workbench {
    pub(super) fn patterns_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(crate::i18n::tr("Name"));
            ui.add(egui::TextEdit::singleline(&mut self.workspace.name).desired_width(170.));
            if ui
                .add_enabled(
                    !self.selection().is_empty() || !self.story_selection().is_empty(),
                    egui::Button::new(crate::i18n::tr("Save selection")),
                )
                .clicked()
            {
                match ObjectClipboard::copy(
                    self.editor.document(),
                    &self.selection(),
                    &self.story_selection(),
                ) {
                    Ok(c) => self.workspace_request(WorkspaceJob::SavePattern {
                        name: self.workspace.name.clone(),
                        clipboard: Arc::new(c),
                    }),
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
        });
        ui.separator();
        let mut action = None;
        for p in &self.workspace.patterns {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .selectable_label(self.workspace.selected_pattern.as_ref() == Some(&p.id), {
                        let __i18n_arg_0 = &(p.name);
                        let __i18n_arg_1 = &(p.objects);
                        let __i18n_arg_2 = &(p.lines);
                        let __i18n_arg_3 = &(p.duration);
                        crate::i18n::format(
                            "{0} · {1} objects · {2} lines · {3} beats",
                            &[
                                __i18n_arg_0.to_string(),
                                __i18n_arg_1.to_string(),
                                __i18n_arg_2.to_string(),
                                format!("{:.3}", __i18n_arg_3),
                            ],
                        )
                    })
                    .clicked()
                {
                    self.workspace.selected_pattern = Some(p.id.clone());
                    self.workspace.confirm_delete = None;
                }
                if ui.button(if p.favorite { "★" } else { "☆" }).clicked() {
                    action = Some(WorkspaceJob::Favorite {
                        id: p.id.clone(),
                        value: !p.favorite,
                    });
                }
            });
        }
        if self.workspace.patterns.is_empty() {
            ui.label(crate::i18n::tr(
                "Select notes, events or Storyboard events, then save a pattern.",
            ));
        }
        if let Some(id) = self.workspace.selected_pattern.clone() {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(crate::i18n::tr("Target line ID"));
                ui.add(crate::numeric::Value::new(&mut self.workspace.line).clamp_range(0..=4095));
                ui.label(crate::i18n::tr("Layer"));
                ui.add(crate::numeric::Value::new(&mut self.workspace.layer).clamp_range(0..=3));
                ui.label(crate::i18n::tr("Time scale"));
                ui.add(
                    crate::numeric::Value::new(&mut self.workspace.scale)
                        .speed(0.05)
                        .clamp_range(0.01..=100.),
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(crate::i18n::tr("X offset"));
                ui.add(crate::numeric::Value::new(&mut self.workspace.offset[0]).speed(1.));
                ui.label(crate::i18n::tr("Y offset"));
                ui.add(crate::numeric::Value::new(&mut self.workspace.offset[1]).speed(1.));
                ui.checkbox(&mut self.workspace.mirrored, crate::i18n::tr("Mirror X"));
            });
            ui.label({ let __i18n_arg_0 = &(self.beat); crate::i18n::format("Insert at beat {0}. Missing target lines are created. Events use the destination line's textures and controls.", &[format!("{:.3}", __i18n_arg_0)]) });
            ui.horizontal_wrapped(|ui| {
                if ui.button(crate::i18n::tr("Insert pattern")).clicked() {
                    match (
                        Beat::from_grid(self.beat, self.division),
                        Beat::from_grid(self.workspace.scale, 1000),
                    ) {
                        (Ok(beat), Ok(scale)) => {
                            action = Some(WorkspaceJob::InsertPattern {
                                id: id.clone(),
                                settings: Insertion {
                                    line: self.workspace.line,
                                    layer: self.workspace.layer,
                                    beat,
                                    scale,
                                    offset: self.workspace.offset,
                                    mirrored: self.workspace.mirrored,
                                },
                            })
                        }
                        _ => self.error = Some("Invalid pattern timing".into()),
                    }
                }
                self.workspace_delete_button(
                    ui,
                    &id,
                    &mut action,
                    WorkspaceJob::DeletePattern(id.clone()),
                );
            });
        }
        if let Some(job) = action {
            self.workspace_request(job);
        }
    }
}
