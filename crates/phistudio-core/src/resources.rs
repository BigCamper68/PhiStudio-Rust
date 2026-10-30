//! Bounded media inventory and reversible file changes inside private assets.
use crate::{ChartDocument, ChartError, ProjectOptions};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    Image,
    Gif,
    Audio,
    Font,
}
impl ResourceKind {
    pub fn from_name(name: &str) -> Option<Self> {
        let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "png" | "jpg" | "jpeg" | "webp" | "bmp" => Self::Image,
            "gif" => Self::Gif,
            "ogg" | "wav" | "mp3" | "flac" | "m4a" | "aac" | "opus" => Self::Audio,
            "ttf" | "otf" => Self::Font,
            _ => return None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Image => "Image / texture",
            Self::Gif => "GIF",
            Self::Audio => "Audio",
            Self::Font => "Font",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Resource {
    pub name: String,
    pub kind: ResourceKind,
    pub bytes: u64,
    pub references: usize,
}
#[derive(Debug)]
pub struct ResourceUndo {
    assets: PathBuf,
    name: String,
    before: Option<tempfile::NamedTempFile>,
    after: Option<(u64, u64)>,
}
impl ResourceUndo {
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Texture discovery does not scan chart JSON or decode images on the UI thread.
pub fn texture_names(assets: &Path) -> Result<Vec<String>, ChartError> {
    let mut names = crate::package::inventory(assets)?
        .into_iter()
        .filter(|p| {
            matches!(
                ResourceKind::from_name(&p.to_string_lossy()),
                Some(ResourceKind::Image | ResourceKind::Gif)
            )
        })
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}
pub fn texture_path(assets: &Path, name: &str) -> Result<PathBuf, ChartError> {
    if !matches!(
        ResourceKind::from_name(name),
        Some(ResourceKind::Image | ResourceKind::Gif)
    ) {
        return Err(invalid("Choose an image or GIF texture"));
    }
    path(assets, name)
}
fn invalid(s: &str) -> ChartError {
    ChartError::Invalid(s.into())
}
fn path(root: &Path, name: &str) -> Result<PathBuf, ChartError> {
    let rel = crate::package::relative(name)?;
    let root = root.canonicalize()?;
    let mut p = root.clone();
    for part in rel.components() {
        p.push(part);
        if let Ok(m) = fs::symlink_metadata(&p)
            && m.file_type().is_symlink()
        {
            return Err(invalid("Resource links are not supported"));
        }
    }
    if p == root {
        return Err(invalid("Choose a resource file"));
    }
    Ok(p)
}
fn count_strings(value: &serde_json::Value, names: &mut BTreeMap<String, usize>) {
    match value {
        serde_json::Value::String(s) => {
            *names.entry(s.replace('\\', "/")).or_default() += 1;
        }
        serde_json::Value::Array(a) => {
            for v in a {
                count_strings(v, names)
            }
        }
        serde_json::Value::Object(o) => {
            for v in o.values() {
                count_strings(v, names)
            }
        }
        _ => {}
    }
}
pub fn list(
    assets: &Path,
    document: &ChartDocument,
    options: &ProjectOptions,
) -> Result<Vec<Resource>, ChartError> {
    list_with_references(assets, references(document, options)?, None)
}
/// Only reference strings are needed to list assets, not another chart clone.
pub fn references(
    document: &ChartDocument,
    options: &ProjectOptions,
) -> Result<BTreeMap<String, usize>, ChartError> {
    let mut refs = BTreeMap::new();
    count_strings(document.as_json(), &mut refs);
    count_strings(&serde_json::to_value(options)?, &mut refs);
    Ok(refs)
}
pub fn list_with_references(
    assets: &Path,
    mut refs: BTreeMap<String, usize>,
    active_chart: Option<&Path>,
) -> Result<Vec<Resource>, ChartError> {
    let files = crate::package::inventory(assets)?;
    let active = active_chart.and_then(|p| p.canonicalize().ok());
    // Imported projects keep their working JSON outside the original asset
    // directory. Resolve its original path through the existing package map.
    let represented = active_chart
        .and_then(|chart| {
            crate::package::export_chart_path(chart, assets)
                .ok()
                .flatten()
        })
        .and_then(|relative| assets.join(relative).canonicalize().ok());
    // References in other charts and Phira sidecars also protect shared resources.
    for relative in &files {
        if relative
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("json"))
        {
            let source = path(assets, &relative.to_string_lossy())?;
            // The working document already includes the active chart's edits.
            if active.as_ref() == Some(&source) || represented.as_ref() == Some(&source) {
                continue;
            }
            let file = fs::File::open(source)?;
            if let Ok(value) = crate::json_io::read::<serde_json::Value>(
                file.take(ChartDocument::MAX_JSON_BYTES as u64 + 1),
            ) {
                count_strings(&value, &mut refs);
            }
        }
    }
    let mut result = vec![];
    for relative in files {
        let name = relative.to_string_lossy().replace('\\', "/");
        if let Some(kind) = ResourceKind::from_name(&name) {
            result.push(Resource {
                name: name.clone(),
                kind,
                bytes: fs::metadata(path(assets, &name)?)?.len(),
                references: refs.get(&name).copied().unwrap_or(0),
            });
        }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}
fn fingerprint(p: &Path) -> Result<(u64, u64), ChartError> {
    let mut f = fs::File::open(p)?.take(128 * 1024 * 1024 + 1);
    let (mut hash, mut len) = (0xcbf29ce484222325u64, 0);
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        len += n as u64;
        if len > 128 * 1024 * 1024 {
            return Err(invalid("Media exceeds 128 MiB"));
        }
        for b in &buf[..n] {
            hash = (hash ^ *b as u64).wrapping_mul(0x100000001b3);
        }
    }
    Ok((len, hash))
}
fn copy(source: &Path, destination: &Path) -> Result<(), ChartError> {
    let parent = destination
        .parent()
        .ok_or_else(|| invalid("Missing resource directory"))?;
    let mut out = tempfile::NamedTempFile::new_in(parent)?;
    let mut input = fs::File::open(source)?.take(128 * 1024 * 1024 + 1);
    let n = std::io::copy(&mut input, &mut out)?;
    if n == 0 || n > 128 * 1024 * 1024 {
        return Err(invalid("Media must contain 1 byte–128 MiB"));
    }
    out.flush()?;
    out.as_file().sync_all()?;
    out.persist(destination).map_err(|e| e.error)?;
    Ok(())
}
fn backup(root: &Path, p: &Path) -> Result<tempfile::NamedTempFile, ChartError> {
    let folder = root.join("resource-undo");
    fs::create_dir_all(&folder)?;
    let mut out = tempfile::NamedTempFile::new_in(folder)?;
    let mut input = fs::File::open(p)?;
    std::io::copy(&mut input, &mut out)?;
    out.flush()?;
    out.as_file().sync_all()?;
    Ok(out)
}
pub fn add(assets: &Path, source: &Path, name: &str) -> Result<(String, ResourceUndo), ChartError> {
    if ResourceKind::from_name(name).is_none() {
        return Err(invalid("Choose an image, GIF, audio file or font"));
    }
    fingerprint(source)?;
    let folder = assets.join("phistudio-media");
    fs::create_dir_all(&folder)?;
    if fs::symlink_metadata(&folder)?.file_type().is_symlink() {
        return Err(invalid("Invalid media directory"));
    }
    let ext = Path::new(name).extension().unwrap().to_string_lossy();
    let mut out = tempfile::Builder::new()
        .prefix("media-")
        .suffix(&format!(".{ext}"))
        .tempfile_in(&folder)?;
    let mut input = fs::File::open(source)?.take(128 * 1024 * 1024 + 1);
    let n = std::io::copy(&mut input, &mut out)?;
    if n == 0 || n > 128 * 1024 * 1024 {
        return Err(invalid("Media exceeds the size limit"));
    }
    out.flush()?;
    out.as_file().sync_all()?;
    let target = out.into_temp_path().keep().map_err(|e| e.error)?;
    let name = target
        .strip_prefix(assets)
        .map_err(|_| invalid("Invalid media directory"))?
        .to_string_lossy()
        .replace('\\', "/");
    let undo = ResourceUndo {
        assets: assets.to_owned(),
        name: name.clone(),
        before: None,
        after: Some(fingerprint(&target)?),
    };
    Ok((name, undo))
}
pub fn replace(
    root: &Path,
    assets: &Path,
    name: &str,
    source: &Path,
    source_name: &str,
) -> Result<ResourceUndo, ChartError> {
    let kind = ResourceKind::from_name(name).ok_or_else(|| invalid("Unsupported resource"))?;
    if ResourceKind::from_name(source_name) != Some(kind)
        || !Path::new(name)
            .extension()
            .unwrap()
            .eq_ignore_ascii_case(Path::new(source_name).extension().unwrap())
    {
        return Err(invalid("Replacement must use the same file format"));
    }
    let target = path(assets, name)?;
    fingerprint(&target)?;
    fingerprint(source)?;
    let before = backup(root, &target)?;
    copy(source, &target)?;
    Ok(ResourceUndo {
        assets: assets.to_owned(),
        name: name.into(),
        before: Some(before),
        after: Some(fingerprint(&target)?),
    })
}
pub fn delete(
    root: &Path,
    assets: &Path,
    name: &str,
    doc: &ChartDocument,
    options: &ProjectOptions,
) -> Result<ResourceUndo, ChartError> {
    let entry = list(assets, doc, options)?
        .into_iter()
        .find(|r| r.name == name)
        .ok_or_else(|| invalid("Resource no longer exists"))?;
    if entry.references > 0 {
        return Err(invalid(
            "This resource is referenced by a chart or package settings",
        ));
    }
    let target = path(assets, name)?;
    let before = backup(root, &target)?;
    fs::remove_file(&target)?;
    Ok(ResourceUndo {
        assets: assets.to_owned(),
        name: name.into(),
        before: Some(before),
        after: None,
    })
}
pub fn undo(change: ResourceUndo) -> Result<(), ChartError> {
    let target = path(&change.assets, &change.name)?;
    let actual = if target.exists() {
        Some(fingerprint(&target)?)
    } else {
        None
    };
    if actual != change.after {
        return Err(invalid("Resource changed since this operation"));
    }
    if let Some(before) = change.before {
        copy(before.path(), &target)?;
    } else {
        fs::remove_file(target)?;
    }
    Ok(())
}
