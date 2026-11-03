//! Project-persistent extension data and explicit export-copy policies.
use crate::ChartError;
use phistudio_sdk::{
    PLUGIN_DATA_KEY, PatchOperation, PluginData, PluginDataExport, PluginDataPolicy,
    plugin_data_path, plugin_data_policy,
};
use serde_json::{Value, json};
use std::borrow::Cow;

fn invalid(message: impl Into<String>) -> ChartError {
    ChartError::Invalid(message.into())
}

pub(crate) fn operations(
    root: &Value,
    plugin: &str,
    entry: Option<PluginData>,
) -> Result<Vec<PatchOperation>, ChartError> {
    let path = plugin_data_path(plugin).map_err(invalid)?;
    if let Some(entry) = &entry {
        entry.validate().map_err(invalid)?;
    }
    let mut operations = vec![];
    let existing = match root.get(PLUGIN_DATA_KEY) {
        None if entry.is_none() => return Ok(operations),
        None => {
            operations.push(PatchOperation::Add {
                path: format!("/{PLUGIN_DATA_KEY}"),
                value: json!({}),
            });
            None
        }
        Some(Value::Object(entries)) => Some(entries),
        Some(_) => {
            return Err(invalid(
                "Existing plugin data container is not an object; preserve or migrate it explicitly",
            ));
        }
    };
    if let Some(entry) = entry {
        operations.push(PatchOperation::Add {
            path,
            value: serde_json::to_value(entry)?,
        });
    } else if let Some(existing) = existing.filter(|entries| entries.contains_key(plugin)) {
        operations.push(PatchOperation::Remove { path });
        if existing.len() == 1 {
            operations.push(PatchOperation::Remove {
                path: format!("/{PLUGIN_DATA_KEY}"),
            });
        }
    }
    Ok(operations)
}

/// Filters only the managed root namespace. Source trees are never modified.
/// Runtime mode preserves unrecognized entries instead of guessing their schema.
pub fn export_value(root: &Value, mode: PluginDataExport) -> Result<Cow<'_, Value>, ChartError> {
    if mode == PluginDataExport::Preserve || root.get(PLUGIN_DATA_KEY).is_none() {
        return Ok(Cow::Borrowed(root));
    }
    if mode == PluginDataExport::Runtime {
        let Some(entries) = root[PLUGIN_DATA_KEY].as_object() else {
            return Ok(Cow::Borrowed(root));
        };
        let mut remove = false;
        for (plugin, entry) in entries {
            if !phistudio_sdk::valid_id(plugin) {
                continue;
            }
            match plugin_data_policy(entry) {
                Some(PluginDataPolicy::EditorOnly) => remove = true,
                Some(PluginDataPolicy::RequiresConversion) => {
                    return Err(invalid(format!(
                        "Plugin data from {plugin} requires conversion before runtime export"
                    )));
                }
                _ => {}
            }
        }
        if !remove {
            return Ok(Cow::Borrowed(root));
        }
    }
    let mut filtered = root.clone();
    let object = filtered
        .as_object_mut()
        .ok_or_else(|| invalid("Chart root must be an object"))?;
    match mode {
        PluginDataExport::Preserve => return Ok(Cow::Borrowed(root)),
        PluginDataExport::Remove => {
            object.shift_remove(PLUGIN_DATA_KEY);
        }
        PluginDataExport::Runtime => {
            let entries = object
                .get_mut(PLUGIN_DATA_KEY)
                .and_then(Value::as_object_mut)
                .ok_or_else(|| invalid("Plugin data container changed"))?;
            entries.retain(|plugin, entry| {
                !phistudio_sdk::valid_id(plugin)
                    || plugin_data_policy(entry) != Some(PluginDataPolicy::EditorOnly)
            });
            if entries.is_empty() {
                object.shift_remove(PLUGIN_DATA_KEY);
            }
        }
    }
    Ok(Cow::Owned(filtered))
}
