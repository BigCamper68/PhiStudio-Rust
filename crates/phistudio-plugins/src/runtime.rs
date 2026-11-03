//! One serialized worker owns extension code and disk state. The UI sees only
//! metadata and versioned effects; revocation takes effect before a queued stop.
mod assets;
mod dispatch;
mod invocation;
mod providers;
use crate::{
    Host, Origin, OwnedEffect, PluginInfo, Status, WasmLimits, discover, install_package,
    preferences::{self, Preferences},
    retire_package,
};
pub use assets::PreparedImage;
use dispatch::Budget;
pub use dispatch::HostCall;
pub use invocation::Invocation;
use phistudio_core::Editor;
use phistudio_sdk::*;
pub use providers::{Artifact, Operation, Provider};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
};

pub struct Builtin {
    pub manifest: Manifest,
    pub create: Arc<dyn Fn() -> Box<dyn Plugin> + Send + Sync>,
    pub enabled: bool,
}

/// Owns the source snapshot until installation finishes, including on shutdown.
pub trait PackageSource: Send {
    fn path(&self) -> &std::path::Path;
}
impl PackageSource for PathBuf {
    fn path(&self) -> &std::path::Path {
        self.as_path()
    }
}

pub enum Action {
    Provider {
        operation: Operation,
        source: Box<dyn PackageSource>,
        name: String,
        context: EditorContext,
    },
    CancelProvider(Operation),
    Input {
        interaction: crate::Interaction,
        input: InputBatch,
        context: EditorContext,
    },
    Reload,
    Install(Box<dyn PackageSource>),
    Enable(String),
    Disable(String),
    Remove(String),
    Setting {
        plugin: String,
        key: String,
        value: Value,
    },
    Command {
        plugin: String,
        id: String,
        arguments: Value,
        context: EditorContext,
    },
    Ui {
        plugin: String,
        event: UiEvent,
        context: EditorContext,
    },
    Event {
        topic: String,
        payload: Value,
        context: EditorContext,
    },
    Notify {
        plugin: String,
        topic: String,
        payload: Value,
        context: EditorContext,
    },
    Hook {
        point: String,
        payload: Value,
        context: EditorContext,
    },
    ScopedHook {
        invocation: Invocation,
        point: String,
        payload: Value,
        context: EditorContext,
    },
    ServiceReply {
        call: HostCall,
        outcome: ServiceOutcome,
    },
    Render {
        plugin: String,
        frame: RenderFrame,
        context: EditorContext,
    },
}

#[derive(Default)]
pub struct Output {
    pub effects: Vec<OwnedEffect>,
    pub calls: Vec<HostCall>,
    pub result: Value,
    pub cancelled: Option<String>,
    /// Activations that must still be current when using the result payload.
    pub required: Vec<Origin>,
    pub interaction: Option<crate::Interaction>,
    pub operation: Option<Operation>,
    pub artifact: Option<Artifact>,
    pub invocation: Option<Invocation>,
}
pub struct Completion {
    pub id: u64,
    pub result: Result<Output, String>,
}
#[derive(Clone, Debug, Default)]
pub struct Registry {
    pub plugins: Vec<PluginInfo>,
    pub diagnostics: Vec<String>,
    revoked: BTreeMap<String, u64>,
}
impl Registry {
    pub(crate) fn valid_scope(&self, origin: &Origin, required: &[Origin]) -> bool {
        self.current(origin).is_some() && required.iter().all(|p| self.current(p).is_some())
    }
    pub(crate) fn current(&self, origin: &Origin) -> Option<&PluginInfo> {
        if self.revoked.contains_key(&origin.plugin) {
            return None;
        }
        self.plugins
            .iter()
            .find(|p| p.origin.as_ref() == Some(origin))
    }
}

