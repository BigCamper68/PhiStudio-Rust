//! Project activity is private metadata, independent of chart Save and Undo.
use crate::{ChartError, library::checked_workspace, storage::write_json};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
const MAX_BYTES: u64 = 64 * 1024;
#[derive(Default, Serialize, Deserialize)]
struct Activity {
    version: u32,
    total_ms: u64,
    sessions: BTreeMap<String, Session>,
}
#[derive(Serialize, Deserialize)]
struct Session {
    elapsed_ms: u64,
    updated_ms: u64,
}
fn folder(root: &Path) -> Result<PathBuf, ChartError> {
    let root = root.canonicalize()?;
    let folder = root.join("activity");
    fs::create_dir_all(&folder)?;
    let folder = folder.canonicalize()?;
    if !folder.starts_with(root) {
        return Err(ChartError::Invalid(
            "Project activity is outside private storage".into(),
        ));
    }
    Ok(folder)
}
fn path(root: &Path, workspace: &Path) -> Result<PathBuf, ChartError> {
    let workspace = checked_workspace(root, workspace)?;
    let mut name = workspace.file_name().unwrap().to_owned();
    name.push(".json");
    Ok(folder(root)?.join(name))
}
fn read(path: &Path) -> Result<Activity, ChartError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_BYTES => {
            return Err(ChartError::Invalid(
                "Invalid project activity record".into(),
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Activity {
                version: 1,
                ..Default::default()
            });
        }
        Err(e) => return Err(e.into()),
        _ => {}
    }
    let record: Activity = crate::json_io::read(File::open(path)?.take(MAX_BYTES + 1))?;
    if record.version != 1 || record.sessions.len() > 128 {
        return Err(ChartError::Invalid(
            "Unsupported project activity record".into(),
        ));
    }
    Ok(record)
}
/// Read the accumulated editing time in milliseconds.
pub fn project_edit_time(root: &Path, workspace: &Path) -> Result<u64, ChartError> {
    Ok(read(&path(root, workspace)?)?.total_ms)
}
/// Publish an absolute elapsed time for one application session. Retrying the
/// same checkpoint is idempotent; separate application sessions accumulate.
pub fn record_project_edit_time(
    root: &Path,
    workspace: &Path,
    session: &str,
    elapsed_ms: u64,
) -> Result<u64, ChartError> {
    if session.is_empty()
        || session.len() > 64
        || !session
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        return Err(ChartError::Invalid(
            "Invalid editing session identifier".into(),
        ));
    }
    let path = path(root, workspace)?;
    let lock_path = path.with_extension("lock");
    if fs::symlink_metadata(&lock_path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(ChartError::Invalid("Invalid activity lock".into()));
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    fs2::FileExt::lock_exclusive(&lock)?;
    let mut record = read(&path)?;
    let previous = record.sessions.get(session).map_or(0, |s| s.elapsed_ms);
    if elapsed_ms > previous {
        record.total_ms = record.total_ms.saturating_add(elapsed_ms - previous);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u64::MAX as u128) as u64;
        record.sessions.insert(
            session.to_owned(),
            Session {
                elapsed_ms,
                updated_ms: now,
            },
        );
        if record.sessions.len() > 128
            && let Some(oldest) = record
                .sessions
                .iter()
                .filter(|(key, _)| key.as_str() != session)
                .min_by_key(|(_, s)| s.updated_ms)
                .map(|(key, _)| key.clone())
        {
            record.sessions.remove(&oldest);
        }
        write_json(&path, &record)?;
    }
    Ok(record.total_ms)
}
