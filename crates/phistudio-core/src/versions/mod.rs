//! Named complete workspace versions. Restore publishes one atomic descriptor;
//! the previous generation and external imports are never overwritten.
pub(crate) mod objects;
mod store;
use store::directory;
pub(crate) use store::stats;
pub(crate) use store::{expand, folder, save_backup, snapshot, transfer};
pub use store::{list, load, save};
mod compare;
pub use compare::difference;
mod restore;
use crate::{
    ChartDocument, ChartError, ImportedProject, ProjectLocation, ProjectOptions, library, package,
    storage,
};
pub use restore::{delete, duplicate, restore};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
const MAX_FILE: u64 = 128 * 1024 * 1024;
const MAX_TOTAL: u64 = 1032 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Fingerprint {
    pub bytes: u64,
    pub hash: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Version {
    pub version: u32,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_name: Option<String>,
    pub created_ms: u64,
    #[serde(default)]
    pub parents: Vec<String>,
    pub chart: PathBuf,
    pub assets: PathBuf,
    pub options: ProjectOptions,
    pub notes: usize,
    pub events: usize,
    pub story: usize,
    pub files: BTreeMap<String, Fingerprint>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub blobs: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<String>,
    #[serde(default)]
    pub author: Author,
    #[serde(default = "legacy_kind")]
    pub kind: String,
}
fn legacy_kind() -> String {
    "legacy".into()
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    pub email: String,
}
impl Default for Author {
    fn default() -> Self {
        Self {
            name: "Local author".into(),
            email: String::new(),
        }
    }
}
#[derive(Debug)]
pub struct Difference {
    pub notes: [usize; 2],
    pub events: [usize; 2],
    pub story: [usize; 2],
    pub chart_changed: bool,
    pub changed_files: Vec<String>,
    pub chart_changes: Vec<String>,
}
fn invalid(s: impl Into<String>) -> ChartError {
    ChartError::Invalid(s.into())
}
