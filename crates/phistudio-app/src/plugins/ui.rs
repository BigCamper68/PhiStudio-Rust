use super::*;
use crate::scroll::TouchScroll as _;
use phistudio_plugins::{Action, Status};
use phistudio_sdk::{Backend, Contribution};

impl Workbench {
    pub(crate) fn plugin_windows(&mut self, ctx: &egui::Context) {
        if !self.show_projects {
            self.plugins.open = false;
        }
        if self.plugins.open {
            self.start_plugins(self.workspace.root.clone());
            let mut open = true;
            crate::i18n::window("Plugins")
                .id(egui::Id::new("plugins-manager"))
                .open(&mut open)
                .default_width(430.)
                .default_pos([42., 52.])
                .resize(|r| {
                    r.max_size([
                        (ctx.screen_rect().width() - 32.).max(180.),
                        (ctx.screen_rect().height() - 100.).max(120.),
                    ])
                })
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .id_source("plugins-manager-scroll")
                        .max_height((ctx.screen_rect().height() - 130.).max(100.))
                        .touch_show(ui, |ui| self.plugins_manager(ui));
                });
            self.plugins.open = open;
        }
        let panels: Vec<_> = self
            .plugins
            .registry
            .plugins
            .iter()
            .filter(|_| !self.show_projects)
            .flat_map(|p| {
                p.contributions.iter().filter_map(move |c| match c {
                    Contribution::Panel {
                        id,
                        title,
                        location: PanelLocation::Window,
                        ..
                    } => Some((p.manifest.id.clone(), id.clone(), title.clone())),
                    _ => None,
                })
            })
            .collect();
        let busy = self.plugins.busy() || self.modal_open();
        let mut events = vec![];
        for (plugin, id, title) in panels {
            let Some(panel) = self.plugins.panels.get_mut(&(plugin.clone(), id.clone())) else {
                continue;
            };
            if !panel.open {
                continue;
            }
            let images = self
                .plugins
                .runtime
                .as_ref()
                .map(|runtime| {
                    self.plugins
                        .images
                        .widgets(ctx, runtime, &panel.origin, &panel.widgets)
                })
                .unwrap_or_default();
            let mut open = true;
            crate::i18n::window(&title)
                .id(egui::Id::new(("plugin-panel", &plugin, &id)))
                .open(&mut open)
                .default_width(360.)
                .default_pos([64., 72.])
                .resize(|r| {
                    r.max_size([
                        (ctx.screen_rect().width() - 32.).max(180.),
                        (ctx.screen_rect().height() - 100.).max(120.),
                    ])
                })
                .show(ctx, |ui| {
                    ui.set_enabled(!busy);
                    egui::ScrollArea::vertical()
                        .id_source(("plugin-scroll", &plugin, &id))
                        .max_height((ctx.screen_rect().height() - 130.).max(100.))
                        .touch_show(ui, |ui| {
                            widgets::show(ui, &id, &mut panel.widgets, &mut events, &images)
                        });
                });
            panel.open = open;
            for (event, document) in events.drain(..) {
                self.plugin_ui_event(plugin.clone(), event, document);
            }
        }
        if !self.show_projects {
            self.plugin_track_windows(ctx);
        }
        self.plugin_provider_windows(ctx);
        self.plugin_file_hook_window(ctx);
        self.plugin_editor_action_window(ctx);
    }
    pub(crate) fn plugin_panels(&mut self, ui: &mut egui::Ui, location: PanelLocation) {
        let panels: Vec<_> = self
            .plugins
            .registry
            .plugins
            .iter()
            .flat_map(|p| {
                p.contributions.iter().filter_map(move |c| match c {
                    Contribution::Panel {
                        id,
                        title,
                        location: l,
                        ..
                    } if *l == location => Some((p.manifest.id.clone(), id.clone(), title.clone())),
                    _ => None,
                })
            })
            .collect();
        let busy = self.plugins.busy() || self.modal_open();
        for (plugin, id, title) in panels {
            let Some(panel) = self.plugins.panels.get_mut(&(plugin.clone(), id.clone())) else {
                continue;
            };
            let mut events = vec![];
            let images = self
                .plugins
                .runtime
                .as_ref()
                .map(|runtime| {
                    self.plugins
                        .images
                        .widgets(ui.ctx(), runtime, &panel.origin, &panel.widgets)
                })
                .unwrap_or_default();
            ui.push_id((&plugin, &id), |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    ui.separator();
                    ui.strong(crate::i18n::tr(&title));
                    widgets::show(ui, &id, &mut panel.widgets, &mut events, &images);
                });
            });
            for (event, document) in events {
                self.plugin_ui_event(plugin.clone(), event, document);
            }
        }
    }
    fn plugins_manager(&mut self, ui: &mut egui::Ui) {
        let busy = self.plugins.busy();
        if busy {
            ui.label(crate::i18n::tr("Working…"));
            ui.ctx().request_repaint();
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!busy, egui::Button::new(crate::i18n::tr("Reload plugins")))
                .clicked()
            {
                self.plugins.settings.clear();
                self.plugin_action(Action::Reload);
            }
            #[cfg(any(target_os = "android", target_os = "ios"))]
            if ui.button(crate::i18n::tr("Install plugin")).clicked() {
                self.files.request = Some(crate::files::FileRequest::PluginPackage);
            }
        });
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        ui.horizontal_wrapped(|ui| {
            ui.label(crate::i18n::tr("Plugin package path"));
            ui.add(egui::TextEdit::singleline(&mut self.plugins.import_path).desired_width(220.));
            if ui
                .add_enabled(
                    !busy && !self.plugins.import_path.trim().is_empty(),
                    egui::Button::new(crate::i18n::tr("Install plugin")),
                )
                .clicked()
            {
                let path = PathBuf::from(self.plugins.import_path.trim());
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                self.install_plugin(MediaFile::new(path, name, false));
            }
        });
        let diagnostics = self.plugins.registry.diagnostics.clone();
        for error in diagnostics {
            ui.colored_label(crate::theme::flick(), error);
        }
        if self.plugins.registry.plugins.is_empty() && !busy {
            ui.label(crate::i18n::tr("No plugins installed"));
        }
        let plugins = self.plugins.registry.plugins.clone();
        for info in plugins {
            let id = &info.manifest.id;
            ui.push_id(id, |ui| {
                ui.separator();
                ui.strong(&info.manifest.name);
                ui.small(format!("{} · {}", id, info.manifest.version));
                ui.small(crate::i18n::tr(match info.manifest.backend {
                    Backend::Builtin => "Built-in plugin",
                    Backend::Wasm { .. } => "Wasm plugin",
                    Backend::Native { .. } => "Native plugin",
                }));
                if !info.manifest.description.is_empty() {
                    ui.label(&info.manifest.description);
                }
                if let Status::Failed(error) = &info.status {
                    ui.colored_label(crate::theme::flick(), error);
                }
                for (key, error) in &self.plugins.effect_errors {
                    if &key.plugin == id
                        && info
                            .origin
                            .as_ref()
                            .is_some_and(|o| o.generation == key.generation)
                    {
                        ui.small(crate::i18n::tr("Preview effect error"));
                        ui.colored_label(
                            crate::theme::flick(),
                            format!("{} / {}: {error}", key.contribution, key.pass),
                        );
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    if info.status == Status::Enabled {
                        if ui.button(crate::i18n::tr("Disable plugin")).clicked() {
                            self.plugin_action(Action::Disable(id.clone()));
                        }
                    } else if ui
                        .add_enabled(!busy, egui::Button::new(crate::i18n::tr("Enable plugin")))
                        .clicked()
                    {
                        self.plugin_action(Action::Enable(id.clone()));
                    }
                    if !matches!(info.manifest.backend, Backend::Builtin)
                        && ui
                            .add_enabled(!busy, egui::Button::new(crate::i18n::tr("Remove plugin")))
                            .clicked()
                    {
                        self.plugin_action(Action::Remove(id.clone()));
                    }
                });
            });
        }
    }
}
