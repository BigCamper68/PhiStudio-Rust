use super::*;

impl Workbench {
    pub(super) fn bpm_ui(&mut self, ui: &mut egui::Ui) {
        let mut rows = self.editor.document().bpm_changes();
        rows.sort_by_key(|(_, v)| v.start);
        let first = rows[0].0;
        let revision = self.editor.revision();
        if self
            .bpm_draft
            .as_ref()
            .is_some_and(|v| v.revision != revision)
        {
            self.bpm_draft = None;
        }
        let selected = self.bpm_draft.as_ref().and_then(|v| v.index);
        egui::ScrollArea::vertical()
            .id_source("bpm-list")
            .max_height(140.)
            .touch_show(ui, |ui| {
                for (index, change) in &rows {
                    if ui
                        .selectable_label(selected == Some(*index), {
                            let __i18n_arg_0 = &(change.start);
                            let __i18n_arg_1 = &(change.bpm);
                            let __i18n_arg_2 = &(crate::i18n::tr(if *index == first {
                                "  ·  first"
                            } else {
                                ""
                            }));
                            crate::i18n::format(
                                "Beat {0}  ·  {1} BPM{2}",
                                &[
                                    __i18n_arg_0.to_string(),
                                    format!("{:.3}", __i18n_arg_1),
                                    __i18n_arg_2.to_string(),
                                ],
                            )
                        })
                        .clicked()
                    {
                        self.bpm_draft = Some(BpmDraft {
                            index: Some(*index),
                            revision,
                            start: change.start.to_string(),
                            bpm: change.bpm,
                        });
                    }
                }
            });
        if ui.button(crate::i18n::tr("+ BPM change")).clicked() {
            let at = Beat::from_grid(self.beat, self.division)
                .unwrap_or(Beat::ZERO)
                .max(
                    rows.last()
                        .unwrap()
                        .1
                        .start
                        .checked_add(Beat::ONE)
                        .unwrap_or(Beat::ONE),
                );
            self.bpm_draft = Some(BpmDraft {
                index: None,
                revision,
                start: at.to_string(),
                bpm: self.editor.document().tempo().bpm_at(self.beat),
            });
        }
        let Some(draft) = self.bpm_draft.as_mut() else {
            ui.small(crate::i18n::tr("Select a BPM entry or add a change."));
            return;
        };
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Beat"));
            ui.add_enabled(
                draft.index != Some(first),
                egui::TextEdit::singleline(&mut draft.start).desired_width(155.),
            );
        });
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("BPM"));
            ui.add(
                crate::numeric::Value::new(&mut draft.bpm)
                    .speed(0.1)
                    .max_decimals(5),
            );
        });
        let apply = ui.button(crate::i18n::tr("Apply BPM")).clicked();
        let delete = ui
            .add_enabled(
                draft.index.is_some_and(|v| v != first),
                egui::Button::new(RichText::new(crate::i18n::tr("Delete BPM")).color(flick())),
            )
            .clicked();
        if apply {
            match draft.start.parse::<Beat>() {
                Ok(start) => {
                    let change = BpmChange {
                        start,
                        bpm: draft.bpm,
                    };
                    let command =
                        draft
                            .index
                            .map_or(Command::AddBpm(change), |index| Command::UpdateBpm {
                                index,
                                change,
                            });
                    self.commit_authoring(command);
                }
                Err(e) => self.error = Some(e.to_string()),
            }
        } else if delete {
            let index = draft.index.unwrap();
            self.commit_authoring(Command::DeleteBpm(index));
        }
    }
}
