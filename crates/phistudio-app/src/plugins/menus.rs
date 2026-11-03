//! Editor entry points for enabled plugins; the loader owns only lifecycle UI.
use super::*;
use crate::scroll::TouchCombo as _;
use phistudio_sdk::{EditorMenu, Setting};
#[cfg(test)]
mod tests;

impl Workbench {
    pub(crate) fn open_plugin_panel(&mut self, origin: &Origin, panel: &str) {
        if self
            .plugins
            .runtime
            .as_ref()
            .is_none_or(|r| !r.is_current(origin))
        {
            return;
        }
        if let Some(view) = self
            .plugins
            .panels
            .get_mut(&(origin.plugin.clone(), panel.into()))
            && view.origin == *origin
        {
            view.open = true;
        }
    }
    pub(crate) fn plugin_editor_menu(&mut self, ui: &mut egui::Ui, menu: EditorMenu) {
        let plugins = self.plugins.registry.plugins.clone();
        let modal = self.modal_open();
        let mut separated = false;
        for info in plugins {
            let Some(origin) = &info.origin else { continue };
            if self
                .plugins
                .runtime
                .as_ref()
                .is_none_or(|r| !r.is_current(origin))
            {
                continue;
            }
            for contribution in &info.contributions {
                let entry = match contribution {
                    Contribution::Menu {
                        title,
                        location,
                        panel,
                        ..
                    } if *location == menu => Some((title, panel)),
                    Contribution::Panel {
                        id,
                        title,
                        location: PanelLocation::Window,
                        ..
                    } if menu == EditorMenu::Windows
                        && !info
                            .contributions
                            .iter()
                            .any(|c| matches!(c,Contribution::Menu{panel,..} if panel==id)) =>
                    {
                        Some((title, id))
                    }
                    _ => None,
                };
                if let Some((title, panel)) = entry {
                    if !separated {
                        ui.separator();
                        separated = true
                    }
                    let ready = self
                        .plugins
                        .panels
                        .get(&(origin.plugin.clone(), panel.clone()))
                        .is_some_and(|p| p.origin == *origin);
                    if ui
                        .add_enabled(ready && !modal, egui::Button::new(crate::i18n::tr(title)))
                        .clicked()
                    {
                        self.open_plugin_panel(origin, panel);
                        ui.close_menu();
                    }
                }
                if let Contribution::Track { id, title } = contribution
                    && menu == EditorMenu::Windows
                {
                    if !separated {
                        ui.separator();
                        separated = true
                    }
                    if ui
                        .add_enabled(!modal, egui::Button::new(crate::i18n::tr(title)))
                        .clicked()
                    {
                        self.plugins
                            .rendering
                            .tracks
                            .entry((origin.plugin.clone(), id.clone()))
                            .and_modify(|v| v.origin = origin.clone())
                            .or_insert_with(|| rendering::TrackWindow {
                                origin: origin.clone(),
                                open: false,
                            })
                            .open = true;
                        ui.close_menu();
                    }
                }
            }
            let legacy = !info
                .contributions
                .iter()
                .any(|c| matches!(c, Contribution::Menu { .. }));
            if menu == EditorMenu::File
                && legacy
                && (!info.manifest.settings.is_empty()
                    || info.contributions.iter().any(|c| {
                        matches!(
                            c,
                            Contribution::Command { .. }
                                | Contribution::Tool { .. }
                                | Contribution::Format { .. }
                                | Contribution::Resource { .. }
                        )
                    }))
            {
                if !separated {
                    ui.separator();
                    separated = true
                }
                ui.push_id(&info.manifest.id, |ui| {
                    let title = format!("{} · {}", info.manifest.name, crate::i18n::tr("More…"));
                    let response = egui::CollapsingHeader::new(title)
                        .id_source("plugin-actions")
                        .show(ui, |ui| self.plugin_actions(ui, &info));
                    // Keep the actions visible as the section expands at the
                    // bottom of a short File menu.
                    let reveal = response.header_response.clicked()
                        || (!response.fully_closed() && !response.fully_open());
                    if reveal
                        && egui::collapsing_header::CollapsingState::load(
                            ui.ctx(),
                            response.header_response.id,
                        )
                        .is_some_and(|state| state.is_open())
                        && let Some(body) = response.body_response
                    {
                        ui.scroll_to_rect(response.header_response.rect.union(body.rect), None);
                    }
                });
            }
        }
    }
    fn plugin_actions(&mut self, ui: &mut egui::Ui, info: &phistudio_plugins::PluginInfo) {
        let id = &info.manifest.id;
        let busy = self.plugins.busy();
        for contribution in &info.contributions {
            match contribution {
                Contribution::Resource {
                    id: resource,
                    title,
                    resource: kind,
                    ..
                } => {
                    if !title.is_empty() {
                        ui.label(title);
                    }
                    let provider = phistudio_plugins::Provider::Resource {
                        id: resource.clone(),
                        resource: *kind,
                    };
                    if ui
                        .add_enabled(
                            self.plugin_provider_available(&provider)
                                && self.asset_root().is_some(),
                            egui::Button::new(crate::i18n::tr("Convert resource")),
                        )
                        .clicked()
                        && let Some(origin) = &info.origin
                    {
                        self.open_plugin_provider(origin, provider);
                        ui.close_menu();
                    }
                }
                Contribution::Format {
                    id: format,
                    title,
                    decode,
                    encode,
                    ..
                } => {
                    ui.label(title);
                    ui.horizontal_wrapped(|ui| {
                        for (enabled, label, operation) in [
                            (
                                *decode,
                                "Import with plugin",
                                phistudio_sdk::FormatOperation::Decode,
                            ),
                            (
                                *encode,
                                "Export with plugin",
                                phistudio_sdk::FormatOperation::Encode,
                            ),
                        ] {
                            let provider = phistudio_plugins::Provider::Format {
                                id: format.clone(),
                                operation,
                            };
                            if enabled
                                && ui
                                    .add_enabled(
                                        self.plugin_provider_available(&provider),
                                        egui::Button::new(crate::i18n::tr(label)),
                                    )
                                    .clicked()
                                && let Some(origin) = &info.origin
                            {
                                self.open_plugin_provider(origin, provider);
                                ui.close_menu();
                            }
                        }
                    });
                }
                Contribution::Tool {
                    id: tool, title, ..
                } => {
                    if ui
                        .add_enabled(
                            !self.modal_open(),
                            egui::Button::new(crate::i18n::tr(title)),
                        )
                        .clicked()
                        && let Some(origin) = &info.origin
                    {
                        self.select_plugin_tool(origin, Some(tool));
                        ui.close_menu();
                    }
                }
                Contribution::Command {
                    id: command,
                    title,
                    shortcut,
                    ..
                } => {
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(
                                !busy && !self.modal_open(),
                                egui::Button::new(crate::i18n::tr(title)),
                            )
                            .clicked()
                        {
                            self.plugin_command(id, command, Value::Null);
                            ui.close_menu();
                        }
                        if let Some(shortcut) = shortcut {
                            ui.small(shortcut);
                        }
                    });
                }
                _ => {}
            }
        }
        for (key, setting) in &info.manifest.settings {
            let value = self
                .plugins
                .settings
                .entry((id.clone(), key.clone()))
                .or_insert_with(|| {
                    info.settings
                        .get(key)
                        .cloned()
                        .unwrap_or_else(|| setting.default_value())
                });
            let changed = setting_ui(ui, key, setting, value, !busy);
            if changed {
                let value = value.clone();
                self.plugin_action(Action::Setting {
                    plugin: id.clone(),
                    key: key.clone(),
                    value,
                });
            }
        }
    }
}

