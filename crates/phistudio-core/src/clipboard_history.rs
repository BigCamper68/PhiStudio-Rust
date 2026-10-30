//! Portable mixed clipboard history with bounded retention and owned sound assets.
use crate::{ChartError, objects::ObjectClipboard, patterns, storage::write_json};
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub pinned: bool,
    pub objects: usize,
    pub lines: usize,
    pub preview: Vec<[f64; 4]>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct History {
    version: u32,
    pub entries: Vec<Entry>,
}
impl Default for History {
    fn default() -> Self {
        Self {
            version: 1,
            entries: vec![],
        }
    }
}
fn invalid() -> ChartError {
    ChartError::Invalid("Invalid clipboard history or retention limit".into())
}
impl History {
    pub fn load(root: &Path) -> Result<Self, ChartError> {
        fs::create_dir_all(root)?;
        let path = root.join("history.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let m = fs::symlink_metadata(&path)?;
        if !m.is_file() || m.file_type().is_symlink() || m.len() > 2 * 1024 * 1024 {
            return Err(invalid());
        }
        let mut bytes = vec![];
        fs::File::open(path)?
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err(invalid());
        }
        let value: Self = serde_json::from_slice(&bytes)?;
        let ids: std::collections::BTreeSet<_> = value.entries.iter().map(|e| &e.id).collect();
        if value.version != 1
            || value.entries.len() > 40
            || value.entries.iter().filter(|e| e.pinned).count() > 20
            || ids.len() != value.entries.len()
        {
            return Err(invalid());
        }
        for e in &value.entries {
            if e.name.len() > 512
                || e.preview.len() > 200
                || e.preview.iter().flatten().any(|v| !v.is_finite())
            {
                return Err(invalid());
            }
            patterns::load(root, &e.id)?;
        }
        Ok(value)
    }
    fn publish(&mut self, root: &Path, mut next: Self) -> Result<(), ChartError> {
        let mut recent = 0;
        next.entries.retain(|e| {
            if e.pinned {
                true
            } else {
                recent += 1;
                recent <= 20
            }
        });
        let removed: Vec<_> = self
            .entries
            .iter()
            .filter(|e| !next.entries.iter().any(|n| n.id == e.id))
            .map(|e| e.id.clone())
            .collect();
        write_json(&root.join("history.json"), &next)?;
        *self = next;
        for id in removed {
            patterns::delete(root, &id)?;
        }
        Ok(())
    }
    pub fn push(
        &mut self,
        root: &Path,
        clipboard: ObjectClipboard,
        assets: Option<&Path>,
    ) -> Result<(), ChartError> {
        let entry = Entry {
            id: String::new(),
            name: String::new(),
            pinned: false,
            objects: clipboard.len(),
            lines: clipboard.required_lines(),
            preview: clipboard.preview(),
        };
        let pattern = patterns::save(root, "Clipboard", clipboard, assets)?;
        let id = pattern.id;
        let mut next = self.clone();
        next.entries.insert(
            0,
            Entry {
                id: id.clone(),
                ..entry
            },
        );
        if let Err(error) = self.publish(root, next) {
            if !self.entries.iter().any(|e| e.id == id) {
                let _ = patterns::delete(root, &id);
            }
            return Err(error);
        }
        Ok(())
    }
    pub fn rename(&mut self, root: &Path, id: &str, name: &str) -> Result<(), ChartError> {
        if name.len() > 512 {
            return Err(invalid());
        }
        let mut next = self.clone();
        next.entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(invalid)?
            .name = name.trim().into();
        self.publish(root, next)
    }
    pub fn pin(&mut self, root: &Path, id: &str, pin: bool) -> Result<(), ChartError> {
        let mut next = self.clone();
        next.entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(invalid)?
            .pinned = pin;
        if next.entries.iter().filter(|e| e.pinned).count() > 20 {
            return Err(invalid());
        }
        self.publish(root, next)
    }
    pub fn remove(&mut self, root: &Path, id: &str) -> Result<(), ChartError> {
        let mut next = self.clone();
        next.entries.retain(|e| e.id != id);
        self.publish(root, next)
    }
    pub fn activate(
        &self,
        root: &Path,
        id: &str,
        assets: Option<&Path>,
    ) -> Result<ObjectClipboard, ChartError> {
        if !self.entries.iter().any(|e| e.id == id) {
            return Err(invalid());
        }
        patterns::prepare(root, id, assets)
    }
}
