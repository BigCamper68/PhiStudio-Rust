use crate::DocumentVersion;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextTarget {
    Note,
    Selection,
    Line,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextArguments {
    pub version: DocumentVersion,
    pub target: ContextTarget,
    pub line: usize,
    pub notes: Vec<ContextNote>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextNote {
    pub line: usize,
    pub index: usize,
}
