use super::*;
use crate::{
    i18n::{format, tr},
    workbench::Workbench,
};

impl Workbench {
    pub(crate) fn updates_ui(&mut self, ui: &mut egui::Ui) {
        ui.strong(tr("Application updates"));
        ui.label(format(
            "Installed version: {0}",
            &[service::installed_version().into()],
        ));
        ui.checkbox(
            &mut self.options.check_updates_on_start,
            tr("Check for updates on startup"),
        );
        ui.small(tr(
            "Checks stable GitHub releases in the background. Updates are installed manually.",
        ));
        if ui
            .add_enabled(
                self.updates.pending.is_none(),
                egui::Button::new(tr("Check for updates now")),
            )
            .clicked()
        {
            self.updates.start(false);
        }
        match &self.updates.status {
            Status::Idle => {}
            Status::Checking => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr("Checking for updates…"));
                });
            }
            Status::Current(release) => {
                ui.label(tr("Your version is up to date."));
                ui.small(format(
                    "Latest stable version: {0}",
                    &[release.version.to_string()],
                ));
            }
            Status::Available(release) => {
                ui.label(format(
                    "A new version is available: {0}",
                    &[release.version.to_string()],
                ));
                ui.hyperlink_to(tr("Open release"), &release.url);
            }
            Status::Failed(error) => {
                ui.label(tr(error.message()));
            }
        }
        ui.hyperlink_to(tr("All releases"), service::RELEASES);
    }
    pub(crate) fn update_banner(&mut self, ctx: &egui::Context) {
        let Some(release) = self.updates.notification().cloned() else {
            return;
        };
        let enabled = !self.modal_open_except_projects();
        egui::TopBottomPanel::top("application-update")
            .frame(
                egui::Frame::none()
                    .fill(crate::theme::panel())
                    .inner_margin(8.),
            )
            .show(ctx, |ui| {
                ui.set_enabled(enabled);
                ui.style_mut().wrap = Some(true);
                ui.label(format(
                    "A new version is available: {0}",
                    &[release.version.to_string()],
                ));
                ui.horizontal_wrapped(|ui| {
                    ui.hyperlink_to(tr("Open release"), &release.url);
                    if ui.button(tr("Later")).clicked() {
                        self.updates.dismissed = true;
                    }
                });
            });
    }
}
