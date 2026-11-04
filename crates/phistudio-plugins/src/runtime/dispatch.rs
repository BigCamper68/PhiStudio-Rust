use super::*;
use std::{
    collections::VecDeque,
    sync::atomic::{AtomicUsize, Ordering},
};
mod buffering;
mod services;
use buffering::{Deliveries, DispatchQueue};

/// Shared across application-service round trips, so a guest cannot reset its
/// dispatch limit by repeatedly asking the application for another reply.
#[derive(Clone)]
pub(super) struct Budget {
    remaining: Arc<AtomicUsize>,
    editable: bool,
    interactive: bool,
    background: bool,
    interaction: Option<crate::Interaction>,
    intercept_services: bool,
    operation: Option<Operation>,
    invocation: Option<Invocation>,
}
impl Budget {
    pub(super) fn new(editable: bool) -> Self {
        Self {
            remaining: Arc::new(AtomicUsize::new(2048)),
            editable,
            interactive: false,
            background: false,
            interaction: None,
            intercept_services: true,
            operation: None,
            invocation: None,
        }
    }
    pub(super) fn background() -> Self {
        Self {
            background: true,
            ..Self::new(false)
        }
    }
    pub(super) fn interactive() -> Self {
        Self {
            interactive: true,
            ..Self::new(true)
        }
    }
    pub(super) fn long_running() -> Self {
        Self {
            remaining: Arc::new(AtomicUsize::new(262_144)),
            background: true,
            interactive: true,
            ..Self::new(true)
        }
    }
    pub(super) fn provider(operation: Operation) -> Self {
        Self {
            remaining: Arc::new(AtomicUsize::new(16_384)),
            operation: Some(operation),
            ..Self::new(false)
        }
    }
    pub(super) fn hook(invocation: Invocation) -> Self {
        Self {
            invocation: Some(invocation),
            ..Self::new(false).interceptor()
        }
    }
    pub(super) fn input(interaction: crate::Interaction, editable: bool) -> Self {
        Self {
            interaction: Some(interaction),
            interactive: editable,
            background: !editable,
            ..Self::new(editable)
        }
    }
    pub(super) fn interceptor(&self) -> Self {
        Self {
            editable: false,
            interactive: false,
            intercept_services: false,
            ..self.clone()
        }
    }
    pub(super) fn take(&self) -> Result<(), String> {
        let mut remaining = self.remaining.load(Ordering::Relaxed);
        loop {
            let next = remaining
                .checked_sub(1)
                .ok_or_else(|| "Plugin cascade exceeds its dispatch limit".to_owned())?;
            match self.remaining.compare_exchange_weak(
                remaining,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(()),
                Err(current) => remaining = current,
            }
        }
    }
}

/// An application-service request with an unforgeable runtime continuation.
/// The application resolves it and submits Action::ServiceReply with this value.
#[derive(Clone)]
pub struct HostCall {
    pub origin: Origin,
    pub required: Vec<Origin>,
    pub request: ServiceCall,
    pub context: EditorContext,
    pub(super) reply_service: ServiceRef,
    pub(super) budget: Budget,
}
impl HostCall {
    pub fn invocation(&self) -> Option<&Invocation> {
        self.budget.invocation.as_ref()
    }
    pub fn operation(&self) -> Option<&Operation> {
        self.budget.operation.as_ref()
    }
    pub fn interaction(&self) -> Option<&crate::Interaction> {
        self.budget.interaction.as_ref()
    }
    pub fn is_background(&self) -> bool {
        self.budget.background
    }
    pub(crate) fn budget_editable(&self) -> bool {
        self.budget.editable
    }
    /// Return metadata without granting another native action or document edit.
    pub fn readonly_reply(mut self) -> Self {
        self.budget = self.budget.interceptor();
        self
    }
    /// A completed native action can invalidate its gesture (for example by
    /// changing tools). Its acknowledgement keeps activation scopes and limits,
    /// but cannot perform a second action from the same gesture.
    pub fn after_editor_action(mut self) -> Self {
        if self.budget.interaction.take().is_some() {
            self.budget = self.budget.interceptor();
        }
        self
    }
}

enum Dispatch {
    Effect(OwnedEffect),
    Reply {
        recipient: Origin,
        required: Vec<Origin>,
        reply: ServiceReply,
    },
}

