//! Explicit native file actions keep their continuations on the application side.
use super::*;
use crate::files::FileRequest;
use phistudio_plugins::{Invocation, Output};
use phistudio_sdk::{FILE_BEFORE_HOOK, FileAction, FileContent, FileIntercept};

pub enum Continuation {
    Load {
        path: PathBuf,
        assets: Option<PathBuf>,
    },
    Save(PathBuf),
    Request(FileRequest),
    Export(phistudio_core::ProjectExport),
}
struct Pending {
    job: u64,
    invocation: Invocation,
    payload: FileIntercept,
    continuation: Continuation,
    expected: Vec<Origin>,
}
#[derive(Default)]
pub(super) struct FileHooks {
    pending: Option<Pending>,
    pub(super) jobs: BTreeSet<u64>,
}

pub struct FileApproval {
    invocation: Invocation,
    required: Vec<Origin>,
}
impl FileApproval {
    pub(crate) fn cancel(&self) {
        self.invocation.cancel();
    }
    pub(crate) fn current(&self, app: &Workbench) -> bool {
        self.invocation.version() == app.plugin_version()
            && app
                .plugins
                .runtime
                .as_ref()
                .is_some_and(|r| r.is_invocation_current(&self.invocation, &self.required))
    }
}

fn basename(path: &std::path::Path) -> Option<String> {
    path.file_name().map(|s| s.to_string_lossy().into_owned())
}

