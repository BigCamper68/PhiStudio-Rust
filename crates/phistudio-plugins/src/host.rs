use phistudio_sdk::*;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    panic::{AssertUnwindSafe, catch_unwind},
};
mod hooks;
mod providers;

pub const MAX_CONTRIBUTIONS: usize = 512;
pub const MAX_EFFECTS: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Disabled,
    Enabled,
    Failed(String),
}

struct Slot {
    diagnostics: PluginDiagnostics,
    manifest: Manifest,
    instance: Box<dyn Plugin>,
    granted: BTreeSet<Capability>,
    settings: BTreeMap<String, Value>,
    status: Status,
    generation: u64,
    contributions: Vec<Contribution>,
    storage: Value,
}

#[derive(Clone, Debug)]
pub struct PluginInfo {
    pub diagnostics: PluginDiagnostics,
    pub manifest: Manifest,
    pub status: Status,
    pub settings: BTreeMap<String, Value>,
    pub granted: BTreeSet<Capability>,
    pub origin: Option<Origin>,
    pub contributions: Vec<Contribution>,
}

/// A generation prevents queued work from an old activation surviving a reload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    pub plugin: String,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OwnedEffect {
    pub origin: Origin,
    /// Callers whose activations must remain current until this effect applies.
    pub required: Vec<Origin>,
    pub interaction: Option<crate::Interaction>,
    pub operation: Option<crate::Operation>,
    pub invocation: Option<crate::Invocation>,
    pub effect: Effect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CallResult {
    pub effects: Vec<OwnedEffect>,
    pub result: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HookResult {
    pub payload: Value,
    pub cancelled: Option<String>,
    pub effects: Vec<OwnedEffect>,
    pub required: Vec<Origin>,
    pub replacement: Option<Origin>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("Plugin {plugin}: {message}")]
pub struct HostError {
    pub plugin: String,
    pub message: String,
}

fn error(id: &str, message: impl Into<String>) -> HostError {
    HostError {
        plugin: id.into(),
        message: message.into(),
    }
}

#[derive(Default)]
pub struct Host {
    slots: BTreeMap<String, Slot>,
    generation: u64,
}

impl Host {
    pub fn install(
        &mut self,
        manifest: Manifest,
        instance: Box<dyn Plugin>,
        granted: BTreeSet<Capability>,
    ) -> Result<(), HostError> {
        manifest.validate().map_err(|e| error(&manifest.id, e))?;
        if self.slots.contains_key(&manifest.id) {
            return Err(error(&manifest.id, "Plugin is already installed"));
        }
        if !granted.is_subset(&manifest.capabilities) {
            return Err(error(&manifest.id, "Cannot grant undeclared capabilities"));
        }
        let settings = manifest
            .settings
            .iter()
            .map(|(k, s)| (k.clone(), s.default_value()))
            .collect();
        self.slots.insert(
            manifest.id.clone(),
            Slot {
                diagnostics: PluginDiagnostics::default(),
                manifest,
                instance,
                granted,
                settings,
                status: Status::Disabled,
                generation: 0,
                contributions: vec![],
                storage: Value::Null,
            },
        );
        Ok(())
    }

    pub fn manifests(&self) -> impl Iterator<Item = &Manifest> {
        self.slots.values().map(|s| &s.manifest)
    }
    pub fn set_granted(
        &mut self,
        id: &str,
        granted: BTreeSet<Capability>,
    ) -> Result<(), HostError> {
        let slot = self
            .slots
            .get_mut(id)
            .ok_or_else(|| error(id, "Plugin is not installed"))?;
        if slot.status == Status::Enabled || !granted.is_subset(&slot.manifest.capabilities) {
            return Err(error(
                id,
                "Disable the plugin before changing its declared permissions",
            ));
        }
        slot.granted = granted;
        Ok(())
    }
    pub fn snapshot(&self) -> Vec<PluginInfo> {
        self.slots
            .values()
            .map(|slot| PluginInfo {
                diagnostics: slot.diagnostics.clone(),
                manifest: slot.manifest.clone(),
                status: slot.status.clone(),
                settings: slot.settings.clone(),
                granted: slot.granted.clone(),
                origin: (slot.status == Status::Enabled).then(|| Origin {
                    plugin: slot.manifest.id.clone(),
                    generation: slot.generation,
                }),
                contributions: slot.contributions.clone(),
            })
            .collect()
    }
    pub fn storage(&self, id: &str) -> Option<&Value> {
        self.slots.get(id).map(|s| &s.storage)
    }
    pub fn restore_storage(&mut self, id: &str, value: Value) -> Result<(), HostError> {
        let slot = self
            .slots
            .get_mut(id)
            .ok_or_else(|| error(id, "Plugin is not installed"))?;
        if slot.status == Status::Enabled {
            return Err(error(id, "Restore storage before activation"));
        }
        slot.storage = value;
        Ok(())
    }
    pub fn store(&mut self, origin: &Origin, value: Value) -> Result<(), HostError> {
        if !self.is_current(origin) || !self.granted(&origin.plugin, Capability::Storage) {
            return Err(error(
                &origin.plugin,
                "Plugin storage access is no longer current",
            ));
        }
        self.slots.get_mut(&origin.plugin).unwrap().storage = value;
        Ok(())
    }
    pub fn status(&self, id: &str) -> Option<&Status> {
        self.slots.get(id).map(|s| &s.status)
    }
    pub fn settings(&self, id: &str) -> Option<&BTreeMap<String, Value>> {
        self.slots.get(id).map(|s| &s.settings)
    }
    pub fn granted(&self, id: &str, capability: Capability) -> bool {
        self.slots
            .get(id)
            .is_some_and(|s| s.granted.contains(&capability))
    }
    pub fn is_current(&self, origin: &Origin) -> bool {
        self.slots
            .get(&origin.plugin)
            .is_some_and(|s| s.status == Status::Enabled && s.generation == origin.generation)
    }
    pub fn origin(&self, plugin: &str) -> Option<Origin> {
        self.slots
            .get(plugin)
            .filter(|s| s.status == Status::Enabled)
            .map(|s| Origin {
                plugin: plugin.into(),
                generation: s.generation,
            })
    }
    pub fn contributions(&self) -> impl Iterator<Item = (&str, &Contribution)> {
        self.slots
            .iter()
            .filter(|(_, s)| s.status == Status::Enabled)
            .flat_map(|(id, s)| s.contributions.iter().map(move |c| (id.as_str(), c)))
    }

    fn plan(
        &self,
        id: &str,
        visiting: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
        order: &mut Vec<String>,
    ) -> Result<(), HostError> {
        if done.contains(id) {
            return Ok(());
        }
        let slot = self
            .slots
            .get(id)
            .ok_or_else(|| error(id, "Required plugin is not installed"))?;
        if !visiting.insert(id.into()) {
            return Err(error(id, "Dependency cycle"));
        }
        for (dependency, required) in &slot.manifest.dependencies {
            let other = self
                .slots
                .get(dependency)
                .ok_or_else(|| error(id, format!("Missing dependency {dependency}")))?;
            if !required.matches(&other.manifest.version) {
                return Err(error(
                    id,
                    format!("Dependency {dependency} does not match {required}"),
                ));
            }
            self.plan(dependency, visiting, done, order)?;
        }
        visiting.remove(id);
        done.insert(id.into());
        order.push(id.into());
        Ok(())
    }

    pub fn activation_plan(&self, id: &str) -> Result<Vec<String>, HostError> {
        let mut order = vec![];
        self.plan(id, &mut BTreeSet::new(), &mut BTreeSet::new(), &mut order)?;
        Ok(order)
    }

    pub fn enable(&mut self, id: &str) -> Result<Vec<OwnedEffect>, HostError> {
        let order = self.activation_plan(id)?;
        let wanted: BTreeSet<_> = order
            .iter()
            .map(String::as_str)
            .chain(
                self.slots
                    .iter()
                    .filter(|(_, s)| s.status == Status::Enabled)
                    .map(|(id, _)| id.as_str()),
            )
            .collect();
        for enabled in &wanted {
            if let Some(conflict) = self.slots[*enabled]
                .manifest
                .conflicts
                .iter()
                .find(|c| wanted.contains(c.as_str()))
            {
                return Err(error(enabled, format!("Conflicts with {conflict}")));
            }
        }
        let mut effects = vec![];
        let mut activated = vec![];
        for next in order {
            if self.slots[&next].status == Status::Enabled {
                continue;
            }
            self.generation = self
                .generation
                .checked_add(1)
                .ok_or_else(|| error(&next, "Activation generation exhausted"))?;
            let slot = self.slots.get_mut(&next).unwrap();
            slot.generation = self.generation;
            let request = Request::Activate {
                manifest: slot.manifest.clone(),
                settings: slot.settings.clone(),
                storage: if slot.granted.contains(&Capability::Storage) {
                    slot.storage.clone()
                } else {
                    Value::Null
                },
            };
            match invoke(slot, &request) {
                Ok(response) => {
                    slot.contributions = response.contributions;
                    slot.status = Status::Enabled;
                    effects.extend(owned(slot, response.effects));
                    activated.push(next);
                }
                Err(failure) => {
                    slot.status = Status::Failed(failure.message.clone());
                    slot.contributions.clear();
                    for previous in activated.iter().rev() {
                        self.disable(previous);
                    }
                    return Err(failure);
                }
            }
        }
        Ok(effects)
    }

    /// Dependants stop before dependencies. Cleanup is unconditional, even when
    /// the extension traps during deactivation. Persistent data is not removed.
    pub fn disable(&mut self, id: &str) -> Vec<String> {
        let mut disabled = vec![];
        self.disable_inner(id, &mut BTreeSet::new(), &mut disabled);
        disabled
    }
    fn disable_inner(&mut self, id: &str, seen: &mut BTreeSet<String>, disabled: &mut Vec<String>) {
        if !seen.insert(id.into()) {
            return;
        }
        let dependants: Vec<_> = self
            .slots
            .iter()
            .filter(|(_, s)| {
                s.status == Status::Enabled && s.manifest.dependencies.contains_key(id)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for other in dependants {
            self.disable_inner(&other, seen, disabled);
        }
        if let Some(slot) = self.slots.get_mut(id) {
            if slot.status == Status::Enabled {
                let _ = invoke(slot, &Request::Deactivate);
            }
            slot.contributions.clear();
            slot.generation = 0;
            slot.status = Status::Disabled;
            disabled.push(id.into());
        }
    }
    pub fn uninstall(&mut self, id: &str) -> Vec<String> {
        let disabled = self.disable(id);
        self.slots.remove(id);
        disabled
    }

    fn call(&mut self, id: &str, mut request: Request) -> Result<Response, HostError> {
        let slot = self
            .slots
            .get_mut(id)
            .ok_or_else(|| error(id, "Plugin is not installed"))?;
        if slot.status != Status::Enabled {
            return Err(error(id, "Plugin is not enabled"));
        }
        if !slot.granted.contains(&Capability::DocumentRead)
            && let Some(context) = request.context_mut()
        {
            context.chart = None;
        }
        let result = invoke(slot, &request);
        if let Err(failure) = &result {
            self.disable(id);
            self.slots.get_mut(id).unwrap().status = Status::Failed(failure.message.clone());
        }
        result
    }

    pub fn command(
        &mut self,
        plugin: &str,
        id: &str,
        arguments: Value,
        context: EditorContext,
    ) -> Result<CallResult, HostError> {
        self.require(
            plugin,
            |c| matches!(c, Contribution::Command { id: registered, .. } if registered == id),
        )?;
        let response = self.call(
            plugin,
            Request::Command {
                id: id.into(),
                arguments,
                context,
            },
        )?;
        Ok(CallResult {
            effects: owned(&self.slots[plugin], response.effects),
            result: response.result,
        })
    }
    pub fn ui(
        &mut self,
        plugin: &str,
        event: UiEvent,
        context: EditorContext,
    ) -> Result<CallResult, HostError> {
        self.require(
            plugin,
            |c| matches!(c, Contribution::Panel { id, .. } if id == &event.panel),
        )?;
        let response = self.call(plugin, Request::Ui { event, context })?;
        Ok(CallResult {
            effects: owned(&self.slots[plugin], response.effects),
            result: response.result,
        })
    }
    pub fn render(
        &mut self,
        plugin: &str,
        frame: RenderFrame,
        mut context: EditorContext,
    ) -> Result<CallResult, HostError> {
        frame.validate().map_err(|e| error(plugin, e))?;
        self.require(plugin, |c| match (frame.target.kind, c) {
            (RenderKind::Preview, Contribution::Preview { id })
            | (RenderKind::Track, Contribution::Track { id, .. })
            | (RenderKind::Track, Contribution::TimelineTrack { id, .. }) => id == &frame.target.id,
            (RenderKind::PreviewEffect, Contribution::PreviewEffect { id, .. }) => {
                id == &frame.target.id
            }
            _ => false,
        })?;
        context.chart = None;
        context.selection = Value::Null;
        let response = self.call(plugin, Request::Render { frame, context })?;
        Ok(CallResult {
            effects: owned(&self.slots[plugin], response.effects),
            result: response.result,
        })
    }
    pub fn input(
        &mut self,
        plugin: &str,
        tool: &str,
        input: InputBatch,
        mut context: EditorContext,
    ) -> Result<CallResult, HostError> {
        input.validate().map_err(|e| error(plugin, e))?;
        self.require(plugin,|c|matches!(c,Contribution::Tool {id,surfaces,buttons,..}
            if id==tool && surfaces.contains(&input.surface.kind) && buttons.contains(&input.pointer.button)))?;
        if input.surface.kind == InputSurfaceKind::Track {
            self.require(
                plugin,
                |c| matches!(c,Contribution::Track {id,..} | Contribution::TimelineTrack {id,..} if Some(id)==input.surface.id.as_ref()),
            )?;
        }
        context.chart = None;
        context.selection = Value::Null;
        let response = self.call(
            plugin,
            Request::Input {
                tool: tool.into(),
                input,
                context,
            },
        )?;
        Ok(CallResult {
            effects: owned(&self.slots[plugin], response.effects),
            result: response.result,
        })
    }
    fn require(
        &self,
        plugin: &str,
        predicate: impl Fn(&Contribution) -> bool,
    ) -> Result<(), HostError> {
        self.slots
            .get(plugin)
            .filter(|s| s.status == Status::Enabled && s.contributions.iter().any(predicate))
            .map(|_| ())
            .ok_or_else(|| error(plugin, "Contribution is not registered"))
    }

    pub fn event(
        &mut self,
        topic: &str,
        payload: Value,
        context: EditorContext,
    ) -> (Vec<OwnedEffect>, Vec<HostError>) {
        let listeners: Vec<_> = self
            .contributions()
            .filter(|(_, c)| matches!(c, Contribution::Event { topic: t } if t == topic))
            .map(|(id, _)| id.to_owned())
            .take(MAX_CONTRIBUTIONS + 1)
            .collect();
        if listeners.len() > MAX_CONTRIBUTIONS {
            return (
                vec![],
                vec![error("host", "Event dispatch exceeds its listener limit")],
            );
        }
        let mut effects = vec![];
        let mut errors = vec![];
        let mut bytes = 0;
        for id in listeners {
            if self.status(&id) != Some(&Status::Enabled) {
                continue;
            }
            match self.call(
                &id,
                Request::Event {
                    topic: topic.into(),
                    payload: payload.clone(),
                    context: context.clone(),
                },
            ) {
                Ok(response) => {
                    let size = crate::limits::serialized_size(
                        &response.effects,
                        MAX_MESSAGE_BYTES - bytes,
                    );
                    if effects.len() + response.effects.len() > MAX_EFFECTS || size.is_none() {
                        errors.push(error(
                            &id,
                            "Event dispatch exceeds its aggregate effect limit",
                        ));
                        break;
                    }
                    bytes += size.unwrap_or(0);
                    effects.extend(owned(&self.slots[&id], response.effects));
                }
                Err(error) => errors.push(error),
            }
        }
        (effects, errors)
    }

    pub fn event_to(
        &mut self,
        plugin: &str,
        topic: &str,
        payload: Value,
        context: EditorContext,
    ) -> Result<CallResult, HostError> {
        self.require(
            plugin,
            |c| matches!(c,Contribution::Event {topic:t} if t == topic),
        )?;
        let response = self.call(
            plugin,
            Request::Event {
                topic: topic.into(),
                payload,
                context,
            },
        )?;
        Ok(CallResult {
            effects: owned(&self.slots[plugin], response.effects),
            result: response.result,
        })
    }

    pub fn service(
        &mut self,
        consumer: &str,
        provider: &str,
        required: &VersionReq,
        request: Request,
    ) -> Result<CallResult, HostError> {
        let Request::Service { id, .. } = &request else {
            return Err(error(consumer, "Expected a service request"));
        };
        let slot = self
            .slots
            .get(consumer)
            .ok_or_else(|| error(consumer, "Plugin is not installed"))?;
        if slot.status != Status::Enabled
            || !slot.granted.contains(&Capability::Services)
            || (consumer != provider && !slot.manifest.dependencies.contains_key(provider))
        {
            return Err(error(
                consumer,
                "Service access needs a granted capability and a declared dependency",
            ));
        }
        self.require(provider, |c| matches!(c, Contribution::Service { id: registered, version } if registered == id && required.matches(version)))?;
        let response = self.call(provider, request)?;
        Ok(CallResult {
            effects: owned(&self.slots[provider], response.effects),
            result: response.result,
        })
    }

    /// Replies need no event subscription and cannot be broadcast or spoofed
    /// by an emitted event. The runtime retains their original action context.
    pub fn service_result(
        &mut self,
        recipient: &Origin,
        reply: ServiceReply,
        mut context: EditorContext,
    ) -> Result<CallResult, HostError> {
        if !self.is_current(recipient) {
            return Err(error(
                &recipient.plugin,
                "Service caller is no longer active",
            ));
        }
        if !self.granted(&recipient.plugin, Capability::Services)
            && !(reply.service.provider == "host" && reply.service.id == "features")
        {
            return Err(error(&recipient.plugin, "Services were not granted"));
        }
        // Replies use bounded query results rather than repeating snapshots.
        context.chart = None;
        let response = self.call(&recipient.plugin, Request::ServiceResult { reply, context })?;
        Ok(CallResult {
            effects: owned(&self.slots[&recipient.plugin], response.effects),
            result: response.result,
        })
    }

    pub fn set_setting(
        &mut self,
        id: &str,
        key: &str,
        value: Value,
    ) -> Result<Vec<OwnedEffect>, HostError> {
        let slot = self
            .slots
            .get_mut(id)
            .ok_or_else(|| error(id, "Plugin is not installed"))?;
        if !slot
            .manifest
            .settings
            .get(key)
            .is_some_and(|s| s.accepts(&value))
        {
            return Err(error(id, "Invalid setting value"));
        }
        slot.settings.insert(key.into(), value.clone());
        if slot.status != Status::Enabled {
            return Ok(vec![]);
        }
        let response = self.call(
            id,
            Request::SettingsChanged {
                key: key.into(),
                value,
            },
        )?;
        Ok(owned(&self.slots[id], response.effects))
    }
}

fn owned(slot: &Slot, effects: Vec<Effect>) -> Vec<OwnedEffect> {
    effects
        .into_iter()
        .map(|effect| OwnedEffect {
            origin: Origin {
                plugin: slot.manifest.id.clone(),
                generation: slot.generation,
            },
            required: vec![],
            interaction: None,
            operation: None,
            invocation: None,
            effect,
        })
        .collect()
}

fn invoke(slot: &mut Slot, request: &Request) -> Result<Response, HostError> {
    let id = slot.manifest.id.clone();
    if crate::limits::serialized_size(request, MAX_MESSAGE_BYTES).is_none() {
        return Err(error(&id, "Request exceeds message limit"));
    }
    let started = std::time::Instant::now();
    let result = catch_unwind(AssertUnwindSafe(|| slot.instance.call(request)))
        .map_err(|_| error(&id, "Plugin panicked"))
        .and_then(|response| response.map_err(|e| error(&id, e)))
        .and_then(|response| {
            validate_response(slot, request, &response).map_err(|e| error(&id, e))?;
            Ok(response)
        });
    let elapsed = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
    let callback = match request {
        Request::Activate { .. } => "activate",
        Request::Deactivate => "deactivate",
        Request::Command { .. } => "command",
        Request::Ui { .. } => "ui",
        Request::Render { .. } => "render",
        Request::Input { .. } => "input",
        Request::Hook { .. } => "hook",
        Request::Event { .. } => "event",
        Request::Service { .. } => "service",
        Request::ServiceResult { .. } => "service_result",
        Request::Format { .. } => "format",
        Request::Resource { .. } => "resource",
        _ => "update",
    };
    let execution = slot.instance.execution_cost();
    let diagnostics = &mut slot.diagnostics;
    diagnostics.calls = diagnostics.calls.saturating_add(1);
    diagnostics.failures = diagnostics
        .failures
        .saturating_add(u64::from(result.is_err()));
    diagnostics.total_microseconds = diagnostics.total_microseconds.saturating_add(elapsed);
    diagnostics.maximum_microseconds = diagnostics.maximum_microseconds.max(elapsed);
    if diagnostics.recent.len() >= 64 {
        diagnostics.recent.remove(0);
    }
    diagnostics.recent.push(CallbackCost {
        callback: callback.into(),
        microseconds: elapsed,
        failed: result.is_err(),
        execution,
    });
    result
}

fn validate_response(slot: &Slot, request: &Request, response: &Response) -> Result<(), String> {
    if crate::limits::serialized_size(response, MAX_MESSAGE_BYTES).is_none()
        || response.contributions.len() > MAX_CONTRIBUTIONS
        || response.effects.len() > MAX_EFFECTS
    {
        return Err("Response exceeds plugin limits".into());
    }
    let activating = matches!(request, Request::Activate { .. });
    if !activating && !response.contributions.is_empty() {
        return Err("Contributions can only be registered during activation".into());
    }
    if !matches!(request, Request::Hook { .. }) && response.hook.is_some() {
        return Err("Hook decisions are only valid in hook callbacks".into());
    }
    hooks::validate_response(request, response)?;
    let contributions = if activating {
        &response.contributions
    } else {
        &slot.contributions
    };
    if response
        .effects
        .iter()
        .filter(|e| matches!(e, Effect::Complete { .. } | Effect::Fail { .. }))
        .count()
        > 1
    {
        return Err("A provider callback can finish its operation only once".into());
    }
    if response
        .effects
        .iter()
        .filter(|e| matches!(e, Effect::Edit { .. }))
        .count()
        > 1
    {
        return Err("A callback must group its document edits in one transaction".into());
    }
    let mut ids = BTreeSet::new();
    if response
        .effects
        .iter()
        .filter(|e| matches!(e, Effect::Draw { .. } | Effect::PreviewEffects { .. }))
        .count()
        > 1
    {
        return Err("A render callback must return at most one drawing/effect description".into());
    }
    for contribution in contributions {
        if !valid_id(contribution.id()) {
            return Err("Invalid contribution ID".into());
        }
        let tag = std::mem::discriminant(contribution);
        if !ids.insert((format!("{tag:?}"), contribution.id())) {
            return Err("Duplicate contribution".into());
        }
        if let Some(capability) = contribution.capability() {
            grant(slot, capability)?;
        }
        if let Contribution::TimelineTrack { id, title, width } = contribution
            && (title.trim().is_empty()
                || title.len() > 256
                || !width.is_finite()
                || !(80. ..=400.).contains(width)
                || contributions
                    .iter()
                    .any(|c| matches!(c,Contribution::Track {id:other,..} if id==other)))
        {
            return Err(
                "Timeline tracks need a title, unique track ID and width of 80–400 points".into(),
            );
        }
        if let Contribution::ContextMenu { title, command, .. } = contribution
            && (title.trim().is_empty()
                || title.len() > 256
                || !contributions
                    .iter()
                    .any(|c| matches!(c,Contribution::Command {id,..} if id==command)))
        {
            return Err("Context entries need a title and an owned command".into());
        }
        if let Contribution::Menu { title, panel, .. } = contribution
            && (title.trim().is_empty()
                || title.len() > 256
                || !valid_id(panel)
                || !contributions.iter().any(|c| {
                    matches!(c,
                    Contribution::Panel {id,location:PanelLocation::Window,..} if id==panel)
                }))
        {
            return Err(
                "Menu entries need a title and a window panel owned by the same plugin".into(),
            );
        }
        if let Contribution::Format {
            extensions,
            decode,
            encode,
            ..
        } = contribution
        {
            if !decode && !encode {
                return Err("Format provider has no enabled direction".into());
            }
            validate_extensions(extensions)?;
        }
        if let Contribution::Resource { extensions, .. } = contribution {
            validate_extensions(extensions)?;
        }
        if let Contribution::Tool {
            surfaces, buttons, ..
        } = contribution
            && (surfaces.is_empty()
                || buttons.is_empty()
                || surfaces.len() > 9
                || buttons.len() > 5
                || surfaces.iter().collect::<BTreeSet<_>>().len() != surfaces.len()
                || buttons.iter().collect::<BTreeSet<_>>().len() != buttons.len())
        {
            return Err("Tools need distinct nonempty input surfaces and buttons".into());
        }
    }
    for effect in &response.effects {
        match effect {
            Effect::Complete {
                operation,
                file,
                name,
            } => {
                provider_completion(slot, request)?;
                if *operation == 0 || *file != 2 || !valid_file_name(name) {
                    return Err("Invalid provider completion".into());
                }
            }
            Effect::Fail { operation, message } => {
                provider_completion(slot, request)?;
                if *operation == 0 || message.len() > 16_384 {
                    return Err("Invalid provider failure".into());
                }
            }
            Effect::Edit { transaction } => {
                grant(slot, Capability::DocumentEdit)?;
                phistudio_sdk::validate_plugin_data_owner(
                    &transaction.operations,
                    &slot.manifest.id,
                )?;
                if matches!(
                    request,
                    Request::Hook { .. }
                        | Request::Render { .. }
                        | Request::Format { .. }
                        | Request::Resource { .. }
                        | Request::Activate { .. }
                        | Request::Deactivate
                        | Request::SettingsChanged { .. }
                ) {
                    return Err("Document edits need a versioned action or event context".into());
                }
                if matches!(request,Request::Input {input,..} if input.phase()!=Some(InputPhase::End))
                {
                    return Err("Only a completed input gesture can edit the document".into());
                }
                if request
                    .context()
                    .is_none_or(|c| c.version != transaction.version)
                {
                    return Err("Transaction does not match its request's document version".into());
                }
                if transaction.operations.len() > MAX_PATCH_OPERATIONS {
                    return Err("Transaction exceeds operation limit".into());
                }
            }
            Effect::Panel { id, widgets } => {
                if crate::validation::has_password(widgets) {
                    grant(slot, Capability::Secrets)?;
                }
                if !contributions
                    .iter()
                    .any(|c| matches!(c, Contribution::Panel { id: panel, .. } if panel == id))
                {
                    return Err("Panel is not owned by this plugin".into());
                }
                crate::validation::widgets(widgets)?;
            }
            Effect::Dialog {
                panel,
                open,
                version,
            } => {
                grant(slot, Capability::Ui)?;
                if *open && request.context().is_some_and(|c| c.version != *version) {
                    return Err("Dialog must match the current request's document version".into());
                }
                if !contributions.iter().any(|c|matches!(c,Contribution::Panel {id,location:PanelLocation::Window,..} if id==panel)) {
                    return Err("Dialogs require an owned window panel".into());
                }
            }
            Effect::Draw { frame, layers } => {
                let Request::Render { frame: request, .. } = request else {
                    return Err("Draw effects require a render callback".into());
                };
                if *frame != request.token || request.target.kind == RenderKind::PreviewEffect {
                    return Err("Drawing belongs to another render frame".into());
                }
                crate::validation::layers(layers, request.viewport.timeline.is_some())?;
            }
            Effect::PreviewEffects { frame, passes } => {
                let Request::Render { frame: request, .. } = request else {
                    return Err("Preview effects require their render callback".into());
                };
                if *frame != request.token || request.target.kind != RenderKind::PreviewEffect {
                    return Err("Preview effects belong to another render frame or target".into());
                }
                validate_preview_passes(passes)?;
                for pass in passes {
                    if let PreviewShader::Preset { name } = &pass.shader
                        && !phistudio_core::shaders::PRESETS.contains(&name.as_str())
                    {
                        return Err("Unknown preview shader preset".into());
                    }
                }
            }
            Effect::Store { .. } => grant(slot, Capability::Storage)?,
            Effect::Call { call } => {
                if !(call.service.provider == "host" && call.service.id == "features") {
                    grant(slot, Capability::Services)?;
                }
                call.validate()?;
            }
            Effect::Emit { topic, .. } => {
                if !valid_id(topic) {
                    return Err("Invalid event topic".into());
                }
            }
            Effect::Notify { message } => {
                if message.len() > 16_384 {
                    return Err("Notification is too long".into());
                }
            }
            Effect::SelectTool { id } => {
                grant(slot, Capability::Input)?;
                if id.as_ref().is_some_and(|id| {
                    !contributions
                        .iter()
                        .any(|c| matches!(c,Contribution::Tool {id:owned,..} if owned==id))
                }) {
                    return Err("Selected tool is not owned by this plugin".into());
                }
                if matches!(
                    request,
                    Request::Render { .. } | Request::Hook { .. } | Request::Deactivate
                ) {
                    return Err("Tool selection requires an action callback".into());
                }
            }
        }
    }
    Ok(())
}
fn grant(slot: &Slot, capability: Capability) -> Result<(), String> {
    if slot.granted.contains(&capability) {
        Ok(())
    } else {
        Err(format!("Capability {capability:?} was not granted"))
    }
}
fn validate_extensions(extensions: &[String]) -> Result<(), String> {
    if extensions.is_empty()
        || extensions.len() > 16
        || extensions.iter().collect::<BTreeSet<_>>().len() != extensions.len()
        || extensions.iter().any(|e| !valid_file_extension(e))
    {
        return Err("Provider extensions must be distinct safe lowercase suffixes".into());
    }
    Ok(())
}
fn provider_completion(slot: &Slot, request: &Request) -> Result<(), String> {
    if !slot.granted.contains(&Capability::Formats)
        && !slot.granted.contains(&Capability::Resources)
    {
        return Err("Provider completion requires Formats or Resources".into());
    }
    if !matches!(
        request,
        Request::Format { .. }
            | Request::Resource { .. }
            | Request::ServiceResult { .. }
            | Request::Service { .. }
    ) {
        return Err("Provider completion requires a provider callback chain".into());
    }
    Ok(())
}
