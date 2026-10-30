use super::*;

impl Workbench {
    pub(crate) fn workspace_window(&mut self, ctx: &egui::Context) {
        let Some(view) = self.workspace.view.clone() else {
            return;
        };
        if let Some(image) = self
            .workspace
            .inspection
            .as_mut()
            .and_then(|i| i.image.take())
        {
            self.workspace.image = Some(ctx.load_texture(
                "resource-inspection",
                egui::ColorImage::from_rgba_unmultiplied(image.size, &image.rgba),
                egui::TextureOptions::LINEAR,
            ));
        }
        let title = match view {
            View::Patterns => "Patterns",
            View::Resources => "Project resources",
            View::Versions(_) => "Project versions",
            View::Project(_) => "Manage project",
        };
        let mut open = true;
        let is_versions = matches!(view, View::Versions(_));
        let window=crate::i18n::window(title)
            .id(egui::Id::new(if is_versions {"repository-window"}else{"workspace-window"}))
            .open(&mut open).default_pos([24.,48.])
            .default_size(if is_versions {[860.,500.]}else{[560.,360.]})
            .min_width(240.)
            .resize(|r|r.max_size([(ctx.screen_rect().width()-40.).max(240.),(ctx.screen_rect().height()-90.).max(100.)]))
            .show(ctx,|ui| {
                ui.set_enabled(!self.file_busy);
                if let View::Versions(workspace)=view.clone() {
                    self.versions_ui(ui,workspace);
                    return;
                }
                egui::ScrollArea::vertical().id_source("workspace-content")
                    .max_width(ui.available_width()).max_height((ctx.screen_rect().height()-110.).max(40.))
                    .touch_show_y(ui,|ui| {
                        if self.workspace.loading {ui.spinner();return;}
                        match view.clone() {
                            View::Patterns=>self.patterns_ui(ui),
                            View::Resources=>self.resources_ui(ui),
                            View::Versions(_)=>unreachable!(),
                            View::Project(workspace)=>{
                                ui.label(crate::i18n::tr("Project name in Projects"));
                                ui.text_edit_singleline(&mut self.workspace.name);
                                ui.horizontal_wrapped(|ui| {
                                    if ui.button(crate::i18n::tr("Rename")).clicked(){self.workspace_request(WorkspaceJob::Rename{workspace:workspace.clone(),name:self.workspace.name.clone()});}
                                    if ui.button(crate::i18n::tr("Duplicate project")).clicked(){self.workspace_request(WorkspaceJob::Duplicate{workspace:workspace.clone(),name:self.workspace.name.clone()});}
                                    if ui.button(crate::i18n::tr("Versions…")).clicked(){self.open_workspace(View::Versions(workspace.clone()));}
                                });
                                ui.label(crate::i18n::tr("Duplicating includes the current unsaved chart and project resources."));
                            }
                        }
                        if let Some(error)=&self.error {ui.colored_label(crate::theme::flick(),crate::i18n::tr(error));}
                    });
            });
        #[cfg(test)]
        if let Some(window) = &window {
            self.workspace.rect = window.response.rect;
        }
        #[cfg(not(test))]
        let _ = window;
        if !open {
            self.workspace.view = None;
            self.workspace.audition = Some(None);
        }
    }

    pub(super) fn workspace_delete_button(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        action: &mut Option<WorkspaceJob>,
        job: WorkspaceJob,
    ) {
        if self.workspace.confirm_delete.as_deref() == Some(id) {
            if ui
                .button(
                    RichText::new(crate::i18n::tr("Confirm delete")).color(crate::theme::flick()),
                )
                .clicked()
            {
                *action = Some(job);
                self.workspace.confirm_delete = None;
            }
            if ui.button(crate::i18n::tr("Cancel")).clicked() {
                self.workspace.confirm_delete = None;
            }
        } else if ui.button(crate::i18n::tr("Delete")).clicked() {
            self.workspace.confirm_delete = Some(id.into());
        }
    }
}
