//! Context entries are attached to native note/selection/line properties.
use super::*;
use phistudio_sdk::{ContextArguments, ContextNote, ContextTarget};

#[derive(Clone)]
struct Snapshot {
    arguments: ContextArguments,
    entries: Vec<(Origin, String, String)>,
}

impl Workbench {
    pub(crate) fn plugin_context_menu(&mut self, ui: &mut egui::Ui, target: ContextTarget) {
        let entries: Vec<_> = self
            .plugins
            .registry
            .plugins
            .iter()
            .flat_map(|p| {
                p.contributions.iter().filter_map(move |c| match c {
                    Contribution::ContextMenu {
                        title,
                        target: wanted,
                        command,
                        ..
                    } if *wanted == target => {
                        Some((p.origin.clone()?, title.clone(), command.clone()))
                    }
                    _ => None,
                })
            })
            .collect();
        if entries.is_empty() {
            return;
        }
        let version = self.plugin_version();
        let line = self.line;
        let notes = self
            .selection()
            .notes
            .iter()
            .take(4096)
            .map(|id| ContextNote {
                line: id.line,
                index: id.index,
            })
            .collect::<Vec<_>>();
        let key = ui.id().with(("plugin-context", format!("{target:?}")));
        let snapshot = ui.data_mut(|data| {
            data.get_temp::<Snapshot>(key).unwrap_or_else(|| {
                let snapshot = Snapshot {
                    arguments: ContextArguments {
                        version,
                        target,
                        line,
                        notes,
                    },
                    entries,
                };
                data.insert_temp(key, snapshot.clone());
                snapshot
            })
        });
        let response = ui.menu_button(crate::i18n::tr("Plugin actions…"), |ui| {
            for (origin, title, command) in &snapshot.entries {
                let current = self
                    .plugins
                    .runtime
                    .as_ref()
                    .is_some_and(|r| r.is_current(origin));
                if ui
                    .add_enabled(
                        current
                            && snapshot.arguments.version == self.plugin_version()
                            && !self.plugins.busy(),
                        egui::Button::new(crate::i18n::tr(title)),
                    )
                    .clicked()
                {
                    if snapshot.arguments.version == self.plugin_version() {
                        self.plugin_command(&origin.plugin, command, json!(snapshot.arguments));
                    }
                    ui.close_menu();
                }
            }
        });
        if response.inner.is_none() {
            ui.data_mut(|data| data.remove::<Snapshot>(key));
        }
    }
}