pub struct Runtime {
    sender: Option<SyncSender<(u64, Action)>>,
    receiver: Receiver<Completion>,
    pub(crate) registry: Arc<Mutex<Registry>>,
    assets: Arc<Mutex<assets::Assets>>,
    next: u64,
    pub(crate) next_interaction: u64,
    next_operation: u64,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Runtime {
    pub fn spawn(root: Option<PathBuf>, builtins: Vec<Builtin>) -> Result<Self, String> {
        let (sender, input) = mpsc::sync_channel(32);
        let (output, receiver) = mpsc::channel();
        let registry = Arc::new(Mutex::new(Registry::default()));
        let shared = registry.clone();
        let assets = Arc::new(Mutex::new(assets::Assets::default()));
        let shared_assets = assets.clone();
        let worker = std::thread::Builder::new()
            .name("phistudio-plugins".into())
            .spawn(move || {
                let _shutdown = ShutdownRegistry(shared.clone());
                let mut state = Worker {
                    host: Host::default(),
                    root,
                    preferences: Preferences::default(),
                    builtins,
                    diagnostics: vec![],
                    persistence_error: None,
                    provider_jobs: BTreeMap::new(),
                    assets: shared_assets,
                    package_roots: BTreeMap::new(),
                };
                let initial = state.start().and_then(|effects| {
                    let mut result = Output {
                        effects,
                        ..Default::default()
                    };
                    state.finish(
                        &mut result,
                        &EditorContext::default(),
                        &shared,
                        0,
                        Budget::new(false),
                    )?;
                    Ok(result)
                });
                publish(&state, &shared, 0);
                if output
                    .send(Completion {
                        id: 0,
                        result: initial,
                    })
                    .is_err()
                {
                    return;
                }
                while let Ok((id, action)) = input.recv() {
                    let operation = match &action {
                        Action::Provider { operation, .. } => Some(operation.clone()),
                        Action::ServiceReply { call, .. } => call.operation().cloned(),
                        _ => None,
                    };
                    let result =
                        state
                            .run(action, &shared)
                            .and_then(|(mut result, context, budget)| {
                                state.finish(&mut result, &context, &shared, id, budget)?;
                                Ok(result)
                            });
                    if result.is_err()
                        && let Some(operation) = operation
                    {
                        operation.cancel();
                    }
                    publish(&state, &shared, id);
                    state.prune_providers();
                    state.prune_assets(&shared, id);
                    if output.send(Completion { id, result }).is_err() {
                        break;
                    }
                }
                let ids: Vec<_> = state.host.manifests().map(|m| m.id.clone()).collect();
                for id in ids {
                    state.host.disable(&id);
                }
                if let Ok(mut assets) = state.assets.lock() {
                    assets.clear();
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            sender: Some(sender),
            receiver,
            registry,
            assets,
            next: 1,
            next_interaction: 1,
            next_operation: 1,
            worker: Some(worker),
        })
    }

    pub fn submit(&mut self, action: Action) -> Result<u64, String> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| "Plugin registry interrupted")?;
        let revoked = revocations(&registry, &action);
        let hook_scope = match &action {
            Action::ScopedHook { invocation, .. } => Some(invocation.clone()),
            _ => None,
        };
        let mut hook_state = if let (Some(invocation), Action::ScopedHook { context, .. }) =
            (&hook_scope, &action)
        {
            if !invocation.belongs_to(&self.registry) {
                return Err("Hook invocation belongs to another runtime".into());
            }
            Some(invocation.admit(context)?)
        } else {
            None
        };
        if let Action::Provider { operation, .. } | Action::CancelProvider(operation) = &action
            && !operation.belongs_to(&self.registry)
        {
            return Err("Provider operation belongs to another runtime".into());
        }
        let provider_scope = match &action {
            Action::Provider { operation, .. } => Some(operation.clone()),
            _ => None,
        };
        let mut provider_state = if let (Some(operation), Action::Provider { context, .. }) =
            (&provider_scope, &action)
        {
            Some(operation.admit(context, &registry)?)
        } else {
            None
        };
        let input_scope = match &action {
            Action::Input { interaction, .. } => Some(interaction.clone()),
            _ => None,
        };
        let mut input_state = if let (Some(interaction), Action::Input { input, context, .. }) =
            (&input_scope, &action)
        {
            if !registry.valid_scope(interaction.origin(), &[])
                && input.phase() != Some(InputPhase::Cancel)
            {
                return Err("Input tool was disabled or reloaded".into());
            }
            Some(interaction.admit(input, context)?)
        } else {
            None
        };
        if let Action::ServiceReply { call, .. } = &action
            && (!registry.valid_scope(&call.origin, &call.required)
                || call
                    .invocation()
                    .is_some_and(|i| !i.belongs_to(&self.registry) || !i.is_active())
                || call.interaction().is_some_and(|i| !i.is_active())
                || call.operation().is_some_and(|o| {
                    !o.belongs_to(&self.registry) || !o.current_in(&registry, true)
                }))
        {
            return Err("Service caller was disabled or reloaded".into());
        }
        let id = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or("Plugin request sequence exhausted")?;
        self.sender
            .as_ref()
            .ok_or("Plugin worker stopped")?
            .try_send((id, action))
            .map_err(|e| match e {
                mpsc::TrySendError::Full(_) => "Plugin queue is busy".to_owned(),
                mpsc::TrySendError::Disconnected(_) => "Plugin worker stopped".to_owned(),
            })?;
        if let Some((state, next)) = &mut input_state {
            **state = next.clone();
        }
        if let Some(state) = &mut provider_state {
            state.started = true;
        }
        if let Some(state) = &mut hook_state {
            state.started = true;
        }
        for plugin in revoked {
            registry.revoked.insert(plugin, id);
        }
        Ok(id)
    }
    pub fn poll(&self) -> Result<Option<Completion>, String> {
        match self.receiver.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err("Plugin worker stopped".into()),
        }
    }
    pub fn registry(&self) -> Result<Registry, String> {
        self.registry
            .lock()
            .map(|r| r.clone())
            .map_err(|_| "Plugin registry interrupted".into())
    }
    pub fn is_current(&self, origin: &Origin) -> bool {
        self.registry
            .lock()
            .is_ok_and(|r| r.current(origin).is_some())
    }
    pub fn is_invocation_current(&self, invocation: &Invocation, required: &[Origin]) -> bool {
        if self.sender.is_none() {
            return false;
        }
        self.registry.lock().is_ok_and(|r| {
            invocation.belongs_to(&self.registry)
                && invocation.is_active()
                && required.iter().all(|p| r.current(p).is_some())
        })
    }
    pub fn is_effect_current(&self, effect: &OwnedEffect) -> bool {
        self.registry.lock().is_ok_and(|r| {
            r.valid_scope(&effect.origin, &effect.required)
                && effect
                    .invocation
                    .as_ref()
                    .is_none_or(|i| i.belongs_to(&self.registry) && i.is_active())
                && effect.interaction.as_ref().is_none_or(|i| i.is_active())
                && effect
                    .operation
                    .as_ref()
                    .is_none_or(|o| o.belongs_to(&self.registry) && o.current_in(&r, false))
        })
    }
    pub fn is_call_current(&self, call: &HostCall) -> bool {
        self.registry.lock().is_ok_and(|r| {
            r.valid_scope(&call.origin, &call.required)
                && call
                    .invocation()
                    .is_none_or(|i| i.belongs_to(&self.registry) && i.is_active())
                && call.interaction().is_none_or(|i| i.is_active())
                && call
                    .operation()
                    .is_none_or(|o| o.belongs_to(&self.registry) && o.current_in(&r, true))
        })
    }
    pub fn is_output_current(&self, output: &Output) -> bool {
        if output.invocation.is_some() && self.sender.is_none() {
            return false;
        }
        self.registry.lock().is_ok_and(|r| {
            output.required.iter().all(|p| r.current(p).is_some())
                && output
                    .invocation
                    .as_ref()
                    .is_none_or(|i| i.belongs_to(&self.registry) && i.is_active())
                && output.interaction.as_ref().is_none_or(|i| i.is_active())
                && output
                    .operation
                    .as_ref()
                    .is_none_or(|o| o.belongs_to(&self.registry) && o.current_in(&r, false))
        })
    }
    pub fn apply_transaction(
        &self,
        origin: &Origin,
        required: &[Origin],
        session: u64,
        editor: &mut Editor,
        transaction: Transaction,
    ) -> Result<bool, String> {
        self.apply_scoped_transaction(origin, required, None, session, editor, transaction)
    }
    pub fn apply_scoped_transaction(
        &self,
        origin: &Origin,
        required: &[Origin],
        interaction: Option<&crate::Interaction>,
        session: u64,
        editor: &mut Editor,
        transaction: Transaction,
    ) -> Result<bool, String> {
        let registry = self
            .registry
            .lock()
            .map_err(|_| "Plugin registry interrupted")?;
        let plugin = registry
            .current(origin)
            .ok_or("Plugin action was cancelled or its activation changed")?;
        if !registry.valid_scope(origin, required) {
            return Err("Plugin action's caller was disabled or reloaded".into());
        }
        if !plugin.granted.contains(&Capability::DocumentEdit) {
            return Err("Plugin document editing is not granted".into());
        }
        if transaction.version.session != session {
            return Err("Plugin action belongs to another project session".into());
        }
        validate_plugin_data_owner(&transaction.operations, &origin.plugin)?;
        if interaction.is_some_and(|i| {
            i.version() != transaction.version
                || (i.origin() != origin && !required.contains(i.origin()))
        }) {
            return Err("Gesture owner or version does not match the action scope".into());
        }
        let mut gesture = interaction.map(|i| i.lock()).transpose()?;
        if let Some(state) = &gesture
            && (state.cancelled || state.committed || !state.ended)
        {
            return Err("Gesture was cancelled, has not ended or already committed".into());
        }
        let changed = editor
            .apply_patch(
                transaction.version.revision,
                transaction.label,
                transaction.operations,
            )
            .map_err(|e| e.to_string())?;
        if changed && let Some(state) = &mut gesture {
            state.committed = true;
        }
        Ok(changed)
    }
    /// Shutdown happens after the render loop, never while drawing a frame.
    pub fn shutdown(&mut self) -> Result<(), String> {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| "Plugin worker failed during shutdown".to_owned())?;
        }
        Ok(())
    }
}

