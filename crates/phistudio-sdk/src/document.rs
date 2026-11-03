use crate::DocumentVersion;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Arguments for host/document read and inspect. Paths use RFC 6901 escaping.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentQuery {
    pub version: DocumentVersion,
    pub path: String,
}

/// Select literal object keys without cloning unrelated source data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentFields {
    pub version: DocumentVersion,
    pub path: String,
    pub fields: Vec<String>,
}

/// A bounded array page whose object entries contain only the selected keys.
/// Missing keys stay absent; explicit nulls and non-object entries are retained.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentRows {
    pub version: DocumentVersion,
    pub path: String,
    pub offset: usize,
    pub limit: usize,
    pub fields: Vec<String>,
}

/// Arguments for host/document slice (arrays) and keys (objects).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentPage {
    pub version: DocumentVersion,
    pub path: String,
    #[serde(default)]
    pub offset: usize,
    pub limit: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentValue {
    pub version: DocumentVersion,
    pub value: Value,
}

/// Result of applying an existing native authoring validator to a source object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentValidation {
    pub version: DocumentVersion,
    pub valid: bool,
    pub message: Option<String>,
}

/// Byte limits may return fewer than `limit` entries. Continue at next_offset;
/// None denotes the end, including an empty collection or an offset past it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentItems {
    pub version: DocumentVersion,
    pub offset: usize,
    pub total: usize,
    pub next_offset: Option<usize>,
    pub items: Vec<Value>,
}