impl Worker {
    pub(super) fn finish(
        &mut self,
        output: &mut Output,
        context: &EditorContext,
        registry: &Mutex<Registry>,
        job: u64,
        budget: Budget,
    ) -> Result<(), String> {
        let mut queue = DispatchQueue::default();
        for effect in std::mem::take(&mut output.effects) {
            queue.push_back(Dispatch::Effect(effect))?;
        }
        let mut deliveries = Deliveries::default();
        while let Some(dispatch) = queue.pop_front() {
            if budget.invocation.as_ref().is_some_and(|i| !i.is_active()) {
                break;
            }
            if budget.interaction.as_ref().is_some_and(|i| !i.is_active()) {
                break;
            }
            if budget.operation.as_ref().is_some_and(|o| !o.is_current()) {
                break;
            }
            budget.take()?;
            let mut owned = match dispatch {
                Dispatch::Reply {
                    recipient,
                    required,
                    reply,
                } => {
                    if !current(&self.host, registry, &recipient, &required, job)? {
                        continue;
                    }
                    match self.host.service_result(&recipient, reply, context.clone()) {
                        Ok(result) => {
                            for mut effect in result.effects.into_iter().rev() {
                                inherit(&mut effect, &required, None);
                                queue.push_front(Dispatch::Effect(effect))?;
                            }
                        }
                        Err(error) => self.diagnostics.push(error.to_string()),
                    }
                    continue;
                }
                Dispatch::Effect(owned) => owned,
            };
            owned.interaction = budget.interaction.clone();
            owned.operation = budget.operation.clone();
            owned.invocation = budget.invocation.clone();
            if !current(&self.host, registry, &owned.origin, &owned.required, job)? {
                continue;
            }
            match &owned.effect {
                Effect::Dialog { open: true, .. } if !budget.interactive => continue,
                Effect::Complete {
                    operation,
                    file,
                    name,
                } => {
                    let scope = budget
                        .operation
                        .as_ref()
                        .filter(|o| o.id() == *operation)
                        .ok_or("Provider completion does not match an active operation")?;
                    if !budget.intercept_services {
                        return Err("Hooks cannot complete provider operations".into());
                    }
                    output.artifact =
                        Some(self.complete_provider(scope, &owned, *file, name.clone())?);
                    break;
                }
                Effect::Fail { operation, message } => {
                    let scope = budget
                        .operation
                        .as_ref()
                        .filter(|o| o.id() == *operation)
                        .ok_or("Provider failure does not match an active operation")?;
                    if !budget.intercept_services {
                        return Err("Hooks cannot complete provider operations".into());
                    }
                    self.fail_provider(scope, &owned)?;
                    output.cancelled = Some(message.clone());
                    break;
                }
                Effect::Edit { .. } if !budget.editable => {
                    return Err("Document edits are not allowed in this callback chain".into());
                }
                Effect::SelectTool { .. } if !budget.editable => {
                    return Err("Tool selection is not allowed in this callback chain".into());
                }
                Effect::Store { value } => {
                    self.host
                        .store(&owned.origin, value.clone())
                        .map_err(|e| e.to_string())?;
                    if let Some(root) = &self.root {
                        preferences::write(
                            &root
                                .join("plugins-data")
                                .join(format!("{}.json", owned.origin.plugin)),
                            value,
                        )?;
                    }
                }
                Effect::Emit { topic, payload } => {
                    let topic = format!("{}.{topic}", owned.origin.plugin);
                    let (emitted, errors) =
                        self.host.event(&topic, payload.clone(), context.clone());
                    for mut effect in emitted {
                        inherit(&mut effect, &owned.required, Some(&owned.origin));
                        queue.push_back(Dispatch::Effect(effect))?;
                    }
                    self.diagnostics
                        .extend(errors.into_iter().map(|e| e.to_string()));
                }
                Effect::Call { call } => {
                    let original = call.clone();
                    let routed = self.intercept_service(
                        &mut owned, &original, context, registry, job, &budget,
                    )?;
                    for effect in routed.effects {
                        deliveries.effect(effect)?;
                    }
                    for call in routed.calls {
                        deliveries.call(call)?;
                    }
                    if !current(&self.host, registry, &owned.origin, &owned.required, job)? {
                        continue;
                    }
                    if let Some(outcome) = routed.cancelled {
                        queue.push_front(Dispatch::Reply {
                            recipient: owned.origin,
                            required: owned.required,
                            reply: ServiceReply {
                                request_id: original.request_id,
                                service: original.service,
                                outcome,
                            },
                        })?;
                        continue;
                    }
                    let call = routed.call;
                    if call.service.provider == "host"
                        && matches!(
                            call.service.id.as_str(),
                            "network" | "websocket" | "secrets"
                        )
                    {
                        let mut metadata = context.clone();
                        metadata.chart = None;
                        let io_call = HostCall {
                            origin: owned.origin.clone(),
                            required: owned.required.clone(),
                            request: call,
                            reply_service: original.service.clone(),
                            context: metadata,
                            budget: budget.clone(),
                        };
                        if let Some(outcome) = self.io_service(io_call) {
                            queue.push_front(Dispatch::Reply {
                                recipient: owned.origin,
                                required: owned.required,
                                reply: ServiceReply {
                                    request_id: original.request_id,
                                    service: original.service,
                                    outcome,
                                },
                            })?;
                        }
                        continue;
                    }
                    if call.service.provider == "host"
                        && matches!(call.service.id.as_str(), "features" | "diagnostics")
                    {
                        let outcome = self.feature_service(&owned.origin, &call);
                        queue.push_front(Dispatch::Reply {
                            recipient: owned.origin,
                            required: owned.required,
                            reply: ServiceReply {
                                request_id: original.request_id,
                                service: original.service,
                                outcome,
                            },
                        })?;
                        continue;
                    }
                    if call.service.provider == "host" && call.service.id == "assets" {
                        let asset_call = HostCall {
                            origin: owned.origin.clone(),
                            required: owned.required.clone(),
                            request: call,
                            reply_service: original.service.clone(),
                            context: context.clone(),
                            budget: budget.clone(),
                        };
                        let outcome = self.asset_service(&asset_call, registry, job);
                        queue.push_front(Dispatch::Reply {
                            recipient: owned.origin,
                            required: owned.required,
                            reply: ServiceReply {
                                request_id: original.request_id,
                                service: original.service,
                                outcome,
                            },
                        })?;
                        continue;
                    }
                    if call.service.provider == "host" && call.service.id == "files" {
                        let outcome = self.provider_io(
                            &owned.origin,
                            &owned.required,
                            &call,
                            budget.operation.as_ref(),
                        );
                        queue.push_front(Dispatch::Reply {
                            recipient: owned.origin,
                            required: owned.required,
                            reply: ServiceReply {
                                request_id: original.request_id,
                                service: original.service,
                                outcome,
                            },
                        })?;
                        continue;
                    }
                    if call.service.provider == "host" {
                        let mut context = context.clone();
                        context.chart = None;
                        deliveries.call(HostCall {
                            origin: owned.origin,
                            required: owned.required,
                            request: call,
                            reply_service: original.service,
                            context,
                            budget: budget.clone(),
                        })?;
                        continue;
                    }
                    let provider = &call.service.provider;
                    let revoked = registry
                        .lock()
                        .map_err(|_| "Plugin registry interrupted")?
                        .revoked
                        .get(provider)
                        .is_some_and(|id| *id > job);
                    let result = if revoked {
                        Err("Service provider was disabled or reloaded".into())
                    } else {
                        self.host
                            .service(
                                routed
                                    .principal
                                    .as_ref()
                                    .map_or(&owned.origin.plugin, |p| &p.plugin),
                                provider,
                                &call.service.version,
                                Request::Service {
                                    id: call.service.id.clone(),
                                    method: call.method.clone(),
                                    arguments: call.arguments.clone(),
                                    context: context.clone(),
                                },
                            )
                            .map_err(|e| e.to_string())
                    };
                    let (provider_effects, outcome) = match result {
                        Ok(result) => (
                            result.effects,
                            ServiceOutcome::Success {
                                value: result.result,
                            },
                        ),
                        Err(error) => (vec![], ServiceOutcome::error("service_unavailable", error)),
                    };
                    queue.push_front(Dispatch::Reply {
                        recipient: owned.origin.clone(),
                        required: owned.required.clone(),
                        reply: ServiceReply {
                            request_id: original.request_id,
                            service: original.service,
                            outcome,
                        },
                    })?;
                    for mut effect in provider_effects.into_iter().rev() {
                        inherit(&mut effect, &owned.required, Some(&owned.origin));
                        queue.push_front(Dispatch::Effect(effect))?;
                    }
                }
                _ => deliveries.effect(owned)?,
            }
        }
        // A failed callback can disable owners already encountered in the queue.
        deliveries.effects.retain(|e| {
            self.host.is_current(&e.origin)
                && e.required.iter().all(|p| self.host.is_current(p))
                && e.interaction.as_ref().is_none_or(|i| i.is_active())
                && e.operation.as_ref().is_none_or(|o| o.is_current())
        });
        deliveries.calls.retain(|c| {
            self.host.is_current(&c.origin)
                && c.required.iter().all(|p| self.host.is_current(p))
                && c.interaction().is_none_or(|i| i.is_active())
                && c.operation().is_none_or(|o| o.is_running())
        });
        output.effects = deliveries.effects;
        output.calls = deliveries.calls;
        Ok(())
    }
}