struct ShutdownRegistry(Arc<Mutex<Registry>>);
impl Drop for ShutdownRegistry {
    fn drop(&mut self) {
        if let Ok(mut registry) = self.0.lock() {
            for plugin in &mut registry.plugins {
                plugin.status = Status::Disabled;
                plugin.origin = None;
                plugin.contributions.clear();
            }
        }
    }
}

fn revocations(registry: &Registry, action: &Action) -> BTreeSet<String> {
    let mut ids = match action {
        Action::Reload => registry
            .plugins
            .iter()
            .map(|p| p.manifest.id.clone())
            .collect(),
        Action::Disable(id) | Action::Remove(id) => BTreeSet::from([id.clone()]),
        _ => BTreeSet::new(),
    };
    loop {
        let old = ids.len();
        for plugin in &registry.plugins {
            if plugin
                .manifest
                .dependencies
                .keys()
                .any(|dep| ids.contains(dep))
            {
                ids.insert(plugin.manifest.id.clone());
            }
        }
        if old == ids.len() {
            break;
        }
    }
    ids
}
fn publish(worker: &Worker, registry: &Mutex<Registry>, completed: u64) {
    if let Ok(mut registry) = registry.lock() {
        registry.plugins = worker.host.snapshot();
        registry.diagnostics = worker.diagnostics.clone();
        registry.revoked.retain(|_, request| *request > completed);
    }
}

