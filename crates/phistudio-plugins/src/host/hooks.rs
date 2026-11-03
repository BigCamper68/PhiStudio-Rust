use super::*;

impl Host {
    /// Ascending priority, then plugin ID. Replacements feed the next hook;
    /// cancellation or failure ends the chain. Results retain every participant.
    pub fn hook(
        &mut self,
        point: &str,
        payload: Value,
        context: EditorContext,
    ) -> Result<HookResult, HostError> {
        self.hook_checked(point, payload, context, |_| true)
    }
    pub(crate) fn hook_checked(
        &mut self,
        point: &str,
        payload: Value,
        context: EditorContext,
        active: impl Fn(&Origin) -> bool,
    ) -> Result<HookResult, HostError> {
        let mut hooks: Vec<_> = self
            .contributions()
            .filter_map(|(id, c)| match c {
                Contribution::Hook { point: p, priority } if p == point => {
                    Some((*priority, id.to_owned()))
                }
                _ => None,
            })
            .collect();
        hooks.sort();
        if hooks.len() > MAX_CONTRIBUTIONS {
            return Err(error("host", "Hook chain exceeds its callback limit"));
        }
        let mut result = HookResult {
            payload,
            cancelled: None,
            effects: vec![],
            required: vec![],
            replacement: None,
        };
        let mut effect_bytes = 0;
        for (_, id) in hooks {
            let origin = Origin {
                plugin: id.clone(),
                generation: self.slots[&id].generation,
            };
            result.required.push(origin.clone());
            for participant in &result.required {
                if !active(participant) {
                    return Err(error(&participant.plugin, "Hook activation was revoked"));
                }
            }
            let response = self.call(
                &id,
                Request::Hook {
                    point: point.into(),
                    payload: result.payload.clone(),
                    context: context.clone(),
                },
            )?;
            if result.effects.len() + response.effects.len() > MAX_EFFECTS {
                return Err(error(&id, "Hook chain exceeds its effect count limit"));
            }
            let bytes =
                crate::limits::serialized_size(&response.effects, MAX_MESSAGE_BYTES - effect_bytes)
                    .ok_or_else(|| error(&id, "Hook chain exceeds its effect byte limit"))?;
            effect_bytes += bytes;
            result
                .effects
                .extend(owned(&self.slots[&id], response.effects));
            match response.hook {
                Some(HookDecision::Cancel { reason }) => {
                    result.cancelled = Some(reason);
                    break;
                }
                Some(HookDecision::Replace { payload }) => {
                    result.payload = payload;
                    result.replacement = Some(origin);
                }
                _ => {}
            }
        }
        for participant in &result.required {
            if !active(participant) {
                return Err(error(&participant.plugin, "Hook activation was revoked"));
            }
        }
        for effect in &mut result.effects {
            effect.required.extend(
                result
                    .required
                    .iter()
                    .filter(|p| *p != &effect.origin)
                    .cloned(),
            );
        }
        Ok(result)
    }
}

pub(super) fn validate_response(request: &Request, response: &Response) -> Result<(), String> {
    if let Request::Hook { point, payload, .. } = request
        && point == SERVICE_BEFORE_HOOK
        && let Some(HookDecision::Replace {
            payload: replacement,
        }) = &response.hook
    {
        let original: ServiceIntercept = serde_json::from_value(payload.clone())
            .map_err(|e| format!("Invalid service interception: {e}"))?;
        let replacement: ServiceIntercept = serde_json::from_value(replacement.clone())
            .map_err(|e| format!("Invalid service replacement: {e}"))?;
        replacement.validate_replacement(&original)?;
    }
    if let Request::Hook { point, payload, .. } = request
        && point == FILE_BEFORE_HOOK
        && let Some(HookDecision::Replace {
            payload: replacement,
        }) = &response.hook
    {
        let original: FileIntercept = serde_json::from_value(payload.clone())
            .map_err(|e| format!("Invalid file interception: {e}"))?;
        let replacement: FileIntercept = serde_json::from_value(replacement.clone())
            .map_err(|e| format!("Invalid file replacement: {e}"))?;
        replacement.validate_replacement(&original)?;
    }
    if let Request::Hook { point, payload, .. } = request
        && point == ACTION_BEFORE_HOOK
        && let Some(HookDecision::Replace {
            payload: replacement,
        }) = &response.hook
    {
        let original: ActionIntercept = serde_json::from_value(payload.clone())
            .map_err(|e| format!("Invalid action interception: {e}"))?;
        let replacement: ActionIntercept = serde_json::from_value(replacement.clone())
            .map_err(|e| format!("Invalid action replacement: {e}"))?;
        replacement.validate_replacement(&original)?;
    }
    if let Some(HookDecision::Cancel { reason }) = &response.hook
        && reason.len() > 16_384
    {
        return Err("Hook cancellation reason is too long".into());
    }
    Ok(())
}