fn inherit(effect: &mut OwnedEffect, required: &[Origin], caller: Option<&Origin>) {
    for origin in required.iter().chain(caller) {
        if origin != &effect.origin && !effect.required.contains(origin) {
            effect.required.push(origin.clone());
        }
    }
}

pub(super) fn current(
    host: &Host,
    registry: &Mutex<Registry>,
    origin: &Origin,
    required: &[Origin],
    job: u64,
) -> Result<bool, String> {
    let registry = registry.lock().map_err(|_| "Plugin registry interrupted")?;
    Ok(std::iter::once(origin).chain(required).all(|p| {
        host.is_current(p) && !registry.revoked.get(&p.plugin).is_some_and(|id| *id > job)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_long_work_retains_a_shared_bound_and_default_limits() {
        let normal = Budget::new(true);
        for _ in 0..2048 {
            normal.take().unwrap()
        }
        assert!(normal.take().is_err());
        let long = Budget::long_running();
        assert!(long.background && long.editable);
        let continuation = long.clone();
        for _ in 0..262_143 {
            continuation.take().unwrap()
        }
        long.take().unwrap();
        assert!(continuation.take().is_err());
        let hook = long.interceptor();
        assert!(!hook.editable && !hook.intercept_services && hook.take().is_err());
    }
}