struct Worker {
    host: Host,
    root: Option<PathBuf>,
    preferences: Preferences,
    builtins: Vec<Builtin>,
    diagnostics: Vec<String>,
    persistence_error: Option<String>,
    provider_jobs: BTreeMap<u64, providers::Files>,
    assets: Arc<Mutex<assets::Assets>>,
    package_roots: BTreeMap<String, PathBuf>,
}
impl Worker {
    fn library(&self) -> Result<PathBuf, String> {
        self.root
            .as_ref()
            .map(|r| r.join("plugins"))
            .ok_or("Open the project library before installing plugins".into())
    }
    fn start(&mut self) -> Result<Vec<OwnedEffect>, String> {
        let loaded = self
            .root
            .as_ref()
            .map(|root| preferences::read::<Preferences>(&root.join("plugins-state.json")))
            .transpose();
        let loaded = loaded.and_then(|p| {
            let p = p.unwrap_or_default();
            if p.version != 1 {
                Err("Unsupported plugin settings version".into())
            } else {
                Ok(p)
            }
        });
        match loaded {
            Ok(preferences) => {
                self.preferences = preferences;
                self.persistence_error = None;
            }
            Err(error) => {
                self.persistence_error = Some(error.clone());
                return Err(error);
            }
        }
        self.load()
    }
    fn load(&mut self) -> Result<Vec<OwnedEffect>, String> {
        self.package_roots.clear();
        let ids: Vec<_> = self.host.manifests().map(|m| m.id.clone()).collect();
        for id in ids {
            self.host.uninstall(&id);
        }
        self.diagnostics.clear();
        let mut defaults = BTreeMap::new();
        for builtin in &self.builtins {
            self.host
                .install(
                    builtin.manifest.clone(),
                    (builtin.create)(),
                    builtin.manifest.capabilities.clone(),
                )
                .map_err(|e| e.to_string())?;
            defaults.insert(builtin.manifest.id.clone(), builtin.enabled);
        }
        if self.root.is_some() {
            for found in discover(&self.library()?).map_err(|e| e.to_string())? {
                let result = found.package.and_then(|package| {
                    let instance = package.load(WasmLimits::default())?;
                    let root = package.root().to_path_buf();
                    let manifest = package.manifest;
                    let id = manifest.id.clone();
                    self.host
                        .install(manifest.clone(), instance, manifest.capabilities)
                        .map_err(|e| e.to_string())?;
                    self.package_roots.insert(id, root);
                    Ok(())
                });
                if let Err(error) = result {
                    self.diagnostics
                        .push(format!("{}: {error}", found.path.display()));
                }
            }
        }
        let ids: Vec<_> = self.host.manifests().map(|m| m.id.clone()).collect();
        let mut enabled = vec![];
        for id in ids {
            let restored = (|| -> Result<(), String> {
                if let Some(root) = &self.root {
                    self.host
                        .restore_storage(
                            &id,
                            preferences::read(
                                &root.join("plugins-data").join(format!("{id}.json")),
                            )?,
                        )
                        .map_err(|e| e.to_string())?;
                }
                if let Some(preference) = self.preferences.plugins.get(&id) {
                    for (key, value) in &preference.settings {
                        if let Err(e) = self.host.set_setting(&id, key, value.clone()) {
                            self.diagnostics.push(e.to_string());
                        }
                    }
                }
                Ok(())
            })();
            if let Err(error) = restored {
                self.diagnostics.push(format!("{id}: {error}"));
                self.host.uninstall(&id);
                continue;
            }
            let desired = self
                .preferences
                .plugins
                .get(&id)
                .map_or(*defaults.get(&id).unwrap_or(&false), |p| p.enabled);
            self.preferences
                .plugins
                .entry(id.clone())
                .or_insert_with(|| preferences::PluginPreference {
                    enabled: desired,
                    ..Default::default()
                });
            if desired {
                enabled.push(id);
            }
        }
        let mut effects = vec![];
        for id in enabled {
            match self.host.enable(&id) {
                Ok(next) => effects.extend(next),
                Err(error) => self.diagnostics.push(error.to_string()),
            }
        }
        Ok(effects)
    }
    fn remember(&mut self, enabled: bool) -> Result<(), String> {
        if let Some(error) = &self.persistence_error {
            return Err(error.clone());
        }
        for info in self.host.snapshot() {
            let entry = self
                .preferences
                .plugins
                .entry(info.manifest.id)
                .or_default();
            if enabled {
                entry.enabled = info.status == Status::Enabled;
            }
            entry.settings.extend(info.settings);
        }
        if let Some(root) = &self.root {
            preferences::write(&root.join("plugins-state.json"), &self.preferences)?;
        }
        Ok(())
    }
    fn run(
        &mut self,
        action: Action,
        registry: &Mutex<Registry>,
    ) -> Result<(Output, EditorContext, Budget), String> {
        if matches!(
            action,
            Action::Install(_) | Action::Enable(_) | Action::Remove(_) | Action::Setting { .. }
        ) && let Some(error) = &self.persistence_error
        {
            return Err(error.clone());
        }
        let mut output = Output::default();
        match &action {
            Action::Provider { operation, .. } => {
                output.required.push(operation.origin().clone());
                output.operation = Some(operation.clone());
            }
            Action::Command { plugin, .. }
            | Action::Ui { plugin, .. }
            | Action::Notify { plugin, .. }
            | Action::Render { plugin, .. } => {
                output.required.extend(self.host.origin(plugin));
            }
            Action::Input { interaction, .. } => {
                output.required.push(interaction.origin().clone());
                output.interaction = Some(interaction.clone());
            }
            Action::ServiceReply { call, .. } => {
                output.required.push(call.origin.clone());
                output.required.extend(call.required.clone());
                output.interaction = call.interaction().cloned();
                output.operation = call.operation().cloned();
                output.invocation = call.invocation().cloned();
            }
            Action::ScopedHook { invocation, .. } => output.invocation = Some(invocation.clone()),
            _ => {}
        }
        let mut context = EditorContext::default();
        let budget = match &action {
            Action::Command { plugin, .. } | Action::Ui { plugin, .. }
                if self.host.granted(plugin, Capability::LongRunning) =>
            {
                Budget::long_running()
            }
            Action::Provider { operation, .. } => Budget::provider(operation.clone()),
            Action::Input {
                interaction, input, ..
            } => Budget::input(interaction.clone(), input.phase() == Some(InputPhase::End)),
            Action::ServiceReply { call, .. } => call.budget.clone(),
            Action::Hook { .. } => Budget::new(false).interceptor(),
            Action::ScopedHook { invocation, .. } => Budget::hook(invocation.clone()),
            Action::Render { .. } => Budget::background(),
            _ => Budget::new(matches!(
                action,
                Action::Command { .. }
                    | Action::Ui { .. }
                    | Action::Event { .. }
                    | Action::Notify { .. }
            )),
        };
        match action {
            Action::Provider {
                operation,
                source,
                name,
                context: c,
            } => {
                let result = self.start_provider(operation, source, name, c.clone())?;
                output.effects = result.effects;
                output.result = result.result;
                context = c;
            }
            Action::CancelProvider(operation) => {
                operation.cancel();
                self.provider_jobs.remove(&operation.id());
            }
            Action::Input {
                interaction,
                input,
                context: c,
            } => {
                let cancel = input.phase() == Some(InputPhase::Cancel);
                if !cancel && !interaction.is_active() {
                    return Ok((output, c, budget));
                }
                let result = self.host.input(
                    &interaction.origin().plugin,
                    interaction.tool(),
                    input,
                    c.clone(),
                );
                if !cancel {
                    let result = result.map_err(|e| e.to_string())?;
                    output.effects = result.effects;
                    output.result = result.result;
                }
                context = c;
            }
            Action::Reload => {
                let ids: Vec<_> = self.host.manifests().map(|m| m.id.clone()).collect();
                for id in ids {
                    self.host.uninstall(&id);
                }
                output.effects = self.start()?;
            }
            Action::Install(source) => {
                let library = self.library()?;
                let (package, prepared) = install_package(source.path(), &library, |package| {
                    if self.host.status(&package.manifest.id).is_some() {
                        return Err("Plugin is already installed".into());
                    }
                    package.prepare(WasmLimits::default())
                })?;
                let instance = prepared.instantiate(&package)?;
                let package_root = package.root().to_path_buf();
                let manifest = package.manifest;
                let id = manifest.id.clone();
                self.host
                    .install(manifest.clone(), instance, manifest.capabilities)
                    .map_err(|e| e.to_string())?;
                self.package_roots.insert(id.clone(), package_root);
                if let Some(root) = &self.root {
                    self.host
                        .restore_storage(
                            &id,
                            preferences::read(
                                &root.join("plugins-data").join(format!("{id}.json")),
                            )?,
                        )
                        .map_err(|e| e.to_string())?;
                }
                if let Some(preference) = self.preferences.plugins.get(&id) {
                    for (key, value) in &preference.settings {
                        if let Err(error) = self.host.set_setting(&id, key, value.clone()) {
                            self.diagnostics.push(error.to_string());
                        }
                    }
                }
                self.remember(false)?;
                output.result = Value::from(id);
            }
            Action::Enable(id) => {
                output.effects = self.host.enable(&id).map_err(|e| e.to_string())?;
                self.remember(true)?;
            }
            Action::Disable(id) => {
                self.host.disable(&id);
                self.remember(true)?;
            }
            Action::Remove(id) => {
                let info = self
                    .host
                    .snapshot()
                    .into_iter()
                    .find(|p| p.manifest.id == id)
                    .ok_or("Plugin is not installed")?;
                if matches!(info.manifest.backend, Backend::Builtin) {
                    return Err("Bundled plugins can be disabled but not removed".into());
                }
                retire_package(&self.library()?, &id)?;
                self.host.uninstall(&id);
                self.package_roots.remove(&id);
                self.preferences.plugins.entry(id).or_default().enabled = false;
                self.remember(true)?;
            }
            Action::Setting { plugin, key, value } => {
                output.effects = self
                    .host
                    .set_setting(&plugin, &key, value)
                    .map_err(|e| e.to_string())?;
                self.remember(false)?;
            }
            Action::Command {
                plugin,
                id,
                arguments,
                context: c,
            } => {
                let result = self
                    .host
                    .command(&plugin, &id, arguments, c.clone())
                    .map_err(|e| e.to_string())?;
                output.effects = result.effects;
                output.result = result.result;
                context = c;
            }
            Action::Ui {
                plugin,
                event,
                context: c,
            } => {
                let result = self
                    .host
                    .ui(&plugin, event, c.clone())
                    .map_err(|e| e.to_string())?;
                output.effects = result.effects;
                output.result = result.result;
                context = c;
            }
            Action::Event {
                topic,
                payload,
                context: c,
            } => {
                let (effects, errors) = self.host.event(&topic, payload, c.clone());
                output.effects = effects;
                self.diagnostics
                    .extend(errors.into_iter().map(|e| e.to_string()));
                context = c;
            }
            Action::Notify {
                plugin,
                topic,
                payload,
                context: c,
            } => {
                let result = self
                    .host
                    .event_to(&plugin, &topic, payload, c.clone())
                    .map_err(|e| e.to_string())?;
                output.effects = result.effects;
                output.result = result.result;
                context = c;
            }
            Action::Hook {
                point,
                payload,
                context: c,
            }
            | Action::ScopedHook {
                point,
                payload,
                context: c,
                ..
            } => {
                if output.invocation.as_ref().is_some_and(|i| !i.is_active()) {
                    return Ok((output, c, budget));
                }
                let result = self
                    .host
                    .hook_checked(&point, payload, c.clone(), |origin| {
                        output.invocation.as_ref().is_none_or(Invocation::is_active)
                            && registry
                                .lock()
                                .is_ok_and(|r| !r.revoked.contains_key(&origin.plugin))
                    })
                    .map_err(|e| e.to_string())?;
                output.effects = result.effects;
                output.result = result.payload;
                output.cancelled = result.cancelled;
                output.required = result.required;
                context = c;
            }
            Action::ServiceReply { call, outcome } => {
                budget.take()?;
                let result = self
                    .host
                    .service_result(
                        &call.origin,
                        ServiceReply {
                            request_id: call.request.request_id,
                            service: call.reply_service,
                            outcome,
                        },
                        call.context.clone(),
                    )
                    .map_err(|e| e.to_string())?;
                output.effects = result.effects;
                for effect in &mut output.effects {
                    effect.required.clone_from(&call.required);
                }
                output.result = result.result;
                context = call.context;
            }
            Action::Render {
                plugin,
                frame,
                context: c,
            } => {
                let result = self
                    .host
                    .render(&plugin, frame, c.clone())
                    .map_err(|e| e.to_string())?;
                output.effects = result.effects;
                output.result = result.result;
                context = c;
            }
        }
        Ok((output, context, budget))
    }
}
