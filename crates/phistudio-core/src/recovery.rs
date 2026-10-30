//! A single atomic envelope keeps the chart and its resource location together.
//! Recovery never overwrites the explicitly saved project file.
use crate::storage::{checked_location, write_json};
use crate::{ChartDocument, ChartError, ProjectLocation, load_chart};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
};

#[derive(Clone, Debug)]
pub struct RecoverySnapshot {
    pub document: ChartDocument,
    pub location: Option<ProjectLocation>,
    pub beat: f64,
    pub line: usize,
}
#[derive(Deserialize)]
struct Envelope {
    version: u32,
    document: serde_json::Value,
    location: Option<ProjectLocation>,
    beat: f64,
    line: usize,
}
#[derive(Serialize)]
struct WriteEnvelope<'a> {
    version: u32,
    document: &'a serde_json::Value,
    location: Option<ProjectLocation>,
    beat: f64,
    line: usize,
}

pub fn save_recovery(root: &Path, snapshot: &RecoverySnapshot) -> Result<(), ChartError> {
    let location = snapshot
        .location
        .clone()
        .map(|p| checked_location(root, p))
        .transpose()?;
    if !snapshot.beat.is_finite() || snapshot.beat < 0. {
        return Err(ChartError::Invalid("Invalid recovery position".into()));
    }
    let directory = root.join("recovery");
    fs::create_dir_all(&directory)?;
    if !directory.canonicalize()?.starts_with(root.canonicalize()?) {
        return Err(ChartError::Invalid(
            "Recovery folder is outside private storage".into(),
        ));
    }
    write_json(
        &directory.join("session.json"),
        &WriteEnvelope {
            version: 1,
            document: snapshot.document.as_json(),
            location,
            beat: snapshot.beat,
            line: snapshot.line,
        },
    )
}

pub fn load_recovery(root: &Path) -> Result<Option<RecoverySnapshot>, ChartError> {
    let path = root.join("recovery/session.json");
    if let Ok(canonical) = path.canonicalize()
        && !canonical.starts_with(root.canonicalize()?)
    {
        return Err(ChartError::Invalid(
            "Recovery file is outside private storage".into(),
        ));
    }
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let envelope: Envelope =
        crate::json_io::read(file.take(ChartDocument::MAX_JSON_BYTES as u64 + 64 * 1024))?;
    if envelope.version != 1 || !envelope.beat.is_finite() || envelope.beat < 0. {
        return Err(ChartError::Invalid(
            "Unsupported or invalid recovery snapshot".into(),
        ));
    }
    let document = ChartDocument::from_value(envelope.document)?;
    let location = envelope
        .location
        .map(|p| checked_location(root, p))
        .transpose()?;
    // A completed explicit save may precede the worker clearing recovery.
    if let Some(location) = &location
        && !crate::vcs::is_working_copy(&location.chart)
        && let Ok(saved) = load_chart(&location.chart)
        && saved.serialization_value() == document.serialization_value()
    {
        return Ok(None);
    }
    let line = envelope.line.min(document.line_count() - 1);
    Ok(Some(RecoverySnapshot {
        document,
        location,
        beat: envelope.beat,
        line,
    }))
}

pub fn clear_recovery(root: &Path) -> Result<(), ChartError> {
    let directory = root.join("recovery");
    if let Ok(canonical) = directory.canonicalize()
        && !canonical.starts_with(root.canonicalize()?)
    {
        return Err(ChartError::Invalid(
            "Recovery folder is outside private storage".into(),
        ));
    }
    match fs::remove_file(directory.join("session.json")) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
