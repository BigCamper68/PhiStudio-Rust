//! Application adapter for the worker-owned extension host.
mod drawing;
mod editor_actions;
mod file_hooks;
pub use file_hooks::FileApproval;
mod images;
mod input;
mod inspection;
mod menus;
mod preview_effects;
mod providers;
mod rendering;
#[cfg(test)]
mod tests;
mod tracks;
mod ui;
mod widgets;
use crate::{media::MediaFile, workbench::Workbench};
use phistudio_plugins::{Action, HostCall, Origin, OwnedEffect, Registry, Runtime};
use phistudio_sdk::{
    Capability, Contribution, DocumentVersion, EditorContext, Effect, PanelLocation, UiEvent,
    Widget,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
};

pub(crate) struct Panel {
    pub origin: Origin,
    pub widgets: Vec<Widget>,
    pub open: bool,
}
#[derive(Default)]
pub(crate) struct State {
    pub open: bool,
    pub runtime: Option<Runtime>,
    pub registry: Registry,
    pub panels: BTreeMap<(String, String), Panel>,
    pub pending: BTreeMap<u64, Option<DocumentVersion>>,
    background: BTreeSet<u64>,
    rendering: rendering::Rendering,
    images: images::Images,
    effect_errors: BTreeMap<phistudio_prpr::PreviewFilterKey, String>,
    input: input::Input,
    providers: providers::Providers,
    file_hooks: file_hooks::FileHooks,
    editor_actions: editor_actions::EditorActions,
    calls: VecDeque<HostCall>,
    inspections: VecDeque<HostCall>,
    pub settings: BTreeMap<(String, String), Value>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub import_path: String,
    observed: Option<DocumentVersion>,
    observed_assets: Option<(phistudio_sdk::ProjectAssetsVersion, Option<PathBuf>)>,
    announced: BTreeMap<String, u64>,
    inspection_context: Option<(Value, Value)>,
}
impl State {
    pub fn start(&mut self, root: Option<PathBuf>) -> Result<(), String> {
        if self.runtime.is_none() {
            self.runtime = Some(Runtime::spawn(root, vec![])?);
            self.pending.insert(0, None);
        }
        Ok(())
    }
    pub fn busy(&self) -> bool {
        self.pending.keys().any(|id| !self.background.contains(id))
            || self.calls.iter().any(|c| !c.is_background())
    }
    pub fn working(&self) -> bool {
        !self.pending.is_empty() || !self.calls.is_empty()
    }
    pub fn submit(
        &mut self,
        action: Action,
        version: Option<DocumentVersion>,
    ) -> Result<u64, String> {
        let id = self
            .runtime
            .as_mut()
            .ok_or("Plugin host is not running")?
            .submit(action)?;
        self.pending.insert(id, version);
        Ok(id)
    }
    pub fn close_top(&mut self, editor: bool) -> bool {
        if std::mem::take(&mut self.open) {
            return true;
        }
        if !editor {
            return false;
        }
        for info in &self.registry.plugins {
            for contribution in &info.contributions {
                if let phistudio_sdk::Contribution::Panel {
                    id,
                    location: PanelLocation::Window,
                    ..
                } = contribution
                    && let Some(panel) =
                        self.panels.get_mut(&(info.manifest.id.clone(), id.clone()))
                    && std::mem::take(&mut panel.open)
                {
                    return true;
                }
            }
        }
        for track in self.rendering.tracks.values_mut() {
            if std::mem::take(&mut track.open) {
                return true;
            }
        }
        false
    }
    pub fn shutdown(&mut self) -> Result<(), String> {
        if let Some(runtime) = &mut self.runtime {
            runtime.shutdown()?;
        }
        Ok(())
    }
}
impl phistudio_plugins::PackageSource for MediaFile {
    fn path(&self) -> &std::path::Path {
        self.path()
    }
}

