use super::*;

impl Workbench {
    pub(super) fn line_properties_ui(&mut self, ui: &mut egui::Ui) {
        self.judge_line_selector(ui);
        ui.collapsing(crate::i18n::tr("Manage lines"), |ui| {
            self.line_management_controls(ui)
        });
        let revision = self.editor.revision();
        if self
            .line_draft
            .as_ref()
            .is_none_or(|v| v.index != self.line || v.revision != revision)
        {
            match self.editor.document().line_properties(self.line) {
                Ok(properties) => {
                    self.line_draft = Some(LineDraft {
                        index: self.line,
                        revision,
                        properties,
                    })
                }
                Err(e) => {
                    ui.colored_label(flick(), crate::i18n::tr(e.to_string()));
                    return;
                }
            }
        }
        let line_summaries = self.line_summaries();
        let draft = self.line_draft.as_mut().unwrap();
        let p = &mut draft.properties;
        let mut choose_texture = false;
        let max_column = ((ui.available_width() - 10.) * 0.45).max(60.);
        egui::Grid::new("line-fields")
            .min_col_width(0.)
            .max_col_width(max_column)
            .num_columns(2)
            .spacing([10., 7.])
            .show(ui, |ui| {
                ui.label(crate::i18n::tr("Name"));
                ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(170.));
                ui.end_row();
                ui.label(crate::i18n::tr("Texture"));
                ui.vertical(|ui| {
                    choose_texture = ui.button(crate::i18n::tr("Choose texture…")).clicked();
                    ui.label(&p.texture);
                });
                ui.end_row();
                for (axis,label) in ["Anchor X", "Anchor Y"].into_iter().enumerate() {
                    ui.label(crate::i18n::tr(label)).on_hover_text(crate::i18n::tr("0 = left/bottom, 0.5 = center. Moves the line image without moving notes."));
                    ui.add(crate::numeric::Value::new(&mut p.anchor[axis]).speed(0.01).max_decimals(5));
                    ui.end_row();
                }
                ui.label(crate::i18n::tr("Group"));
                let names = self.editor.document().group_names().unwrap_or_default();
                egui::ComboBox::from_id_source("line-group")
                    .selected_text(names.get(p.group.max(0) as usize).cloned().unwrap_or_else(
                        || crate::i18n::format("Group {0}", &[p.group.to_string()]),
                    ))
                    .touch_show_ui(ui, |ui| {
                        for (i, name) in names.iter().enumerate() {
                            ui.selectable_value(&mut p.group, i as i32, format!("{i} · {name}"));
                        }
                    });
                ui.end_row();
                ui.label(crate::i18n::tr("BPM factor"));
                ui.add(
                    crate::numeric::Value::new(&mut p.bpm_factor)
                        .speed(0.01)
                        .max_decimals(5),
                )
                .on_hover_text(crate::i18n::tr(
                    "Saved in RPE. The current prpr preview does not apply this factor.",
                ));
                ui.end_row();
                ui.label(crate::i18n::tr("Z order"));
                ui.add(crate::numeric::Value::new(&mut p.z_order));
                ui.end_row();
                ui.label(crate::i18n::tr("Parent"));
                egui::ComboBox::from_id_source("parent-line")
                    .width(170.)
                    .selected_text(if p.father < 0 {
                        crate::i18n::tr("None")
                    } else {
                        crate::i18n::format("Line {0}", &[p.father.to_string()])
                    })
                    .touch_show_ui(ui, |ui| {
                        ui.selectable_value(&mut p.father, -1, crate::i18n::tr("None"));
                        for line in line_summaries.iter() {
                            if line.index != self.line {
                                ui.selectable_value(
                                    &mut p.father,
                                    line.index as i32,
                                    format!("{} · {}", line.index, line.name),
                                );
                            }
                        }
                    });
                ui.end_row();
                ui.label(crate::i18n::tr("Inherit rotation"));
                ui.checkbox(&mut p.rotate_with_father, "");
                ui.end_row();
                ui.label(crate::i18n::tr("Cover notes"));
                let mut cover = p.cover != 0;
                if ui.checkbox(&mut cover, "").changed() {
                    p.cover = u8::from(cover);
                }
                ui.end_row();
                ui.label(crate::i18n::tr("HUD binding"));
                egui::ComboBox::from_id_source("hud-binding")
                    .width(170.)
                    .selected_text(crate::i18n::tr(
                        p.attach_ui.map_or("None", AttachedUi::label),
                    ))
                    .touch_show_ui(ui, |ui| {
                        ui.selectable_value(&mut p.attach_ui, None, crate::i18n::tr("None"));
                        for binding in AttachedUi::ALL {
                            ui.selectable_value(
                                &mut p.attach_ui,
                                Some(binding),
                                crate::i18n::tr(binding.label()),
                            );
                        }
                    });
                ui.end_row();
            });
        let apply = ui.button(crate::i18n::tr("Apply line")).clicked();
        if apply {
            let properties = draft.properties.clone();
            self.commit_authoring(Command::EditLine {
                index: self.line,
                properties,
            });
        }
        if choose_texture {
            self.open_line_textures();
        }
    }

    fn judge_line_selector(&mut self, ui: &mut egui::Ui) {
        let summaries = self.line_summaries();
        let before = self.line;
        ui.horizontal(|ui| {
            ui.label(crate::i18n::format("Line {0}", &[self.line.to_string()]));
            egui::ComboBox::from_id_source("judge-properties-line")
                .width((ui.available_width() - 8.).min(360.))
                .selected_text(
                    summaries
                        .get(self.line)
                        .map_or("", |line| line.name.as_str()),
                )
                .touch_show_ui(ui, |ui| {
                    for line in summaries.iter() {
                        ui.selectable_value(
                            &mut self.line,
                            line.index,
                            format!("L{} · {}", line.index, line.name),
                        );
                    }
                });
        });
        if before != self.line {
            self.select(None);
            self.line_draft = None;
            self.timeline.cancel_gesture();
        }
    }

    fn line_management_controls(&mut self, ui: &mut egui::Ui) {
        use crate::i18n::tr;
        let mut action = None;
        let count = self.editor.document().line_count();
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("New lines"));
            ui.add(
                crate::numeric::Value::new(&mut self.line_tools.count)
                    .clamp_range(1..=phistudio_core::line_management::MAX_NEW_LINES),
            );
            if ui.button(tr("Create lines")).clicked() {
                action = Some(Command::AddLines {
                    count: self.line_tools.count,
                });
            }
        });
        ui.small(crate::i18n::format(
            "Total lines: {0} → {1}",
            &[
                count.to_string(),
                count.saturating_add(self.line_tools.count).to_string(),
            ],
        ));
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr("Duplicate line")).clicked() {
                action = Some(Command::DuplicateLine(self.line));
            }
            if ui
                .add_enabled(self.line > 0, egui::Button::new(tr("Move up")))
                .clicked()
            {
                action = Some(Command::MoveLine {
                    from: self.line,
                    to: self.line - 1,
                });
            }
            if ui
                .add_enabled(self.line + 1 < count, egui::Button::new(tr("Move down")))
                .clicked()
            {
                action = Some(Command::MoveLine {
                    from: self.line,
                    to: self.line + 1,
                });
            }
            if ui
                .add_enabled(
                    self.line != 0,
                    egui::Button::new(RichText::new(tr("Delete line")).color(flick())),
                )
                .clicked()
            {
                action = Some(Command::DeleteLine(self.line));
            }
        });
        if let Some(action) = action {
            self.commit_authoring(action);
        }
    }
}
