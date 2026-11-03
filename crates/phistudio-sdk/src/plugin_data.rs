//! Versioned project data, independent of an installed plugin's lifetime.
use crate::{DocumentVersion, valid_id};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PLUGIN_DATA_KEY: &str = "phiStudioPlugins";
pub const PLUGIN_DATA_FORMAT: &str = "phistudio.plugin-data/1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginDataFormat {
    #[serde(rename = "phistudio.plugin-data/1")]
    V1,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginDataPolicy {
    #[default]
    Runtime,
    EditorOnly,
    RequiresConversion,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginData {
    pub format: PluginDataFormat,
    /// The plugin's own data schema version, not its package/API version.
    pub version: u32,
    #[serde(default)]
    pub export: PluginDataPolicy,
    pub data: Value,
}
impl PluginData {
    pub fn new(version: u32, export: PluginDataPolicy, data: Value) -> Self {
        Self {
            format: PluginDataFormat::V1,
            version,
            export,
            data,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version == 0 {
            return Err("Plugin data schema versions start at 1".into());
        }
        Ok(())
    }
}

/// Only recognized envelopes participate in automatic runtime filtering.
/// Unknown/legacy/future entries remain opaque; no subtree is cloned here.
pub fn plugin_data_policy(value: &Value) -> Option<PluginDataPolicy> {
    let entry = value.as_object()?;
    if entry
        .keys()
        .any(|k| !matches!(k.as_str(), "format" | "version" | "export" | "data"))
        || entry.get("format")?.as_str()? != PLUGIN_DATA_FORMAT
        || !entry.contains_key("data")
    {
        return None;
    }
    let version = entry.get("version")?.as_u64()?;
    if version == 0 || version > u64::from(u32::MAX) {
        return None;
    }
    match entry.get("export").map(Value::as_str) {
        None | Some(Some("runtime")) => Some(PluginDataPolicy::Runtime),
        Some(Some("editor_only")) => Some(PluginDataPolicy::EditorOnly),
        Some(Some("requires_conversion")) => Some(PluginDataPolicy::RequiresConversion),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginDataExport {
    #[default]
    Preserve,
    Runtime,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginDataQuery {
    pub version: DocumentVersion,
    pub plugin: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginDataValue {
    pub version: DocumentVersion,
    /// Distinguishes a missing entry from an imported explicit null.
    pub present: bool,
    pub entry: Value,
}
pub fn plugin_data_path(plugin: &str) -> Result<String, String> {
    if !valid_id(plugin) {
        return Err("Invalid plugin data owner".into());
    }
    Ok(format!("/{PLUGIN_DATA_KEY}/{plugin}"))
}
pub fn validate_plugin_data_owner(
    operations: &[crate::PatchOperation],
    owner: &str,
) -> Result<(), String> {
    for operation in operations {
        if let crate::PatchOperation::PluginData { plugin, entry } = operation {
            if plugin != owner || !valid_id(plugin) {
                return Err("Plugin data operation must name its producing plugin".into());
            }
            if let Some(entry) = entry {
                entry.validate()?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PatchOperation;
    use serde_json::json;

    #[test]
    fn recognized_envelopes_have_explicit_versions_and_unknown_formats_stay_opaque() {
        let mut value = serde_json::to_value(PluginData::new(
            4,
            PluginDataPolicy::EditorOnly,
            json!({"unknown":[null,42]}),
        ))
        .unwrap();
        assert_eq!(
            plugin_data_policy(&value),
            Some(PluginDataPolicy::EditorOnly)
        );
        let restored: PluginData = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(restored.version, 4);
        assert_eq!(restored.data["unknown"][0], Value::Null);
        for (key, replacement) in [
            ("format", json!("phistudio.plugin-data/2")),
            ("version", json!(0)),
            ("version", json!(u64::MAX)),
            ("export", json!("future")),
            ("unknown", json!(true)),
        ] {
            let mut changed = value.clone();
            changed[key] = replacement;
            assert_eq!(plugin_data_policy(&changed), None);
        }
        value.as_object_mut().unwrap().remove("data");
        assert_eq!(plugin_data_policy(&value), None);
    }

    #[test]
    fn typed_operations_validate_ownership_and_keep_the_wire_schema_explicit() {
        let entry = PluginData::new(1, PluginDataPolicy::Runtime, Value::Null);
        let operation = PatchOperation::PluginData {
            plugin: "example.owner".into(),
            entry: Some(entry),
        };
        validate_plugin_data_owner(std::slice::from_ref(&operation), "example.owner").unwrap();
        assert!(
            validate_plugin_data_owner(std::slice::from_ref(&operation), "example.other").is_err()
        );
        let value = serde_json::to_value(operation).unwrap();
        assert_eq!(value["op"], "plugin_data");
        assert_eq!(value["entry"]["format"], PLUGIN_DATA_FORMAT);
        assert_eq!(value["entry"]["data"], Value::Null);
        assert_eq!(
            plugin_data_path("example.owner").unwrap(),
            "/phiStudioPlugins/example.owner"
        );
        assert!(plugin_data_path("bad/owner").is_err());
    }
}
