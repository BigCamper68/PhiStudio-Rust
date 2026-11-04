use super::*;
use crate::scroll::TouchScroll as _;

#[derive(Clone)]
pub(super) struct Dialog {
    pub origin: Origin,
    pub panel: String,
    pub version: DocumentVersion,
}

impl State {
    pub(super) fn dismiss_dialog(&mut self) -> bool {
        let Some(dialog) = self.dialog.take() else {
            return false;
        };
        if let Some(panel) = self
            .panels
            .get_mut(&(dialog.origin.plugin.clone(), dialog.panel.clone()))
            && panel.origin == dialog.origin
        {
            panel.open = false;
        }
        self.closed_dialogs.push((dialog.origin, dialog.panel));
        true
    }
}

impl Workbench {
    pub(crate) fn plugin_dialog_modal(&self) -> bool {
        !self.show_projects && self.plugins.dialog.is_some()
    }

    pub(super) fn maintain_plugin_dialog(&mut self) {
        let valid = self.plugins.dialog.as_ref().is_none_or(|dialog| {
            dialog.version == self.plugin_version()
                && self
                    .plugins
                    .runtime
                    .as_ref()
                    .is_some_and(|r| r.is_current(&dialog.origin))
                && self
                    .plugins
                    .panels
                    .get(&(dialog.origin.plugin.clone(), dialog.panel.clone()))
                    .is_some_and(|p| p.open && p.origin == dialog.origin)
        });
        if !valid {
            self.plugins.dismiss_dialog();
        }
        for (origin, panel) in std::mem::take(&mut self.plugins.closed_dialogs) {
            if self
                .plugins
                .runtime
                .as_ref()
                .is_some_and(|r| r.is_current(&origin))
            {
                self.plugin_ui_event(
                    origin.plugin,
                    UiEvent {
                        panel,
                        widget: "dialog.close".into(),
                        value: json!(false),
                    },
                    false,
                );
            }
        }
    }

    pub(super) fn plugin_dialog_window(&mut self, ctx: &egui::Context) {
        if self.show_projects {
            return;
        }
        let Some(dialog) = self.plugins.dialog.clone() else {
            return;
        };
        let title = self
            .plugins
            .registry
            .plugins
            .iter()
            .filter(|p| p.origin.as_ref() == Some(&dialog.origin))
            .flat_map(|p| p.contributions.iter())
            .find_map(|c| match c {
                Contribution::Panel { id, title, .. } if id == &dialog.panel => Some(title.clone()),
                _ => None,
            });
        let Some(title) = title else {
            self.plugins.dismiss_dialog();
            return;
        };
        let busy = self.plugins.busy() || self.workflow_modal_open_without_plugin_dialogs();
        let Some(panel) = self
            .plugins
            .panels
            .get_mut(&(dialog.origin.plugin.clone(), dialog.panel.clone()))
        else {
            return;
        };
        let images = self
            .plugins
            .runtime
            .as_ref()
            .map(|r| {
                self.plugins
                    .images
                    .widgets(ctx, r, &dialog.origin, &panel.widgets)
            })
            .unwrap_or_default();
        let mut open = true;
        let mut events = vec![];
        crate::i18n::window(&title)
            .id(egui::Id::new((
                "plugin-dialog",
                &dialog.origin.plugin,
                &dialog.panel,
            )))
            .open(&mut open)
            .collapsible(false)
            .default_width(420.)
            .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
            .resize(|r| {
                r.max_size([
                    (ctx.screen_rect().width() - 32.).max(180.),
                    (ctx.screen_rect().height() - 90.).max(100.),
                ])
            })
            .show(ctx, |ui| {
                ui.set_enabled(!busy);
                egui::ScrollArea::vertical()
                    .id_source("dialog-body")
                    .max_height((ctx.screen_rect().height() - 130.).max(80.))
                    .touch_show(ui, |ui| {
                        widgets::show(ui, &dialog.panel, &mut panel.widgets, &mut events, &images)
                    });
            });
        if !open {
            self.plugins.dismiss_dialog();
        }
        for (event, document) in events {
            self.plugin_ui_event(dialog.origin.plugin.clone(), event, document);
        }
    }
}
