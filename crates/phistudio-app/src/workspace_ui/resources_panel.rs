use super::*;

impl Workbench {
    pub(super) fn resources_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Import resource…")).clicked() {
                self.workspace.resource_target = None;
                self.open_media_picker(MediaKind::Resource);
            }
            if ui
                .add_enabled(
                    self.workspace.resource_undo.is_some(),
                    egui::Button::new(crate::i18n::tr("Undo resource change")),
                )
                .clicked()
                && let Some(change) = self.workspace.resource_undo.take()
            {
                self.workspace_request(WorkspaceJob::UndoResource(change));
            }
            if ui.button(crate::i18n::tr("Reload list")).clicked() {
                self.workspace_request(WorkspaceJob::Resources);
            }
        });
        ui.label(crate::i18n::tr("Replace keeps the resource path. Referenced resources cannot be deleted. Resource changes have their own Undo."));
        if self.workspace.resources.is_empty() {
            ui.label(crate::i18n::tr("No project resources."));
        }
        let mut inspect = None;
        for r in &self.workspace.resources {
            if ui
                .selectable_label(
                    self.workspace.selected_resource.as_ref() == Some(&r.name),
                    {
                        let __i18n_arg_0 = &(r.name);
                        let __i18n_arg_1 = &(crate::i18n::tr(r.kind.label()));
                        let __i18n_arg_2 = &(r.bytes as f64 / 1024.);
                        let __i18n_arg_3 = &(r.references);
                        crate::i18n::format(
                            "{0} · {1} · {2} KiB · {3} references",
                            &[
                                __i18n_arg_0.to_string(),
                                __i18n_arg_1.to_string(),
                                format!("{:.1}", __i18n_arg_2),
                                __i18n_arg_3.to_string(),
                            ],
                        )
                    },
                )
                .clicked()
            {
                self.workspace.selected_resource = Some(r.name.clone());
                self.workspace.confirm_delete = None;
                self.workspace.inspection = None;
                self.workspace.image = None;
                self.workspace.audition = Some(None);
                inspect = Some(r.name.clone());
            }
        }
        if let Some(name) = inspect {
            self.workspace_request(WorkspaceJob::InspectResource(name));
        }
        if let Some(image) = &self.workspace.image {
            let size = image.size_vec2();
            let scale = (ui.available_width() / size.x).min(1.);
            ui.image(image.id(), size * scale);
        }
        if let Some(inspection) = &self.workspace.inspection {
            ui.label(&inspection.details);
            if let Some(clip) = &inspection.audio {
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::tr("Play sound")).clicked() {
                        self.workspace.audition = Some(Some(clip.clone()));
                    }
                    if ui.button(crate::i18n::tr("Stop sound")).clicked() {
                        self.workspace.audition = Some(None);
                    }
                });
            }
        }
        if let Some(name) = self.workspace.selected_resource.clone() {
            let used = self
                .workspace
                .resources
                .iter()
                .find(|r| r.name == name)
                .is_some_and(|r| r.references > 0);
            let mut action = None;
            ui.horizontal_wrapped(|ui| {
                if ui.button(crate::i18n::tr("Replace from file…")).clicked() {
                    self.workspace.resource_target = Some(name.clone());
                    self.open_media_picker(MediaKind::Resource);
                }
                ui.add_enabled_ui(!used, |ui| {
                    self.workspace_delete_button(
                        ui,
                        &name,
                        &mut action,
                        WorkspaceJob::DeleteResource(name.clone()),
                    )
                });
            });
            if let Some(job) = action {
                self.workspace_request(job);
            }
        }
    }
}
