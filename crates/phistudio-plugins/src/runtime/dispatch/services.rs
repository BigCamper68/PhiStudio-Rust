use super::*;

fn private_service(call: &ServiceCall) -> bool {
    call.service.provider == "host"
        && matches!(
            call.service.id.as_str(),
            "network" | "websocket" | "secrets" | "features" | "diagnostics"
        )
}

pub(super) struct InterceptedService {
    pub call: ServiceCall,
    pub principal: Option<Origin>,
    pub cancelled: Option<ServiceOutcome>,
    pub effects: Vec<OwnedEffect>,
    pub calls: Vec<HostCall>,
}

impl Worker {
    pub(super) fn intercept_service(
        &mut self,
        owned: &mut OwnedEffect,
        call: &ServiceCall,
        context: &EditorContext,
        registry: &Mutex<Registry>,
        job: u64,
        budget: &Budget,
    ) -> Result<InterceptedService, String> {
        let mut routed = InterceptedService {
            call: call.clone(),
            principal: None,
            cancelled: None,
            effects: vec![],
            calls: vec![],
        };
        if private_service(call) || !budget.intercept_services || !self.host.contributions().any(|(_, c)| {
            matches!(c, Contribution::Hook { point, .. } if point == SERVICE_BEFORE_HOOK)
        }) {
            return Ok(routed);
        }
        budget.take()?;
        let input = ServiceIntercept {
            consumer: owned.origin.plugin.clone(),
            call: call.clone(),
        };
        let metadata = EditorContext {
            version: context.version,
            line: context.line,
            beat: context.beat,
            playing: context.playing,
            ..Default::default()
        };
        let result = self.host.hook_checked(
            SERVICE_BEFORE_HOOK,
            serde_json::to_value(input).map_err(|e| e.to_string())?,
            metadata.clone(),
            |origin| {
                registry
                    .lock()
                    .is_ok_and(|r| !r.revoked.contains_key(&origin.plugin))
            },
        );
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                self.diagnostics.push(error.to_string());
                routed.cancelled = Some(ServiceOutcome::error("hook_failed", error.to_string()));
                return Ok(routed);
            }
        };
        for origin in &result.required {
            if *origin != owned.origin && !owned.required.contains(origin) {
                owned.required.push(origin.clone());
            }
        }
        let mut side = Output {
            effects: result.effects,
            ..Default::default()
        };
        for effect in &mut side.effects {
            inherit(effect, &owned.required, Some(&owned.origin));
        }
        if let Err(error) = self.finish(&mut side, &metadata, registry, job, budget.interceptor()) {
            self.diagnostics.push(error.clone());
            routed.cancelled = Some(ServiceOutcome::error("hook_failed", error));
            return Ok(routed);
        }
        routed.effects = side.effects;
        routed.calls = side.calls;
        if let Some(reason) = result.cancelled {
            routed.cancelled = Some(ServiceOutcome::error("hook_cancelled", reason));
        } else {
            let replacement: ServiceIntercept = serde_json::from_value(result.payload)
                .map_err(|e| format!("Invalid intercepted service: {e}"))?;
            if private_service(&replacement.call) {
                routed.cancelled = Some(ServiceOutcome::error(
                    "private_service",
                    "Interceptors cannot redirect calls into private host services",
                ));
                return Ok(routed);
            }
            if replacement.call.service.provider != call.service.provider {
                routed.principal = result.replacement;
            }
            routed.call = replacement.call;
        }
        Ok(routed)
    }
}
