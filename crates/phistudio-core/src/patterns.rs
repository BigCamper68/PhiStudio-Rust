//! Persistent private pattern library; custom hitsounds travel with the pattern.
use crate::{ChartError, objects::ObjectClipboard, storage::write_json};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
const MAX_METADATA: u64 = 16 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pattern {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub favorite: bool,
    pub clipboard: ObjectClipboard,
    pub sounds: BTreeMap<String, String>,
}
fn invalid(s: &str) -> ChartError {
    ChartError::Invalid(s.into())
}
fn directory(root: &Path, id: &str) -> Result<PathBuf, ChartError> {
    if !id.starts_with("pattern-") || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
        return Err(invalid("Invalid pattern ID"));
    }
    let root = root.canonicalize()?;
    let path = root.join(id);
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_dir()
        || meta.file_type().is_symlink()
        || path.canonicalize()?.parent() != Some(root.as_path())
    {
        return Err(invalid("Pattern is outside the library"));
    }
    Ok(path)
}
pub fn load(root: &Path, id: &str) -> Result<Pattern, ChartError> {
    let path = directory(root, id)?.join("pattern.json");
    if fs::symlink_metadata(&path)?.file_type().is_symlink() {
        return Err(invalid("Pattern metadata cannot be a link"));
    }
    let mut bytes = vec![];
    fs::File::open(path)?
        .take(MAX_METADATA + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_METADATA {
        return Err(invalid("Pattern metadata exceeds 16 MiB"));
    }
    let pattern: Pattern = serde_json::from_slice(&bytes)?;
    if pattern.version != 1
        || pattern.id != id
        || pattern.clipboard.is_empty()
        || pattern.clipboard.len() > crate::tools::MAX_OBJECTS
        || pattern.clipboard.required_lines() > 4096
    {
        return Err(invalid("Invalid or unsupported pattern"));
    }
    for (reference, asset) in &pattern.sounds {
        crate::validate_hitsound(reference)?;
        crate::validate_hitsound(asset)?;
    }
    Ok(pattern)
}
#[derive(Clone, Debug)]
pub struct PatternSummary {
    pub id: String,
    pub name: String,
    pub favorite: bool,
    pub objects: usize,
    pub lines: usize,
    pub duration: f64,
}
pub fn list(root: &Path) -> Result<Vec<PatternSummary>, ChartError> {
    fs::create_dir_all(root)?;
    let mut result = vec![];
    for entry in fs::read_dir(root)?.take(2048) {
        let entry = entry?;
        if let Some(id) = entry.file_name().to_str()
            && let Ok(pattern) = load(root, id)
        {
            result.push(PatternSummary {
                id: pattern.id,
                name: pattern.name,
                favorite: pattern.favorite,
                objects: pattern.clipboard.len(),
                lines: pattern.clipboard.required_lines(),
                duration: pattern.clipboard.last.as_f64() - pattern.clipboard.first.as_f64(),
            });
        }
    }
    result.sort_by(|a, b| {
        b.favorite
            .cmp(&a.favorite)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(result)
}
pub fn save(
    root: &Path,
    name: &str,
    clipboard: ObjectClipboard,
    assets: Option<&Path>,
) -> Result<Pattern, ChartError> {
    if name.trim().is_empty()
        || name.len() > 512
        || clipboard.is_empty()
        || clipboard.len() > crate::tools::MAX_OBJECTS
    {
        return Err(invalid("Name the pattern and select 1–20,000 objects"));
    }
    fs::create_dir_all(root)?;
    let stage = tempfile::Builder::new()
        .prefix("pattern-")
        .tempdir_in(root)?;
    let id = stage
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut sounds = BTreeMap::new();
    let mut total = 0;
    let folder = stage.path().join("assets");
    fs::create_dir(&folder)?;
    for (index, reference) in clipboard.custom_hitsounds()?.into_iter().enumerate() {
        let source = crate::hitsound_path(
            assets.ok_or_else(|| invalid("This pattern needs its project's custom hitsounds"))?,
            &reference,
        )?;
        let size = source.metadata()?.len();
        total += size;
        if size == 0 || size > crate::MAX_HITSOUND_BYTES || total > 64 * 1024 * 1024 {
            return Err(invalid("Pattern sounds exceed the size limit"));
        }
        let extension = source.extension().and_then(|v| v.to_str()).unwrap_or("ogg");
        let stored = format!("sound-{index}.{extension}");
        crate::validate_hitsound(&stored)?;
        let mut input = fs::File::open(source)?.take(crate::MAX_HITSOUND_BYTES + 1);
        let mut out = fs::File::create(folder.join(&stored))?;
        let copied = std::io::copy(&mut input, &mut out)?;
        if copied != size {
            return Err(invalid("Sound changed while saving the pattern"));
        }
        out.flush()?;
        out.sync_all()?;
        sounds.insert(reference, stored);
    }
    let pattern = Pattern {
        version: 1,
        id,
        name: name.trim().into(),
        favorite: false,
        clipboard,
        sounds,
    };
    write_json(&stage.path().join("pattern.json"), &pattern)?;
    if stage.path().join("pattern.json").metadata()?.len() > MAX_METADATA {
        return Err(invalid("Pattern metadata exceeds 16 MiB"));
    }
    let _ = stage.keep();
    Ok(pattern)
}
pub fn favorite(root: &Path, id: &str, value: bool) -> Result<(), ChartError> {
    let mut pattern = load(root, id)?;
    pattern.favorite = value;
    write_json(&directory(root, id)?.join("pattern.json"), &pattern)
}
pub fn delete(root: &Path, id: &str) -> Result<(), ChartError> {
    load(root, id)?;
    fs::remove_dir_all(directory(root, id)?)?;
    Ok(())
}
pub fn prepare(
    root: &Path,
    id: &str,
    assets: Option<&Path>,
) -> Result<ObjectClipboard, ChartError> {
    let mut pattern = load(root, id)?;
    let mut remap = BTreeMap::new();
    for (reference, stored) in &pattern.sounds {
        let source = crate::hitsound_path(&directory(root, id)?.join("assets"), stored)?;
        let new = crate::import_hitsound_asset(
            assets.ok_or_else(|| invalid("Save the chart before inserting custom sounds"))?,
            &source,
            stored,
        )?;
        remap.insert(reference.clone(), new);
    }
    pattern.clipboard.remap_hitsounds(&remap);
    Ok(pattern.clipboard)
}
