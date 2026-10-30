//! Private, bounded parameter library; these values never enter chart history.
use crate::{ChartError, storage::write_json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, io::Read, path::Path};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preset {
    pub id: u64,
    pub name: String,
    pub parameters: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Library {
    version: u32,
    next: u64,
    pub presets: Vec<Preset>,
    pub recent: Vec<Value>,
}
impl Default for Library {
    fn default() -> Self {
        Self {
            version: 1,
            next: 1,
            presets: vec![],
            recent: vec![],
        }
    }
}
fn invalid() -> ChartError {
    ChartError::Invalid("Invalid or oversized tool configuration library".into())
}
fn parameters(value: &Value) -> Result<(), ChartError> {
    if !value.is_object() || serde_json::to_vec(value)?.len() > 64 * 1024 {
        Err(invalid())
    } else {
        Ok(())
    }
}
impl Library {
    pub fn load(root: &Path) -> Result<Self, ChartError> {
        let path = root.join("tool-configurations.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let meta = fs::symlink_metadata(&path)?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 8 * 1024 * 1024 {
            return Err(invalid());
        }
        let mut bytes = vec![];
        fs::File::open(path)?
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err(invalid());
        }
        let value: Self = serde_json::from_slice(&bytes)?;
        if value.version != 1
            || value.presets.len() > 100
            || value.recent.len() > 16
            || value
                .presets
                .iter()
                .any(|p| p.name.trim().is_empty() || p.name.len() > 512 || p.id >= value.next)
        {
            return Err(invalid());
        }
        let ids: std::collections::BTreeSet<_> = value.presets.iter().map(|p| p.id).collect();
        if ids.len() != value.presets.len() {
            return Err(invalid());
        }
        for p in value
            .presets
            .iter()
            .map(|p| &p.parameters)
            .chain(&value.recent)
        {
            parameters(p)?;
        }
        Ok(value)
    }
    fn publish(&mut self, root: &Path, next: Self) -> Result<(), ChartError> {
        fs::create_dir_all(root)?;
        write_json(&root.join("tool-configurations.json"), &next)?;
        *self = next;
        Ok(())
    }
    pub fn save(&mut self, root: &Path, name: &str, value: Value) -> Result<(), ChartError> {
        parameters(&value)?;
        if name.trim().is_empty() || name.len() > 512 {
            return Err(ChartError::Invalid("Name the tool configuration".into()));
        }
        let mut next = self.clone();
        if let Some(p) = next.presets.iter_mut().find(|p| p.name == name.trim()) {
            p.parameters = value;
        } else {
            if next.presets.len() >= 100 {
                return Err(invalid());
            }
            let id = next.next;
            next.next = id.checked_add(1).ok_or_else(invalid)?;
            next.presets.push(Preset {
                id,
                name: name.trim().into(),
                parameters: value,
            });
        }
        self.publish(root, next)
    }
    pub fn remove(&mut self, root: &Path, id: u64) -> Result<(), ChartError> {
        let mut next = self.clone();
        next.presets.retain(|p| p.id != id);
        self.publish(root, next)
    }
    pub fn remember(&mut self, root: &Path, value: Value) -> Result<(), ChartError> {
        parameters(&value)?;
        if self.recent.first() == Some(&value) {
            return Ok(());
        }
        let mut next = self.clone();
        next.recent.retain(|v| v != &value);
        next.recent.insert(0, value);
        next.recent.truncate(16);
        self.publish(root, next)
    }
}
