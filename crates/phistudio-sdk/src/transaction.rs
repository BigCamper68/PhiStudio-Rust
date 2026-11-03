use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Indices and asynchronous results are valid only for this document version.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentVersion {
    pub session: u64,
    pub revision: u64,
}

/// Structural edits use RFC 6901 paths and add/replace/remove/test from RFC 6902.
/// Array add inserts; `-` appends. PluginData expands to these reversible edits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum PatchOperation {
    /// Set/remove this plugin's versioned entry, creating its container as needed.
    PluginData {
        plugin: String,
        entry: Option<crate::PluginData>,
    },
    Add {
        path: String,
        value: Value,
    },
    Replace {
        path: String,
        value: Value,
    },
    Remove {
        path: String,
    },
    Test {
        path: String,
        value: Value,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transaction {
    pub version: DocumentVersion,
    pub label: String,
    pub operations: Vec<PatchOperation>,
}