fn setting_ui(
    ui: &mut egui::Ui,
    key: &str,
    setting: &Setting,
    value: &mut Value,
    enabled: bool,
) -> bool {
    let mut changed = false;
    ui.add_enabled_ui(enabled, |ui| match setting {
        Setting::Bool { title, .. } => {
            let mut v = value.as_bool().unwrap_or(false);
            if ui.checkbox(&mut v, crate::i18n::tr(title)).changed() {
                *value = json!(v);
                changed = true;
            }
        }
        Setting::Number {
            title, min, max, ..
        } => {
            let mut v = value.as_f64().unwrap_or(*min);
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr(title));
                if ui
                    .add(
                        crate::numeric::Value::new(&mut v)
                            .clamp_range(*min..=*max)
                            .speed(0.1)
                            .max_decimals(6),
                    )
                    .changed()
                {
                    *value = json!(v);
                    changed = true;
                }
            });
        }
        Setting::Text { title, .. } => {
            let mut v = value.as_str().unwrap_or("").to_owned();
            ui.label(crate::i18n::tr(title));
            if ui
                .add(egui::TextEdit::singleline(&mut v).id_source(("setting", key)))
                .changed()
            {
                *value = json!(v);
                changed = true;
            }
        }
        Setting::Choice { title, options, .. } => {
            let mut v = value.as_str().unwrap_or("").to_owned();
            let old = v.clone();
            ui.label(crate::i18n::tr(title));
            egui::ComboBox::from_id_source(("setting", key))
                .selected_text(options.get(&v).map_or(v.as_str(), String::as_str))
                .touch_show_ui(ui, |ui| {
                    for (key, title) in options {
                        ui.selectable_value(&mut v, key.clone(), crate::i18n::tr(title));
                    }
                });
            if old != v {
                *value = json!(v);
                changed = true;
            }
        }
    });
    changed
}
