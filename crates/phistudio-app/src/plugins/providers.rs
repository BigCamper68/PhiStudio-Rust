//! Provider dialogs retain one operation from picker/preparation through publication.
use super::*;
use crate::scroll::TouchCombo as _;
use phistudio_plugins::{Artifact, Operation, Provider};
use phistudio_sdk::{FormatOperation, PluginDataExport};

pub(super) struct Dialog {
    origin: Origin,
    provider: Provider,
    title: String,
    plugin_data: PluginDataExport,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    source: String,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    destination: String,
}
#[derive(Default)]
pub(super) struct Providers {
    dialog: Option<Dialog>,
    active: Option<Operation>,
    pub(super) jobs: BTreeMap<u64, Operation>,
}
impl Workbench {
    pub(crate) fn plugin_provider_modal(&self) -> bool {
        self.plugins.providers.dialog.is_some() || self.plugins.providers.active.is_some()
    }
    pub(crate) fn cancel_plugin_provider_ui(&mut self) -> bool {
        let had = self.plugin_provider_modal();
        self.plugins.providers.dialog = None;
        if let Some(operation) = self.plugins.providers.active.take() {
            if self
                .files
                .request
                .as_ref()
                .and_then(crate::files::FileRequest::provider_operation)
                == Some(&operation)
            {
                self.files.request = None;
            }
            operation.cancel();
            // Track cleanup like other worker actions so its reply is drained.
            let _ = self.plugins.submit(Action::CancelProvider(operation), None);
        }
        if had {
            self.status = "Plugin operation canceled".into();
        }
        had
    }
    pub(crate) fn provider_operation_current(&self, operation: &Operation) -> bool {
        self.plugins.providers.active.as_ref() == Some(operation)
            && operation.version() == self.plugin_version()
            && operation.is_current()
    }
    pub(crate) fn plugin_provider_preflight(&mut self) {
        if self.plugins.providers.active.as_ref().is_some_and(|o| {
            o.version() != self.plugin_version()
                || (!o.is_current()
                    && !self
                        .plugins
                        .providers
                        .jobs
                        .values()
                        .any(|pending| pending == o))
        }) {
            self.cancel_plugin_provider_ui();
        }
    }
    pub(super) fn open_plugin_provider(&mut self, origin: &Origin, provider: Provider) {
        if !self.plugin_provider_available(&provider) {
            return;
        }
        self.timeline.cancel_gesture();
        self.transport.playing = false;
        let title = self
            .plugins
            .registry
            .plugins
            .iter()
            .filter(|p| p.origin.as_ref() == Some(origin))
            .flat_map(|p| &p.contributions)
            .find_map(|c| match c {
                Contribution::Format { id, title, .. } if id == provider.id() => {
                    Some(title.clone())
                }
                Contribution::Resource { id, title, .. }
                    if id == provider.id() && !title.is_empty() =>
                {
                    Some(title.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| provider.id().to_owned());
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let extension = self
            .plugins
            .registry
            .plugins
            .iter()
            .filter(|p| p.origin.as_ref() == Some(origin))
            .flat_map(|p| &p.contributions)
            .find_map(|c| match c {
                Contribution::Format { id, extensions, .. } if id == provider.id() => {
                    extensions.first()
                }
                _ => None,
            })
            .cloned()
            .unwrap_or_else(|| "data".into());
        self.plugins.providers.dialog = Some(Dialog {
            origin: origin.clone(),
            provider,
            title,
            plugin_data: PluginDataExport::Preserve,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            source: String::new(),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            destination: format!("export.{extension}"),
        });
    }
    pub(super) fn plugin_provider_available(&self, provider: &Provider) -> bool {
        !self.modal_open_except_projects()
            && !self.plugins.busy()
            && self.files.request.is_none()
            && (!self.show_projects
                || matches!(
                    provider,
                    Provider::Format {
                        operation: FormatOperation::Decode,
                        ..
                    }
                ))
    }
    fn begin_plugin_provider(
        &mut self,
        origin: &Origin,
        provider: Provider,
        plugin_data: PluginDataExport,
    ) -> Result<Operation, String> {
        let version = self.plugin_version();
        let operation = self
            .plugins
            .runtime
            .as_mut()
            .ok_or("Plugin host is not running")?
            .begin_provider_with_plugin_data(origin, provider, version, plugin_data)?;
        self.plugins.providers.active = Some(operation.clone());
        Ok(operation)
    }
    pub(crate) fn start_plugin_provider_source(&mut self, operation: Operation, source: MediaFile) {
        if !self.provider_operation_current(&operation) {
            return;
        }
        let context = self.plugin_metadata_context();
        let name = source.name().to_owned();
        match self.plugins.submit(
            Action::Provider {
                operation: operation.clone(),
                source: Box::new(source),
                name,
                context,
            },
            Some(operation.version()),
        ) {
            Ok(id) => {
                self.plugins.providers.jobs.insert(id, operation);
            }
            Err(error) => {
                self.cancel_plugin_provider_ui();
                self.error = Some(error);
            }
        }
    }
    pub(super) fn provider_failed(&mut self, operation: &Operation, message: String) {
        if self.plugins.providers.active.as_ref() == Some(operation) {
            self.cancel_plugin_provider_ui();
            self.error = Some(message);
        }
    }
    pub(super) fn maintain_plugin_provider(&mut self) {
        let stalled = self
            .plugins
            .providers
            .active
            .as_ref()
            .filter(|o| o.is_running())
            .filter(|o| !self.plugins.providers.jobs.values().any(|p| p == *o))
            .filter(|o| !self.plugins.calls.iter().any(|c| c.operation() == Some(*o)))
            .cloned();
        if let Some(operation) = stalled {
            self.provider_failed(
                &operation,
                "Provider returned without completing or continuing its operation".into(),
            );
        }
    }
    pub(crate) fn receive_plugin_provider_source(
        &mut self,
        operation: Operation,
        source: Result<MediaFile, String>,
    ) {
        if !self.provider_operation_current(&operation) {
            return;
        }
        match source {
            Ok(source) => self.start_plugin_provider_source(operation, source),
            Err(error) => {
                self.cancel_plugin_provider_ui();
                self.error = Some(error);
            }
        }
    }
    pub(super) fn plugin_provider_artifact(&mut self, artifact: Artifact) {
        let operation = artifact.operation().clone();
        if !self.provider_operation_current(&operation) {
            return;
        }
        match operation.provider() {
            Provider::Format {
                operation: FormatOperation::Decode,
                ..
            } => self.files.request = Some(crate::files::FileRequest::ProviderImport(artifact)),
            Provider::Format {
                operation: FormatOperation::Encode,
                ..
            } => {
                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                if let Some(dialog) = &self.plugins.providers.dialog {
                    self.files.request = Some(crate::files::FileRequest::ProviderWrite {
                        artifact,
                        path: PathBuf::from(dialog.destination.trim()),
                    });
                }
                #[cfg(any(target_os = "android", target_os = "ios"))]
                {
                    self.files.request = Some(crate::files::FileRequest::ProviderExport(artifact));
                }
            }
            Provider::Resource { .. } => {
                self.files.request = Some(crate::files::FileRequest::ProviderResource(artifact));
            }
        }
    }
    pub(crate) fn receive_plugin_provider_resource(
        &mut self,
        operation: Operation,
        kind: crate::media::MediaKind,
        result: Result<MediaFile, String>,
    ) {
        if !self.provider_operation_current(&operation) {
            return;
        }
        self.plugins.providers.active = None;
        self.plugins.providers.dialog = None;
        match result {
            Ok(file) => {
                self.workspace.resource_target = None;
                self.accept_media(kind, file);
            }
            Err(error) => self.error = Some(error),
        }
    }
    pub(crate) fn receive_plugin_provider_import(
        &mut self,
        operation: Operation,
        result: Result<phistudio_core::ImportedProject, String>,
    ) {
        if !self.provider_operation_current(&operation) {
            if let Ok(project) = result {
                std::thread::spawn(move || {
                    let _ = std::fs::remove_dir_all(project.workspace);
                });
            }
            return;
        }
        self.plugins.providers.active = None;
        self.plugins.providers.dialog = None;
        match result {
            Ok(project) => self.offer_project(project),
            Err(error) => self.error = Some(error),
        }
    }
    pub(crate) fn receive_plugin_provider_write(
        &mut self,
        operation: Operation,
        result: Result<(), String>,
    ) {
        if !self.provider_operation_current(&operation) {
            return;
        }
        self.plugins.providers.active = None;
        self.plugins.providers.dialog = None;
        match result {
            Ok(()) => self.status = "Plugin file exported".into(),
            Err(error) => self.error = Some(error),
        }
    }
    pub(super) fn plugin_provider_windows(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.plugins.providers.dialog.take() else {
            return;
        };
        let mut open = true;
        let mut cancel = false;
        crate::i18n::window("Plugin provider")
            .id(egui::Id::new("plugin-provider"))
            .open(&mut open)
            .default_width(420.)
            .default_pos([56., 64.])
            .resize(|r| {
                r.max_size([
                    (ctx.screen_rect().width() - 24.).max(180.),
                    (ctx.screen_rect().height() - 72.).max(100.),
                ])
            })
            .show(ctx, |ui| {
                if self.plugins.providers.active.is_some() {
                    ui.label(crate::i18n::tr("Working…"));
                } else {
                    ui.label(&dialog.title);
                    let encode = matches!(
                        dialog.provider,
                        Provider::Format {
                            operation: FormatOperation::Encode,
                            ..
                        }
                    );
                    #[cfg(not(any(target_os = "android", target_os = "ios")))]
                    {
                        ui.label(crate::i18n::tr(if encode {
                            "Destination file path"
                        } else {
                            "Source file path"
                        }));
                        ui.add(
                            egui::TextEdit::singleline(if encode {
                                &mut dialog.destination
                            } else {
                                &mut dialog.source
                            })
                            .id_source("provider-path")
                            .desired_width(ui.available_width()),
                        );
                    }
                    let label = if encode {
                        "Export with plugin"
                    } else {
                        "Import with plugin"
                    };
                    if encode {
                        ui.label(crate::i18n::tr("Plugin data in export"));
                        egui::ComboBox::from_id_source("provider-plugin-data")
                            .selected_text(crate::i18n::tr(plugin_data_label(dialog.plugin_data)))
                            .touch_show_ui(ui, |ui| {
                                for mode in [
                                    PluginDataExport::Preserve,
                                    PluginDataExport::Runtime,
                                    PluginDataExport::Remove,
                                ] {
                                    ui.selectable_value(
                                        &mut dialog.plugin_data,
                                        mode,
                                        crate::i18n::tr(plugin_data_label(mode)),
                                    );
                                }
                            });
                    }
                    #[cfg(not(any(target_os = "android", target_os = "ios")))]
                    let valid_path = if encode {
                        !dialog.destination.trim().is_empty()
                    } else {
                        !dialog.source.trim().is_empty()
                    };
                    #[cfg(any(target_os = "android", target_os = "ios"))]
                    let valid_path = true;
                    if ui
                        .add_enabled(valid_path, egui::Button::new(crate::i18n::tr(label)))
                        .clicked()
                    {
                        match self.begin_plugin_provider(
                            &dialog.origin,
                            dialog.provider.clone(),
                            dialog.plugin_data,
                        ) {
                            Ok(operation) => {
                                if encode {
                                    self.files.request = Some(
                                        crate::files::FileRequest::ProviderSnapshot(operation),
                                    );
                                } else {
                                    #[cfg(not(any(target_os = "android", target_os = "ios")))]
                                    {
                                        let path = PathBuf::from(dialog.source.trim());
                                        let name = path
                                            .file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                            .into_owned();
                                        self.start_plugin_provider_source(
                                            operation,
                                            MediaFile::new(path, name, false),
                                        );
                                    }
                                    #[cfg(any(target_os = "android", target_os = "ios"))]
                                    {
                                        self.files.request = Some(
                                            crate::files::FileRequest::ProviderPick(operation),
                                        );
                                    }
                                }
                            }
                            Err(error) => self.error = Some(error),
                        }
                    }
                }
                if ui.button(crate::i18n::tr("Cancel")).clicked() {
                    cancel = true;
                }
            });
        self.plugins.providers.dialog = Some(dialog);
        if !open || cancel {
            self.cancel_plugin_provider_ui();
        }
    }
}

fn plugin_data_label(policy: PluginDataExport) -> &'static str {
    match policy {
        PluginDataExport::Preserve => "Preserve all plugin data",
        PluginDataExport::Runtime => "Runtime data only",
        PluginDataExport::Remove => "Remove all plugin data",
    }
}

#[cfg(test)]
mod tests;
