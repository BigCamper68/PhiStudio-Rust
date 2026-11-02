use crate::{ChartDocument, ChartError};
use std::{
    fs::File,
    io::{BufWriter, Read, Write},
    path::Path,
};

pub fn load_chart(path: &Path) -> Result<ChartDocument, ChartError> {
    let mut text = String::new();
    File::open(path)?
        .take(ChartDocument::MAX_JSON_BYTES as u64 + 1)
        .read_to_string(&mut text)?;
    ChartDocument::parse(&text)
}

/// Stage in the destination directory and replace only after a complete,
/// flushed write. Call Editor::mark_saved only after this succeeds.
pub fn save_chart(document: &ChartDocument, path: &Path) -> Result<(), ChartError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    {
        let mut output = BufWriter::with_capacity(64 * 1024, staged.as_file_mut());
        serde_json::to_writer(&mut output, &document.serialization_value())?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    if staged.as_file().metadata()?.len() > ChartDocument::MAX_JSON_BYTES as u64 {
        return Err(ChartError::Invalid("Saved chart exceeds 256 MiB".into()));
    }
    staged.as_file().sync_all()?;
    staged.persist(path).map_err(|e| ChartError::Io(e.error))?;
    Ok(())
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ProjectLocation {
    pub chart: std::path::PathBuf,
    pub assets: Option<std::path::PathBuf>,
}

pub(crate) fn checked_location(
    root: &Path,
    location: ProjectLocation,
) -> Result<ProjectLocation, ChartError> {
    let root = root.canonicalize()?;
    let chart = location.chart.canonicalize()?;
    let assets = location.assets.map(|p| p.canonicalize()).transpose()?;
    if !chart.starts_with(&root)
        || !chart.is_file()
        || assets
            .as_ref()
            .is_some_and(|p| !p.starts_with(&root) || !p.is_dir())
    {
        return Err(ChartError::Invalid(
            "Project path is outside private storage".into(),
        ));
    }
    Ok(ProjectLocation { chart, assets })
}

/// Remember only an already imported/saved working copy, never a content URI.
pub fn remember_project(root: &Path, location: ProjectLocation) -> Result<(), ChartError> {
    let location = checked_location(root, location)?;
    crate::library::record_opened_project(root, &location)?;
    let mut staged = tempfile::NamedTempFile::new_in(root)?;
    {
        let mut output = BufWriter::with_capacity(64 * 1024, staged.as_file_mut());
        serde_json::to_writer_pretty(&mut output, &location)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    staged.as_file().sync_all()?;
    staged
        .persist(root.join("last-project.json"))
        .map_err(|e| ChartError::Io(e.error))?;
    Ok(())
}

pub(crate) fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), ChartError> {
    let mut staged = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    {
        let mut output = BufWriter::with_capacity(64 * 1024, staged.as_file_mut());
        serde_json::to_writer(&mut output, value)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    staged.as_file().sync_all()?;
    staged.persist(path).map_err(|e| ChartError::Io(e.error))?;
    Ok(())
}

pub fn last_project(root: &Path) -> Result<Option<(ChartDocument, ProjectLocation)>, ChartError> {
    let file = match File::open(root.join("last-project.json")) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let location: ProjectLocation = crate::json_io::read(file.take(64 * 1024))?;
    let location = checked_location(root, location)?;
    Ok(Some((load_chart(&location.chart)?, location)))
}