impl Workbench {
    pub(crate) fn plugin_file_hook_modal(&self) -> bool {
        self.plugins.file_hooks.pending.is_some()
            || matches!(self.files.request, Some(FileRequest::AfterHook { .. }))
    }
    pub(crate) fn cancel_plugin_file_hook(&mut self) -> bool {
        let had = self.plugin_file_hook_modal();
        if let Some(pending) = self.plugins.file_hooks.pending.take() {
            pending.invocation.cancel();
            self.abandon_file_continuation(&pending.continuation);
        }
        if matches!(self.files.request, Some(FileRequest::AfterHook { .. }))
            && let Some(FileRequest::AfterHook { approval, .. }) = self.files.request.take()
        {
            approval.cancel();
        }
        if !had {
            return false;
        }
        self.status = "Plugin operation canceled".into();
        true
    }
    pub(crate) fn plugin_file_hook_preflight(&mut self) {
        if self
            .plugins
            .file_hooks
            .pending
            .as_ref()
            .is_some_and(|p| p.payload.version != self.plugin_version())
        {
            self.cancel_plugin_file_hook();
        }
    }
    fn abandon_file_continuation(&mut self, continuation: &Continuation) {
        if let Continuation::Load { assets, .. } = continuation
            && self.files.pending_open.is_none()
            && &self.files.pending_asset_directory == assets
        {
            self.files.pending_asset_directory = None;
        }
    }
    fn defer_file_hook(
        &mut self,
        continuation: Continuation,
        action: FileAction,
        content: FileContent,
        name: Option<String>,
    ) -> Option<Continuation> {
        let expected: Vec<_> = self
            .plugins
            .registry
            .plugins
            .iter()
            .filter_map(|p| {
                p.origin
                    .as_ref()
                    .filter(|origin| {
                        self.plugins
                    .runtime
                    .as_ref()
                    .is_some_and(|r| r.is_current(origin))
                    && p.contributions.iter().any(
                        |c| matches!(c,Contribution::Hook {point,..} if point == FILE_BEFORE_HOOK),
                    )
                    })
                    .cloned()
            })
            .collect();
        if expected.is_empty() {
            return Some(continuation);
        }
        if self.plugin_file_hook_modal() {
            self.error = Some("Another file operation is waiting for a plugin".into());
            return None;
        }
        let payload = FileIntercept {
            action,
            content,
            version: self.plugin_version(),
            name,
        };
        let Some(runtime) = &self.plugins.runtime else {
            return Some(continuation);
        };
        let invocation = runtime.invocation(payload.version);
        self.cancel_plugin_input();
        self.timeline.cancel_gesture();
        self.transport.playing = false;
        let result = self.plugins.submit(
            Action::ScopedHook {
                invocation: invocation.clone(),
                point: FILE_BEFORE_HOOK.into(),
                payload: json!(payload),
                context: self.plugin_metadata_context(),
            },
            Some(payload.version),
        );
        match result {
            Ok(job) => {
                self.plugins.file_hooks.jobs.insert(job);
                self.plugins.file_hooks.pending = Some(Pending {
                    job,
                    invocation,
                    payload,
                    continuation,
                    expected,
                });
            }
            Err(error) => {
                invocation.cancel();
                self.error = Some(error);
            }
        }
        None
    }
    pub(crate) fn plugin_before_load(&mut self, path: PathBuf) -> Option<PathBuf> {
        let name = basename(&path);
        let continuation = Continuation::Load {
            path,
            assets: self.files.pending_asset_directory.clone(),
        };
        self.defer_file_hook(continuation, FileAction::Open, FileContent::Chart, name)
            .and_then(|c| {
                if let Continuation::Load { path, .. } = c {
                    Some(path)
                } else {
                    None
                }
            })
    }
    pub(crate) fn plugin_before_save(&mut self, path: PathBuf) -> Option<PathBuf> {
        let name = basename(&path);
        self.defer_file_hook(
            Continuation::Save(path),
            FileAction::Save,
            FileContent::Chart,
            name,
        )
        .and_then(|c| {
            if let Continuation::Save(path) = c {
                Some(path)
            } else {
                None
            }
        })
    }
    pub(crate) fn plugin_before_file_request(
        &mut self,
        request: FileRequest,
    ) -> Option<FileRequest> {
        let descriptor = match &request {
            FileRequest::SaveWorkspace | FileRequest::CreateProject { .. } => {
                Some((FileAction::Save, FileContent::Project, None))
            }
            FileRequest::OpenProject(path) => {
                Some((FileAction::Open, FileContent::Project, basename(path)))
            }
            FileRequest::ImportSource(file) => Some((
                FileAction::Open,
                FileContent::Project,
                Some(file.name().into()),
            )),
            FileRequest::DiscardAndOpen(path) => {
                Some((FileAction::Open, FileContent::Chart, basename(path)))
            }
            _ => None,
        };
        let Some((action, content, name)) = descriptor else {
            return Some(request);
        };
        self.defer_file_hook(Continuation::Request(request), action, content, name)
            .and_then(|c| {
                if let Continuation::Request(request) = c {
                    Some(request)
                } else {
                    None
                }
            })
    }
    pub(crate) fn plugin_before_export(
        &mut self,
        package: phistudio_core::ProjectExport,
        content: FileContent,
    ) -> Option<phistudio_core::ProjectExport> {
        let name = Some(package.name.clone());
        self.defer_file_hook(
            Continuation::Export(package),
            FileAction::Export,
            content,
            name,
        )
        .and_then(|c| {
            if let Continuation::Export(package) = c {
                Some(package)
            } else {
                None
            }
        })
    }
    pub(super) fn complete_plugin_file_hook(&mut self, job: u64, result: Result<Output, String>) {
        if self
            .plugins
            .file_hooks
            .pending
            .as_ref()
            .is_none_or(|p| p.job != job)
        {
            return;
        }
        let pending = self.plugins.file_hooks.pending.take().unwrap();
        if pending.payload.version != self.plugin_version() || !pending.invocation.is_active() {
            pending.invocation.cancel();
            self.abandon_file_continuation(&pending.continuation);
            return;
        }
        let output = match result {
            Ok(output) => output,
            Err(error) => {
                pending.invocation.cancel();
                self.abandon_file_continuation(&pending.continuation);
                let revoked = pending.expected.iter().any(|origin| {
                    self.plugins
                        .runtime
                        .as_ref()
                        .is_none_or(|r| !r.is_current(origin))
                });
                let failed = self.plugins.registry.plugins.iter().any(|p| {
                    pending.expected.iter().any(|o| o.plugin == p.manifest.id)
                        && matches!(p.status, phistudio_plugins::Status::Failed(_))
                });
                if !revoked || failed {
                    self.error = Some(error);
                } else {
                    self.status = "Plugin operation canceled".into();
                }
                return;
            }
        };
        if self
            .plugins
            .runtime
            .as_ref()
            .is_none_or(|r| !r.is_output_current(&output))
        {
            pending.invocation.cancel();
            self.abandon_file_continuation(&pending.continuation);
            self.status = "Plugin result discarded because the document changed".into();
            return;
        }
        if let Some(reason) = output.cancelled {
            for effect in output.effects {
                self.apply_plugin_effect(effect);
            }
            pending.invocation.cancel();
            self.abandon_file_continuation(&pending.continuation);
            self.status = if reason.is_empty() {
                "Plugin operation canceled".into()
            } else {
                reason
            };
            return;
        }
        let payload = serde_json::from_value::<FileIntercept>(output.result)
            .map_err(|e| e.to_string())
            .and_then(|p| {
                p.validate_replacement(&pending.payload)?;
                Ok(p)
            });
        let payload = match payload {
            Ok(payload) => payload,
            Err(error) => {
                pending.invocation.cancel();
                self.abandon_file_continuation(&pending.continuation);
                self.error = Some(error);
                return;
            }
        };
        self.plugins.calls.extend(output.calls);
        for effect in output.effects {
            self.apply_plugin_effect(effect);
        }
        let approval = FileApproval {
            invocation: pending.invocation,
            required: output.required,
        };
        if !approval.current(self) {
            approval.invocation.cancel();
            return;
        }
        match pending.continuation {
            Continuation::Save(path) => self.save_now(path),
            Continuation::Load { path, assets } => {
                self.files.pending_asset_directory = assets;
                self.load_now(path);
            }
            Continuation::Request(request) => {
                self.files.request = Some(FileRequest::AfterHook {
                    request: Box::new(request),
                    approval,
                })
            }
            Continuation::Export(mut package) => {
                if let Some(name) = payload.name {
                    package.name = name;
                }
                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                {
                    self.files.export_path = package.name.clone();
                    self.files.export_package = Some(package);
                }
                #[cfg(any(target_os = "android", target_os = "ios"))]
                {
                    self.files.request = Some(FileRequest::AfterHook {
                        request: Box::new(FileRequest::ReadyExport(package)),
                        approval,
                    });
                }
            }
        }
    }
    pub(super) fn plugin_file_hook_window(&mut self, ctx: &egui::Context) {
        if !self.plugin_file_hook_modal() {
            return;
        }
        let mut open = true;
        let mut cancel = false;
        crate::i18n::window("Plugins")
            .id(egui::Id::new("plugin-file-hook"))
            .open(&mut open)
            .default_width(280.)
            .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
            .show(ctx, |ui| {
                ui.label(crate::i18n::tr("Working…"));
                cancel = ui.button(crate::i18n::tr("Cancel")).clicked();
            });
        if !open || cancel {
            self.cancel_plugin_file_hook();
        }
    }
}

#[cfg(test)]
mod tests;
