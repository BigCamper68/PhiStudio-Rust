//! Local project branches. Generation publication is journalled with branch refs.
mod store;
pub(crate) use store::{advance, check_delete, publish};
pub use store::{checkout, create, load, remove};
use store::{read, stage};
pub(crate) mod merge_logic;
pub(crate) mod merge_tracks;
use crate::{
    ChartDocument, ChartError, ImportedProject, ProjectLocation, ProjectOptions, package, storage,
    versions::{self, Fingerprint, Version},
};
use merge_logic::merge;
pub use merge_logic::{apply_merge, prepare_merge};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    io::Read,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Branches {
    version: u32,
    pub current: String,
    pub heads: BTreeMap<String, Option<String>>,
}
impl Default for Branches {
    fn default() -> Self {
        Self {
            version: 1,
            current: "main".into(),
            heads: BTreeMap::from([("main".into(), None)]),
        }
    }
}
impl Branches {
    pub fn head(&self) -> Option<&str> {
        self.heads.get(&self.current)?.as_deref()
    }
}
fn invalid(s: &str) -> ChartError {
    ChartError::Invalid(s.into())
}

#[derive(Serialize, Deserialize)]
struct Pending {
    before: Branches,
    after: Branches,
    generation: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Side {
    Current,
    Incoming,
}
#[derive(Clone, Debug)]
pub struct Conflict {
    pub path: String,
    pub base: Option<Value>,
    pub current: Option<Value>,
    pub incoming: Option<Value>,
    pub choice: Option<Side>,
}

pub struct MergePlan {
    pub current_branch: String,
    pub incoming_branch: String,
    pub base_id: String,
    pub conflicts: Vec<Conflict>,
    book: Branches,
    base: Value,
    ours: Value,
    theirs: Value,
    merged: Value,
    incoming: Version,
    source: tempfile::TempDir,
    chart: PathBuf,
    assets: PathBuf,
}
impl MergePlan {
    pub fn choose(&mut self, index: usize, side: Side) {
        if let Some(c) = self.conflicts.get_mut(index) {
            c.choice = Some(side);
        }
    }
    pub fn unresolved(&self) -> usize {
        self.conflicts.iter().filter(|c| c.choice.is_none()).count()
    }
    fn resolve(&self) -> Result<Value, ChartError> {
        if self.unresolved() > 0 {
            return Err(invalid("Choose a side for every merge conflict"));
        }
        let choices = self
            .conflicts
            .iter()
            .filter_map(|c| c.choice.map(|s| (c.path.clone(), s)))
            .collect();
        Ok(merge(
            Some(&self.base),
            Some(&self.ours),
            Some(&self.theirs),
            "",
            &choices,
            &mut vec![],
            0,
        )?
        .unwrap())
    }
    pub fn changed_paths(&self) -> Vec<String> {
        let mut changed = vec![];
        for key in ["chart", "options", "project", "files"] {
            if self.merged[key] != self.ours[key] {
                changed.push(key.to_owned());
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests;
