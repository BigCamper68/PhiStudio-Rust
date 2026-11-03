use crate::API_VERSION;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    DocumentRead,
    ProjectRead,
    DocumentEdit,
    EditorControl,
    /// Explicit commands/UI actions can yield long scans without locking panels.
    LongRunning,
    Ui,
    Input,
    Timeline,
    Preview,
    Formats,
    Resources,
    Hooks,
    Services,
    Storage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Backend {
    Builtin,
    Wasm { entry: String },
    Native { entry: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub api: u32,
    pub id: String,
    pub name: String,
    pub version: Version,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    pub backend: Backend,
    #[serde(default)]
    pub dependencies: BTreeMap<String, VersionReq>,
    #[serde(default)]
    pub conflicts: BTreeSet<String>,
    #[serde(default)]
    pub capabilities: BTreeSet<Capability>,
    #[serde(default)]
    pub settings: BTreeMap<String, Setting>,
}

/// Human-facing text comes from the extension and need not be an editor key.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Setting {
    Bool {
        title: String,
        default: bool,
    },
    Number {
        title: String,
        default: f64,
        min: f64,
        max: f64,
    },
    Text {
        title: String,
        default: String,
    },
    Choice {
        title: String,
        default: String,
        options: BTreeMap<String, String>,
    },
}

impl Setting {
    pub fn default_value(&self) -> Value {
        match self {
            Self::Bool { default, .. } => Value::from(*default),
            Self::Number { default, .. } => Value::from(*default),
            Self::Text { default, .. } | Self::Choice { default, .. } => {
                Value::from(default.clone())
            }
        }
    }
    pub fn accepts(&self, value: &Value) -> bool {
        match self {
            Self::Bool { .. } => value.is_boolean(),
            Self::Number { min, max, .. } => value
                .as_f64()
                .is_some_and(|n| n.is_finite() && n >= *min && n <= *max),
            Self::Text { .. } => value.as_str().is_some_and(|s| s.len() <= 16_384),
            Self::Choice { options, .. } => value.as_str().is_some_and(|s| options.contains_key(s)),
        }
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.split('.').all(|part| {
            !part.is_empty()
                && part.as_bytes()[0].is_ascii_lowercase()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        })
}

impl Manifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.api != API_VERSION {
            return Err(format!(
                "Unsupported plugin API {}; expected {API_VERSION}",
                self.api
            ));
        }
        if !valid_id(&self.id)
            || self.id == "host"
            || self.name.trim().is_empty()
            || self.name.len() > 256
        {
            return Err(
                "A plugin needs a valid ID other than 'host' and a name of at most 256 bytes"
                    .into(),
            );
        }
        if self
            .dependencies
            .keys()
            .chain(self.conflicts.iter())
            .any(|id| !valid_id(id) || id == &self.id)
        {
            return Err("Invalid or self-referential plugin dependency/conflict".into());
        }
        if self
            .settings
            .iter()
            .any(|(key, setting)| !valid_id(key) || !setting.accepts(&setting.default_value()))
        {
            return Err("Invalid plugin setting or default".into());
        }
        if let Backend::Wasm { entry } | Backend::Native { entry } = &self.backend {
            validate_package_path(entry)?;
        }
        Ok(())
    }
}

/// Portable package names use slash separators on every platform.
pub fn validate_package_path(path: &str) -> Result<(), String> {
    if path.len() > 1024
        || path.contains(['\\', ':', '\0'])
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(
            "Package paths must be relative slash-separated names without traversal".into(),
        );
    }
    Ok(())
}
