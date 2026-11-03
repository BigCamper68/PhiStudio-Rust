use crate::{Host, HostError, Origin};
use phistudio_core::Editor;
use phistudio_sdk::{Capability, Transaction};

impl Host {
    /// Recheck ownership at application time, not just when work was produced.
    pub fn apply_transaction(
        &self,
        origin: &Origin,
        session: u64,
        editor: &mut Editor,
        transaction: Transaction,
    ) -> Result<bool, HostError> {
        let fail = |message: String| HostError {
            plugin: origin.plugin.clone(),
            message,
        };
        if !self.is_current(origin) {
            return Err(fail("Plugin activation is no longer current".into()));
        }
        if !self.granted(&origin.plugin, Capability::DocumentEdit) {
            return Err(fail("Document editing was not granted".into()));
        }
        if transaction.version.session != session {
            return Err(fail(
                "Plugin action belongs to another project session".into(),
            ));
        }
        phistudio_sdk::validate_plugin_data_owner(&transaction.operations, &origin.plugin)
            .map_err(fail)?;
        editor
            .apply_patch(
                transaction.version.revision,
                transaction.label,
                transaction.operations,
            )
            .map_err(|e| fail(e.to_string()))
    }
}
