use crate::{HostCall, Runtime};
use phistudio_sdk::{Capability, DocumentVersion, EditorAction, Version};

impl Runtime {
    /// Common authorization for read-only, document-bound host inspection.
    pub fn authorize_inspection(
        &self,
        call: &HostCall,
        version: DocumentVersion,
        service: &str,
        capability: Capability,
    ) -> Result<(), String> {
        if !self.is_call_current(call) || call.context.version != version {
            return Err("Inspection caller or document changed".into());
        }
        let registry = self
            .registry
            .lock()
            .map_err(|_| "Plugin registry interrupted")?;
        let plugin = registry
            .current(&call.origin)
            .ok_or("Inspection caller is inactive")?;
        if call.request.service.provider != "host"
            || call.request.service.id != service
            || !call.request.service.version.matches(&Version::new(1, 0, 0))
        {
            return Err("Requested inspection service is unavailable".into());
        }
        if [Capability::Services, Capability::DocumentRead, capability]
            .iter()
            .any(|c| !plugin.granted.contains(c))
        {
            return Err("Inspection capabilities were not granted".into());
        }
        Ok(())
    }

    /// Validate again immediately before handing a native action to the editor.
    /// Consuming an input continuation reserves its single gesture commit.
    pub fn authorize_editor_service(
        &self,
        call: &HostCall,
        version: DocumentVersion,
        action: Option<&EditorAction>,
        consume: bool,
    ) -> Result<(), String> {
        if !self.is_call_current(call) {
            return Err("Editor service caller is no longer active".into());
        }
        let registry = self
            .registry
            .lock()
            .map_err(|_| "Plugin registry interrupted")?;
        if !registry.valid_scope(&call.origin, &call.required)
            || call.invocation().is_some_and(|i| !i.is_active())
            || call
                .operation()
                .is_some_and(|o| !o.current_in(&registry, true))
        {
            return Err("Editor service continuation was revoked".into());
        }
        let caller = registry
            .current(&call.origin)
            .ok_or("Editor service caller is inactive")?;
        if call.request.service.provider != "host"
            || call.request.service.id != "editor"
            || !call.request.service.version.matches(&Version::new(1, 0, 0))
        {
            return Err("Unknown editor service".into());
        }
        if call.context.version != version {
            return Err("Editor service document version changed".into());
        }
        if !caller.granted.contains(&Capability::Services) {
            return Err("Editor service requires the services capability".into());
        }
        let Some(action) = action else { return Ok(()) };
        action.validate()?;
        if !call.budget_editable() || !caller.granted.contains(&Capability::EditorControl) {
            return Err("Editor actions require editor_control in an editable callback".into());
        }
        if action.edits_document() && !caller.granted.contains(&Capability::DocumentEdit) {
            return Err("This editor action requires document_edit".into());
        }
        if matches!(action, EditorAction::Copy)
            && !caller.granted.contains(&Capability::DocumentRead)
        {
            return Err("Copy requires document_read".into());
        }
        if let Some(interaction) = call.interaction() {
            if interaction.version() != version
                || (interaction.origin() != &call.origin
                    && !call.required.contains(interaction.origin()))
            {
                return Err("Editor action does not match its input gesture".into());
            }
            let mut state = interaction.lock()?;
            if !state.ended || state.cancelled || state.committed {
                return Err("Input gesture cannot perform another editor action".into());
            }
            if consume {
                state.committed = true;
            }
        }
        Ok(())
    }
}
