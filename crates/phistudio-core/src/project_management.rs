//! Private project creation and reversible removal. External media stay unchanged.
use crate::{ChartDocument, ChartError, ProjectLocation, save_chart};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};
const MIB: u64 = 1024 * 1024;
fn invalid(message: impl Into<String>) -> ChartError {
    ChartError::Invalid(message.into())
}
fn directory(root: &Path, name: &str) -> Result<PathBuf, ChartError> {
    let root = root.canonicalize()?;
    let path = root.join(name);
    fs::create_dir_all(&path)?;
    let path = path.canonicalize()?;
    if !path.starts_with(&root) {
        return Err(invalid("Project storage is outside private storage"));
    }
    Ok(path)
}
#[derive(Clone, Debug)]
pub struct NewProjectMedia {
    pub audio: PathBuf,
    pub audio_name: String,
    pub illustration: PathBuf,
    pub illustration_name: String,
}
fn filename(name: &str, stem: &str, fallback: &str) -> String {
    let extension = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty() && s.len() <= 10 && s.bytes().all(|b| b.is_ascii_alphanumeric()))
        .unwrap_or(fallback);
    format!("{stem}.{}", extension.to_ascii_lowercase())
}
fn copy_media(source: &Path, destination: &Path, limit: u64) -> Result<(), ChartError> {
    let input = File::open(source)?;
    let metadata = input.metadata()?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > limit {
        return Err(invalid(
            "Media must be a nonempty file within the size limit",
        ));
    }
    let mut output = File::create(destination)?;
    let copied = io::copy(&mut input.take(limit + 1), &mut output)?;
    if copied == 0 || copied > limit {
        return Err(invalid("Media exceeds the size limit"));
    }
    output.flush()?;
    output.sync_all()?;
    Ok(())
}
pub fn create_project_with_media(
    root: &Path,
    document: &ChartDocument,
    media: &NewProjectMedia,
) -> Result<ProjectLocation, ChartError> {
    let projects = crate::library::projects_root(root)?;
    let staged = tempfile::Builder::new()
        .prefix("project-")
        .tempdir_in(directory(root, "staging")?)?;
    let audio = filename(&media.audio_name, "audio", "audio");
    let illustration = filename(&media.illustration_name, "illustration", "image");
    copy_media(&media.audio, &staged.path().join(&audio), 128 * MIB)?;
    copy_media(
        &media.illustration,
        &staged.path().join(&illustration),
        32 * MIB,
    )?;
    let mut value = document.as_json().clone();
    value["META"]["song"] = audio.clone().into();
    value["META"]["background"] = illustration.clone().into();
    let document = ChartDocument::from_value(value)?;
    save_chart(&document, &staged.path().join("chart.json"))?;
    let manifest = serde_yaml::to_string(&serde_json::json!({"name": document.name(), "chart": "chart.json", "music": &audio, "illustration": &illustration})).map_err(|e| invalid(e.to_string()))?;
    let mut file = File::create(staged.path().join("info.yml"))?;
    file.write_all(manifest.as_bytes())?;
    file.sync_all()?;
    crate::package::describe_created_project(staged.path(), &document, &audio, &illustration)?;
    let workspace = projects.join(staged.path().file_name().unwrap());
    fs::rename(staged.path(), &workspace)?;
    let _ = staged.keep();
    Ok(ProjectLocation {
        chart: workspace.join("chart.json"),
        assets: Some(workspace),
    })
}
#[derive(Clone, Debug)]
pub struct TrashedProject {
    pub original: PathBuf,
    pub entry: PathBuf,
    pub name: String,
}
#[derive(Serialize, Deserialize)]
struct TrashRecord {
    version: u32,
    folder: String,
    name: String,
}
pub fn trash_project(
    root: &Path,
    workspace: &Path,
    name: &str,
) -> Result<TrashedProject, ChartError> {
    let original = crate::library::checked_workspace(root, workspace)?;
    let slot = tempfile::Builder::new()
        .prefix("deleted-")
        .tempdir_in(directory(root, "trash")?)?;
    let record = TrashRecord {
        version: 1,
        folder: original
            .file_name()
            .unwrap()
            .to_str()
            .ok_or_else(|| invalid("Invalid project folder"))?
            .into(),
        name: name.chars().take(512).collect(),
    };
    crate::storage::write_json(&slot.path().join("entry.json"), &record)?;
    fs::rename(&original, slot.path().join("project"))?;
    let entry = slot.keep();
    // A library record is relative to its project; retaining it permits Undo.
    // The last-session pointer must not refer to a removed workspace.
    let last = root.join("last-project.json");
    if let Ok(file) = File::open(&last)
        && let Ok(location) = crate::json_io::read::<ProjectLocation>(file.take(64 * 1024))
        && location.chart.starts_with(&original)
    {
        let _ = fs::remove_file(last);
    }
    Ok(TrashedProject {
        original,
        entry,
        name: name.into(),
    })
}
pub fn restore_project(root: &Path, trash: &TrashedProject) -> Result<PathBuf, ChartError> {
    let trash_root = directory(root, "trash")?;
    let entry = trash.entry.canonicalize()?;
    if entry.parent() != Some(trash_root.as_path()) || !entry.is_dir() {
        return Err(invalid("Project is outside the private trash"));
    }
    let record_path = entry.join("entry.json");
    let meta = fs::symlink_metadata(&record_path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 16 * 1024 {
        return Err(invalid("Invalid trash record"));
    }
    let record: TrashRecord = crate::json_io::read(File::open(&record_path)?)?;
    let folder = Path::new(&record.folder);
    if record.version != 1
        || folder.components().count() != 1
        || !matches!(folder.components().next(), Some(Component::Normal(_)))
    {
        return Err(invalid("Invalid trash project path"));
    }
    let source = entry.join("project");
    let meta = fs::symlink_metadata(&source)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(invalid("Invalid trashed project"));
    }
    let workspace = crate::library::projects_root(root)?.join(folder);
    match fs::symlink_metadata(&workspace) {
        Ok(_) => return Err(invalid("A project already occupies the original folder")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    fs::rename(source, &workspace)?;
    let _ = fs::remove_file(record_path);
    let _ = fs::remove_dir(entry);
    Ok(workspace)
}
