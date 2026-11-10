use super::*;

impl Workbench {
    pub(in super::super) fn project_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(snapshot) = &self.files.recovery {
            let name = snapshot.document.name().to_owned();
            crate::i18n::window("Restore unsaved changes?").collapsible(false).resizable(false)
                .default_width((ctx.screen_rect().width() - 80.).clamp(260., 420.))
                .anchor(Align2::CENTER_CENTER, [0., 0.]).show(ctx, |ui| {
                    ui.label(RichText::new(name).strong());
                    ui.label(crate::i18n::tr("A recovery copy contains edits from your previous session. Your saved project is unchanged."));
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(crate::i18n::tr("Restore changes")).clicked() { self.files.request = Some(FileRequest::RestoreRecovery); }
                        if ui.button(RichText::new(crate::i18n::tr("Discard recovery")).color(flick())).clicked() { self.files.request = Some(FileRequest::DiscardRecovery); }
                    });
                });
        }
        if let Some((workspace, name)) = self.files.delete_project.clone() {
            let mut open = true;
            crate::i18n::window("Delete project?").open(&mut open).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0., 0.]).default_width(340.).show(ctx, |ui| {
                ui.label(RichText::new(&name).strong());
                ui.label(crate::i18n::tr("Move this local project and its resources to Trash? You can undo the deletion from Projects."));
                if self.editor.is_dirty() && self.file.as_ref().is_some_and(|path| path.starts_with(&workspace)) { ui.colored_label(flick(), crate::i18n::tr("Unsaved edits in this project will be discarded.")); }
                ui.horizontal(|ui| {
                    if ui.button(RichText::new(crate::i18n::tr("Move to Trash")).color(flick())).clicked() { self.files.request = Some(FileRequest::TrashProject { workspace, name }); self.files.delete_project = None; }
                    if ui.button(crate::i18n::tr("Cancel")).clicked() { self.files.delete_project = None; }
                });
            });
            if !open {
                self.files.delete_project = None;
            }
        }
        if let Some(draft) = &mut self.files.new_chart {
            let mut open = true;
            let mut create = false;
            let mut pick = None;
            let error = self.error.clone();
            let available = !self.files.busy && self.files.media.is_none();
            crate::i18n::window("Create chart")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(350.)
                .anchor(Align2::CENTER_CENTER, [0., 0.])
                .show(ctx, |ui| {
                    ui.set_enabled(available);
                    egui::ScrollArea::vertical()
                        .id_source("new-chart-scroll")
                        .max_height((ctx.screen_rect().height() - 80.).max(100.))
                        .touch_show(ui, |ui| {
                            egui::Grid::new("new-chart-fields")
                                .num_columns(2)
                                .show(ui, |ui| {
                                    ui.label(crate::i18n::tr("Name"));
                                    ui.text_edit_singleline(&mut draft.name);
                                    ui.end_row();
                                    ui.label(crate::i18n::tr("BPM"));
                                    ui.add(
                                        crate::numeric::Value::new(&mut draft.bpm)
                                            .clamp_range(1. ..=1000.)
                                            .speed(1.),
                                    );
                                    ui.end_row();
                                });
                            for (kind, media) in [
                                (MediaKind::Audio, &draft.audio),
                                (MediaKind::Illustration, &draft.illustration),
                            ] {
                                ui.add_space(5.);
                                ui.horizontal(|ui| {
                                    if ui
                                        .button({
                                            let __i18n_arg_0 = &(crate::i18n::tr(kind.label()));
                                            crate::i18n::format(
                                                "Choose {0}…",
                                                std::slice::from_ref(__i18n_arg_0),
                                            )
                                        })
                                        .clicked()
                                    {
                                        pick = Some(kind);
                                    }
                                    ui.label(
                                        RichText::new(crate::i18n::tr(if media.is_some() {
                                            "Selected"
                                        } else {
                                            "Required"
                                        }))
                                        .color(if media.is_some() { accent() } else { muted() }),
                                    );
                                });
                                if let Some(media) = media {
                                    ui.small(media.name());
                                }
                            }
                            ui.collapsing(crate::i18n::tr("Chart details"), |ui| {
                                egui::Grid::new("new-chart-details").num_columns(2).show(
                                    ui,
                                    |ui| {
                                        ui.label(crate::i18n::tr("Level"));
                                        ui.text_edit_singleline(&mut draft.level);
                                        ui.end_row();
                                        ui.label(crate::i18n::tr("Composer"));
                                        ui.text_edit_singleline(&mut draft.composer);
                                        ui.end_row();
                                        ui.label(crate::i18n::tr("Charter"));
                                        ui.text_edit_singleline(&mut draft.charter);
                                        ui.end_row();
                                    },
                                );
                            });
                            if let Some(error) = error {
                                ui.colored_label(flick(), crate::i18n::tr(error));
                            }
                            ui.add_space(8.);
                            create = ui
                                .add_enabled(
                                    draft.ready(),
                                    egui::Button::new(crate::i18n::tr("Create chart")),
                                )
                                .clicked();
                        });
                });
            if create && let Some(media) = draft.media() {
                let document =
                    ChartDocument::empty(draft.name.trim(), draft.bpm).and_then(|document| {
                        let mut value = document.as_json().clone();
                        value["META"]["level"] = draft.level.clone().into();
                        value["META"]["composer"] = draft.composer.clone().into();
                        value["META"]["charter"] = draft.charter.clone().into();
                        ChartDocument::from_value(value)
                    });
                match document {
                    Ok(document) => {
                        self.files.request = Some(FileRequest::CreateProject { document, media })
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            if !open && !self.files.busy {
                self.files.new_chart = None;
                self.files.media = None;
            }
            if let Some(kind) = pick {
                self.open_media_picker(kind);
            }
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        if let Some(kind) = self.files.media {
            let mut open = true;
            crate::window::Window::new(
                crate::i18n::format("Choose {0}", &[crate::i18n::tr(kind.label())]),
                egui::Id::new(("media-picker", kind.label())),
            )
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0., 0.])
            .show(ctx, |ui| {
                ui.label(crate::i18n::tr("File path"));
                ui.add(egui::TextEdit::singleline(&mut self.files.media_path).desired_width(350.));
                if ui
                    .add_enabled(
                        !self.files.media_path.trim().is_empty(),
                        egui::Button::new(crate::i18n::tr("Use file")),
                    )
                    .clicked()
                {
                    self.files.request = Some(FileRequest::MediaPath {
                        kind,
                        path: std::path::PathBuf::from(self.files.media_path.trim()),
                    });
                    self.files.media = None;
                }
            });
            if !open {
                self.files.media = None;
            }
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        if let Some(package) = &self.files.export_package {
            let mut open = true;
            let mut write = false;
            crate::i18n::window(if package.name.ends_with(".phisounds.json") {
                "Export sound pack…"
            } else if package.name.ends_with(".phiversions.zip") {
                "Export history…"
            } else {
                "Export project"
            })
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0., 0.])
            .show(ctx, |ui| {
                ui.label(&package.name);
                ui.label(crate::i18n::tr(
                    if package.name.ends_with(".phisounds.json") {
                        "Sound pack destination (.phisounds.json)"
                    } else if package.name.ends_with(".phiversions.zip") {
                        "History archive path"
                    } else {
                        "Package destination (.pez)"
                    },
                ));
                ui.add(egui::TextEdit::singleline(&mut self.files.export_path).desired_width(360.));
                write = ui
                    .add_enabled(
                        !self.files.export_path.trim().is_empty(),
                        egui::Button::new(crate::i18n::tr(
                            if package.name.ends_with(".phisounds.json") {
                                "Export sound pack…"
                            } else {
                                "Export project"
                            },
                        )),
                    )
                    .clicked();
            });
            if write {
                self.files.request = Some(FileRequest::WriteExport(
                    self.files.export_path.trim().into(),
                ));
            }
            if !open {
                self.files.export_package = None;
            }
        }
    }
}
