//! Private resource-pack library. Imported archives and user files stay unchanged.
use crate::ChartError;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkinEntry {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub assets: PathBuf,
}
pub fn valid_id(id: &str) -> bool {
    id.starts_with("skin-")
        && id.len() <= 80
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
fn invalid(s: impl Into<String>) -> ChartError {
    ChartError::Invalid(s.into())
}
pub fn list(root: &Path) -> Result<Vec<SkinEntry>, ChartError> {
    fs::create_dir_all(root)?;
    let root = root.canonicalize()?;
    let mut entries = vec![];
    for dir in fs::read_dir(&root)? {
        let dir = dir?;
        if !dir.file_type()?.is_dir() {
            continue;
        }
        let Some(id) = dir
            .file_name()
            .to_str()
            .map(str::to_owned)
            .filter(|s| valid_id(s))
        else {
            continue;
        };
        let manifest = dir.path().join(".phistudio-skin.json");
        if !manifest.is_file() {
            continue;
        }
        let mut bytes = vec![];
        fs::File::open(manifest)?
            .take(64 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 64 * 1024 {
            return Err(invalid("Invalid skin description"));
        }
        let mut entry: SkinEntry = serde_json::from_slice(&bytes)?;
        let relative = crate::package::relative(
            entry
                .assets
                .to_str()
                .ok_or_else(|| invalid("Invalid skin path"))?,
        )?;
        let path = dir.path().join(relative).canonicalize()?;
        if entry.id != id || !path.starts_with(&root) || !path.is_dir() {
            return Err(invalid("Invalid skin directory"));
        }
        entry.assets = path;
        entries.push(entry);
    }
    entries.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    Ok(entries)
}
pub fn import(
    source: &Path,
    root: &Path,
    inspect: impl FnOnce(&Path) -> Result<(String, String, String), ChartError>,
) -> Result<SkinEntry, ChartError> {
    if source.metadata()?.len() > 256 * 1024 * 1024 {
        return Err(invalid("Skin archive exceeds 256 MiB"));
    }
    fs::create_dir_all(root)?;
    let stage = tempfile::Builder::new().prefix("skin-").tempdir_in(root)?;
    let assets = stage.path().join("assets");
    fs::create_dir(&assets)?;
    let mut zip =
        zip::ZipArchive::new(fs::File::open(source)?).map_err(|e| invalid(e.to_string()))?;
    if zip.len() > 256 {
        return Err(invalid("Skin archive contains more than 256 entries"));
    }
    let mut total = 0;
    let mut names = HashSet::new();
    let mut metadata = vec![];
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| invalid(e.to_string()))?;
        let path = crate::package::relative(file.name())?;
        if !names.insert(path.clone()) {
            return Err(invalid("Duplicate path in skin archive"));
        }
        if file
            .unix_mode()
            .is_some_and(|m| ![0, 0o100000, 0o040000].contains(&(m & 0o170000)))
        {
            return Err(invalid(
                "Skin archives cannot contain links or special files",
            ));
        }
        let dest = assets.join(&path);
        if file.is_dir() {
            fs::create_dir_all(dest)?;
            continue;
        }
        if file.size() > 32 * 1024 * 1024 || total + file.size() > 128 * 1024 * 1024 {
            return Err(invalid(
                "Skin exceeds the 32 MiB file / 128 MiB total limit",
            ));
        }
        fs::create_dir_all(dest.parent().unwrap())?;
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dest)?;
        let copied = io::copy(&mut (&mut file).take(32 * 1024 * 1024 + 1), &mut out)?;
        total += copied;
        if copied > 32 * 1024 * 1024 || total > 128 * 1024 * 1024 {
            return Err(invalid("Skin exceeds the extraction limit"));
        }
        if path.file_name().is_some_and(|s| s == "info.yml")
            && !path.components().any(|s| s.as_os_str() == "__MACOSX")
        {
            metadata.push(path);
        }
    }
    if metadata.len() != 1 {
        return Err(invalid("Choose a Phira skin ZIP containing one info.yml"));
    }
    let relative = Path::new("assets").join(metadata[0].parent().unwrap());
    let (name, author, description) = inspect(&stage.path().join(&relative))?;
    let id = stage
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let mut entry = SkinEntry {
        id,
        name,
        author,
        description,
        assets: relative,
    };
    crate::storage::write_json(&stage.path().join(".phistudio-skin.json"), &entry)?;
    let published = stage.keep().canonicalize()?;
    entry.assets = published.join(entry.assets);
    Ok(entry)
}
pub fn delete(root: &Path, id: &str) -> Result<(), ChartError> {
    if !valid_id(id) {
        return Err(invalid("Invalid skin ID"));
    }
    let root = root.canonicalize()?;
    let path = root.join(id);
    if !path.join(".phistudio-skin.json").is_file() {
        return Err(invalid("Unknown imported skin"));
    }
    if fs::symlink_metadata(&path)?.file_type().is_symlink()
        || path.canonicalize()?.parent() != Some(root.as_path())
    {
        return Err(invalid("Invalid skin directory"));
    }
    fs::remove_dir_all(path)?;
    Ok(())
}
