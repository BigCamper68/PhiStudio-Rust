//! Project-local version control, separate from Save and editor Undo.
mod bundle;
mod graph;
mod line_merge;
mod merge;
mod operations;
mod repository;
mod tree;
mod worktree;
pub use crate::branches::{Conflict, Side};
use crate::{
    ChartDocument, ChartError, ProjectLocation, ProjectOptions,
    versions::{self, Author, Version},
};
pub use bundle::{CollectionSummary, ImportSummary, export_bundle, import_bundle};
pub use graph::{GraphRow, graph};
pub use repository::Repository;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
pub use worktree::{WorkingCopy, is_working_copy, save_working_copy};

pub struct Working<'a> {
    pub document: &'a ChartDocument,
    pub location: &'a ProjectLocation,
    pub options: &'a ProjectOptions,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Head {
    Branch(String),
    Detached(String),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub commit: String,
    pub message: String,
    pub author: Author,
    pub created_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stash {
    pub id: String,
    pub snapshot: String,
    pub base: String,
    pub message: String,
    pub branch: Option<String>,
    pub created_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub created_ms: u64,
    pub message: String,
    pub from: Option<String>,
    pub to: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeKind {
    Merge,
    CherryPick,
    Revert,
    StashApply,
    StashPop,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingMerge {
    pub base: String,
    pub ours: String,
    pub theirs: String,
    pub parents: Vec<String>,
    pub kind: MergeKind,
    pub target: String,
    pub message: String,
    pub stash: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub version: u32,
    pub revision: u64,
    pub head: Head,
    pub branches: BTreeMap<String, String>,
    pub index: String,
    pub tags: BTreeMap<String, Tag>,
    pub stash: Vec<Stash>,
    pub reflog: Vec<LogEntry>,
    pub author: Author,
    pub pending: Option<PendingMerge>,
    #[serde(default)]
    pub retained: Vec<String>,
}
impl State {
    pub fn head_id(&self) -> Option<&str> {
        match &self.head {
            Head::Branch(name) => self.branches.get(name).map(String::as_str),
            Head::Detached(id) => Some(id),
        }
    }
    pub fn branch(&self) -> Option<&str> {
        if let Head::Branch(name) = &self.head {
            Some(name)
        } else {
            None
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}
#[derive(Clone, Debug)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
    pub before_bytes: u64,
    pub after_bytes: u64,
}
#[derive(Clone, Debug)]
pub struct ValueChange {
    pub path: String,
    pub kind: ChangeKind,
    pub before: Option<Value>,
    pub after: Option<Value>,
}
#[derive(Clone, Debug, Default)]
pub struct Difference {
    pub files: Vec<FileChange>,
    pub values: Vec<ValueChange>,
    pub notes: [usize; 2],
    pub events: [usize; 2],
    pub story: [usize; 2],
    pub omitted: bool,
}
impl Difference {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.values.is_empty() && !self.omitted
    }
    pub fn total(&self) -> usize {
        self.files.len() + self.values.len()
    }
}
#[derive(Clone, Debug)]
pub struct Commit {
    pub id: String,
    pub name: String,
    pub parents: Vec<String>,
    pub created_ms: u64,
    pub author: Author,
    pub kind: String,
    pub notes: usize,
    pub events: usize,
    pub story: usize,
    pub files: usize,
    pub system_name: Option<String>,
}
impl From<Version> for Commit {
    fn from(v: Version) -> Self {
        Self {
            id: v.id,
            name: v.name,
            parents: v.parents,
            created_ms: v.created_ms,
            author: v.author,
            kind: v.kind,
            notes: v.notes,
            events: v.events,
            story: v.story,
            files: v.files.len(),
            system_name: v.system_name,
        }
    }
}
pub struct Status {
    pub state: State,
    pub commits: Vec<Commit>,
    pub staged: Difference,
    pub unstaged: Difference,
    pub merge: Option<MergePreview>,
}
impl Status {
    pub fn clean(&self) -> bool {
        self.staged.is_empty() && self.unstaged.is_empty()
    }
}
pub struct MergePreview {
    pub pending: PendingMerge,
    pub conflicts: Vec<Conflict>,
}
pub enum Outcome {
    Unchanged,
    WorkingCopy(WorkingCopy),
    Conflicts(MergePreview),
}
fn invalid(message: &str) -> ChartError {
    ChartError::Invalid(message.into())
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn operation_message(prefix: &str, message: &str) -> String {
    let mut value = format!("{prefix}: {message}");
    while value.len() > 512 {
        value.pop();
    }
    value
}
fn ref_name(name: &str) -> Result<String, ChartError> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > 128
        || name.starts_with('.')
        || name.ends_with('.')
        || name.ends_with('/')
        || name.ends_with(".lock")
        || name.contains("..")
        || name.contains("@{")
        || name == "HEAD"
        || name
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c))
        || name.split('/').any(|p| p.is_empty() || p.starts_with('.'))
    {
        return Err(invalid(
            "Use a nonempty reference name without spaces, control characters or Git punctuation",
        ));
    }
    Ok(name.into())
}