impl Workbench {
    pub(crate) fn sync_plugin_project(&self) {
        if let Some(runtime) = &self.plugins.runtime {
            runtime.set_project(
                self.plugin_version(),
                self.asset_revision,
                self.asset_root(),
            );
        }
    }
    fn plugin_metadata_context(&self) -> EditorContext {
        self.sync_plugin_project();
        EditorContext {
            version: self.plugin_version(),
            line: self.line,
            beat: self.transport.beat,
            playing: self.transport.playing,
            ..Default::default()
        }
    }
    pub(crate) fn plugin_version(&self) -> DocumentVersion {
        DocumentVersion {
            session: self.session,
            revision: self.editor.revision(),
        }
    }
    fn plugin_context(&self, document: bool) -> EditorContext {
        self.sync_plugin_project();
        let selection = self.selection();
        EditorContext {
            version: self.plugin_version(),
            line: self.line,
            beat: self.transport.beat,
            playing: self.transport.playing,
            chart: document.then(|| self.editor.document().as_json().clone()),
            selection: json!({"notes":selection.notes.iter().map(|n|json!({"line":n.line,"index":n.index})).collect::<Vec<_>>(),
                "events":selection.events.iter().map(|e|json!({"line":e.line,"layer":e.layer,"kind":e.kind,"index":e.index})).collect::<Vec<_>>() }),
        }
    }
    pub(crate) fn start_plugins(&mut self, root: Option<PathBuf>) {
        if let Err(error) = self.plugins.start(root) {
            self.error = Some(error);
        }
    }
    pub(crate) fn plugin_action(&mut self, action: Action) {
        self.start_plugins(self.workspace.root.clone());
        if let Err(error) = self.plugins.submit(action, None) {
            self.error = Some(error);
        }
    }
    pub(crate) fn install_plugin(&mut self, file: MediaFile) {
        self.plugin_action(Action::Install(Box::new(file)));
    }
    pub(crate) fn plugin_command(&mut self, plugin: &str, id: &str, arguments: Value) {
        let document = self.plugins.registry.plugins.iter().any(|p| {
            p.manifest.id == plugin
                && p.granted.contains(&Capability::DocumentRead)
                && p.contributions.iter().any(|c| {
                    matches!(c, Contribution::Command {
                    id: registered, snapshot: true, .. } if registered == id)
                })
        });
        let context = self.plugin_context(document);
        let version = context.version;
        let action = Action::Command {
            plugin: plugin.into(),
            id: id.into(),
            arguments,
            context,
        };
        if let Err(error) = self.plugins.submit(action, Some(version)) {
            self.error = Some(error);
        }
    }
    fn plugin_ui_event(&mut self, plugin: String, event: UiEvent, document: bool) {
        let document = document
            && self.plugins.registry.plugins.iter().any(|p| {
                p.manifest.id == plugin
                    && p.granted.contains(&Capability::DocumentRead)
                    && p.contributions.iter().any(|c| {
                        matches!(c, Contribution::Panel {
                            id, snapshot: true, .. } if id == &event.panel)
                    })
            });
        let context = self.plugin_context(document);
        let version = context.version;
        if let Err(error) = self.plugins.submit(
            Action::Ui {
                plugin,
                event,
                context,
            },
            Some(version),
        ) {
            self.error = Some(error);
        }
    }
    pub(crate) fn poll_plugins(&mut self) {
        self.sync_plugin_project();
        let mut completed = vec![];
        if let Some(runtime) = &self.plugins.runtime {
            while !self.plugins.pending.is_empty() {
                match runtime.poll() {
                    Ok(Some(completion)) => completed.push(completion),
                    Ok(None) => break,
                    Err(error) => {
                        self.error = Some(error);
                        self.plugins.pending.clear();
                        self.plugins.background.clear();
                        self.plugins.calls.clear();
                        self.plugins.registry = runtime.registry().unwrap_or_default();
                        break;
                    }
                }
            }
            if !completed.is_empty() {
                match runtime.registry() {
                    Ok(registry) => self.plugins.registry = registry,
                    Err(error) => self.error = Some(error),
                }
            }
            self.plugins
                .panels
                .retain(|_, panel| runtime.is_current(&panel.origin));
            self.plugins.rendering.retain(runtime);
            self.plugins.images.retain(runtime);
            self.plugins.effect_errors.retain(|key, _| {
                runtime.is_current(&Origin {
                    plugin: key.plugin.clone(),
                    generation: key.generation,
                })
            });
        }
        for completion in completed {
            if self.plugins.editor_actions.jobs.remove(&completion.id) {
                self.plugins.pending.remove(&completion.id);
                self.complete_plugin_editor_action(completion.id, completion.result);
                continue;
            }
            if self.plugins.file_hooks.jobs.remove(&completion.id) {
                self.plugins.pending.remove(&completion.id);
                self.complete_plugin_file_hook(completion.id, completion.result);
                continue;
            }
            let interaction = self.plugins.input.complete(completion.id);
            let provider = self.plugins.providers.jobs.remove(&completion.id);
            self.plugins.background.remove(&completion.id);
            let render = self.plugins.rendering.complete(completion.id);
            let version = self.plugins.pending.remove(&completion.id).flatten();
            if interaction.as_ref().is_some_and(|i| !i.is_active()) {
                continue;
            }
            if render
                .as_ref()
                .is_some_and(|t| !t.current(self.plugins.runtime.as_ref()))
            {
                continue;
            }
            match completion.result {
                Err(error) => {
                    if let Some(operation) = provider {
                        if operation.version() == self.plugin_version() {
                            self.provider_failed(&operation, error);
                        }
                    } else {
                        self.error = Some(error);
                    }
                }
                Ok(output) => {
                    if self
                        .plugins
                        .runtime
                        .as_ref()
                        .is_none_or(|r| !r.is_output_current(&output))
                    {
                        continue;
                    }
                    self.plugins.calls.extend(output.calls);
                    for effect in output.effects {
                        if matches!(
                            effect.effect,
                            Effect::Draw { .. } | Effect::PreviewEffects { .. }
                        ) {
                            let current = self
                                .plugins
                                .runtime
                                .as_ref()
                                .is_some_and(|r| r.is_effect_current(&effect));
                            if let Some(ticket) = &render
                                && current
                            {
                                self.plugins.rendering.apply(ticket, effect);
                            }
                            continue;
                        }
                        // Project-bound edits are checked again by the core. Other
                        // plugin UI/settings state can survive project replacement.
                        if matches!(effect.effect, Effect::Edit { .. })
                            && version.is_some_and(|v| v != self.plugin_version())
                        {
                            self.status =
                                "Plugin result discarded because the document changed".into();
                            continue;
                        }
                        self.apply_plugin_effect(effect);
                    }
                    if let Some(artifact) = output.artifact {
                        self.plugin_provider_artifact(artifact);
                    } else if let (Some(operation), Some(message)) =
                        (output.operation, output.cancelled)
                    {
                        self.provider_failed(&operation, message);
                    }
                }
            }
        }
        // Bound document work per frame and leave room in the worker's queue
        // for disable/reload even when a plugin requests many pages at once.
        for _ in 0..2 {
            if self.plugins.pending.len() >= 8 {
                break;
            }
            let Some(call) = self.plugins.calls.pop_front() else {
                break;
            };
            let Some(runtime) = &self.plugins.runtime else {
                break;
            };
            if !runtime.is_call_current(&call) {
                continue;
            }
            if call.request.service.provider == "host" && call.request.service.id == "editor" {
                self.plugin_editor_service(call);
                continue;
            }
            if call.request.service.provider == "host" && call.request.service.id == "project" {
                self.plugin_project_service(call);
                continue;
            }
            if call.request.service.provider == "host" && call.request.service.id == "preview" {
                if call.request.method == "inspect" && self.plugins.inspections.len() < 8 {
                    self.plugins.inspections.push_back(call);
                } else {
                    self.reply_plugin_service(
                        call,
                        phistudio_sdk::ServiceOutcome::error(
                            "inspection_unavailable",
                            "Unknown method or preview inspection queue is full",
                        ),
                    );
                }
                continue;
            }
            let outcome =
                runtime.document_service(&call, self.plugin_version(), self.editor.document());
            self.reply_plugin_service(call, outcome);
        }
        let version = self.plugin_version();
        let asset_version = phistudio_sdk::ProjectAssetsVersion {
            session: self.session,
            revision: self.asset_revision,
        };
        let assets = (
            asset_version,
            self.asset_root().map(std::path::Path::to_path_buf),
        );
        if self.plugins.observed_assets.as_ref() != Some(&assets) {
            self.plugins.observed_assets = Some(assets);
            let topic = "host.project.assets.changed";
            if self.plugins.registry.plugins.iter().any(|p| {
                p.contributions.iter().any(|c| {
                matches!(c, Contribution::Event { topic: registered } if registered == topic)
            })
            }) {
                let context = self.plugin_metadata_context();
                let _ = self.plugins.submit(
                    Action::Event {
                        topic: topic.into(),
                        payload: json!({"assets": asset_version}),
                        context,
                    },
                    Some(version),
                );
            }
        }
        let changed = self.plugins.observed != Some(version);
        let opened = self
            .plugins
            .observed
            .is_none_or(|v| v.session != version.session);
        if changed {
            let topic = if self
                .plugins
                .observed
                .is_none_or(|v| v.session != version.session)
            {
                "host.document.opened"
            } else {
                "host.document.changed"
            };
            self.plugins.observed = Some(version);
            if self.plugins.registry.plugins.iter().any(|p| {
                p.contributions
                    .iter()
                    .any(|c| matches!(c,phistudio_sdk::Contribution::Event {topic:t} if t == topic))
            }) {
                let context = self.plugin_context(false);
                let _ = self.plugins.submit(
                    Action::Event {
                        topic: topic.into(),
                        payload: json!({"version":version}),
                        context,
                    },
                    Some(version),
                );
            }
        }
        let new:Vec<_> = self.plugins.registry.plugins.iter().filter_map(|p| {
            let origin = p.origin.as_ref()?;
            if self.plugins.announced.get(&origin.plugin) == Some(&origin.generation) { return None; }
            let listens = p.contributions.iter().any(|c|matches!(c,phistudio_sdk::Contribution::Event {topic} if topic == "host.document.opened"));
            Some((origin.clone(),listens))
        }).collect();
        for (origin, listens) in new {
            self.plugins
                .announced
                .insert(origin.plugin.clone(), origin.generation);
            if listens && !(changed && opened) {
                let context = self.plugin_context(false);
                let _ = self.plugins.submit(
                    Action::Notify {
                        plugin: origin.plugin,
                        topic: "host.document.opened".into(),
                        payload: json!({"version":version}),
                        context,
                    },
                    Some(version),
                );
            }
        }
        self.maintain_plugin_input();
        let preview_context = json!([
            self.options.preview_aspect,
            self.options.preview_note_scale,
            self.project_options.force_aspect_ratio,
            self.project_options.aspect_ratio,
            self.preview.rect.aspect_ratio()
        ]);
        let plugin_context = json!(
            self.plugins
                .registry
                .plugins
                .iter()
                .filter_map(|p| p.origin.as_ref().map(|o| (
                    &o.plugin,
                    o.generation,
                    &p.manifest.version
                )))
                .collect::<Vec<_>>()
        );
        let previous = self
            .plugins
            .inspection_context
            .replace((preview_context.clone(), plugin_context.clone()));
        for (topic, changed) in [
            (
                "host.preview.configuration.changed",
                previous.as_ref().is_some_and(|p| p.0 != preview_context),
            ),
            (
                "host.plugins.changed",
                previous.as_ref().is_some_and(|p| p.1 != plugin_context),
            ),
        ] {
            if changed
                && self.plugins.registry.plugins.iter().any(|p| {
                    p.contributions.iter()
                .any(|c| matches!(c, Contribution::Event {topic: registered} if registered==topic))
                })
            {
                let context = self.plugin_metadata_context();
                let _ = self.plugins.submit(
                    Action::Event {
                        topic: topic.into(),
                        payload: Value::Null,
                        context,
                    },
                    Some(version),
                );
            }
        }
        self.maintain_plugin_provider();
        self.maintain_plugin_effect_views();
        self.pump_plugin_renders();
    }
    fn reply_plugin_service(&mut self, call: HostCall, outcome: phistudio_sdk::ServiceOutcome) {
        if self
            .plugins
            .runtime
            .as_ref()
            .is_none_or(|r| !r.is_call_current(&call))
        {
            return;
        }
        let version = call.context.version;
        let background = call.is_background();
        let interaction = call.interaction().cloned();
        let operation = call.operation().cloned();
        match self
            .plugins
            .submit(Action::ServiceReply { call, outcome }, Some(version))
        {
            Ok(id) => {
                if let Some(scope) = operation {
                    self.plugins.providers.jobs.insert(id, scope);
                }
                if let Some(scope) = interaction {
                    self.plugins.input.jobs.insert(id, scope);
                }
                if background {
                    self.plugins.background.insert(id);
                }
            }
            Err(error) => self.error = Some(error),
        }
    }
    fn apply_plugin_effect(&mut self, owned: OwnedEffect) {
        if self
            .plugins
            .runtime
            .as_ref()
            .is_none_or(|r| !r.is_effect_current(&owned))
        {
            return;
        }
        match owned.effect {
            Effect::Notify { message } => self.status = message,
            Effect::Complete { .. } | Effect::Fail { .. } => {
                self.error = Some("Provider completion was not handled by the worker".into())
            }
            Effect::Panel { id, widgets } => {
                let key = (owned.origin.plugin.clone(), id);
                let open = self
                    .plugins
                    .panels
                    .get(&key)
                    .is_some_and(|p| p.origin == owned.origin && p.open);
                self.plugins.panels.insert(
                    key,
                    Panel {
                        origin: owned.origin,
                        widgets,
                        open,
                    },
                );
            }
            Effect::Edit { transaction } => {
                let anchor = self.anchor();
                let label = transaction.label.clone();
                let result = self
                    .plugins
                    .runtime
                    .as_ref()
                    .ok_or("Plugin host is not running".into())
                    .and_then(|r| {
                        r.apply_scoped_transaction(
                            &owned.origin,
                            &owned.required,
                            owned.interaction.as_ref(),
                            self.session,
                            &mut self.editor,
                            transaction,
                        )
                    });
                match result {
                    Ok(true) => {
                        self.line = self.line.min(self.editor.document().line_count() - 1);
                        self.select(None);
                        self.inspector = Default::default();
                        self.line_draft = None;
                        self.bpm_draft = None;
                        self.timeline.cancel_gesture();
                        self.preview.drag = None;
                        self.transport.playing = false;
                        self.unlink_invalid_xy();
                        self.remember_anchor(anchor);
                        self.multi_line
                            .lines
                            .retain(|i| *i < self.editor.document().line_count());
                        self.error = None;
                        self.status = label;
                    }
                    Ok(false) => self.status = "No changes".into(),
                    Err(error) => self.error = Some(error),
                }
            }
            Effect::Draw { .. } | Effect::PreviewEffects { .. } => {} // Correlated with its render ticket above.
            Effect::SelectTool { id } => self.select_plugin_tool(&owned.origin, id.as_deref()),
            Effect::Emit { .. } | Effect::Store { .. } | Effect::Call { .. } => {} // Consumed by the worker.
        }
    }
    pub(crate) fn plugin_key(&mut self, key: egui::Key, modifiers: egui::Modifiers) -> bool {
        if self.plugins.busy() {
            return false;
        }
        let spelling = crate::shortcuts_ui::spelling(key, modifiers);
        let command = self
            .plugins
            .registry
            .plugins
            .iter()
            .flat_map(|p| p.contributions.iter().map(move |c| (&p.manifest.id, c)))
            .find_map(|(plugin, c)| match c {
                phistudio_sdk::Contribution::Command {
                    id,
                    shortcut: Some(shortcut),
                    ..
                } if shortcut.eq_ignore_ascii_case(&spelling) => Some((plugin.clone(), id.clone())),
                _ => None,
            });
        if let Some((plugin, id)) = command {
            self.plugin_command(&plugin, &id, Value::Null);
            return true;
        }
        false
    }
}
