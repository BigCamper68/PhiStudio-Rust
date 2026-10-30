//! Private additions to a project's resources; original asset bytes are retained.
use crate::{ChartError, validate_hitsound};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
pub const MAX_HITSOUND_BYTES: u64 = 8 * 1024 * 1024;
/// Reuse identical rendered clips; publish atomically without changing originals.
pub fn write_generated_hitsound(root: &Path, wav: &[u8]) -> Result<String, ChartError> {
    if wav.is_empty() || wav.len() as u64 > MAX_HITSOUND_BYTES {
        return Err(ChartError::Invalid(
            "Rendered hitsound exceeds 8 MiB".into(),
        ));
    }
    let root = root.canonicalize()?;
    let directory = root.join("phistudio-hitsounds");
    fs::create_dir_all(&directory)?;
    let meta = fs::symlink_metadata(&directory)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(ChartError::Invalid(
            "Invalid hitsound resource directory".into(),
        ));
    }
    let hash = wav.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    });
    for collision in 0..16 {
        let name = format!("gain-{hash:016x}-{}-{collision}.wav", wav.len());
        let path = directory.join(&name);
        if let Ok(meta) = fs::symlink_metadata(&path) {
            if meta.is_file()
                && !meta.file_type().is_symlink()
                && meta.len() == wav.len() as u64
                && fs::read(&path)? == wav
            {
                return Ok(format!("phistudio-hitsounds/{name}"));
            }
            continue;
        }
        let mut staged = tempfile::NamedTempFile::new_in(&directory)?;
        staged.write_all(wav)?;
        staged.as_file().sync_all()?;
        match staged.persist_noclobber(path) {
            Ok(_) => return Ok(format!("phistudio-hitsounds/{name}")),
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.error.into()),
        }
    }
    Err(ChartError::Invalid(
        "Could not publish rendered hitsound".into(),
    ))
}
pub fn hitsound_path(root: &Path, reference: &str) -> Result<std::path::PathBuf, ChartError> {
    let reference = validate_hitsound(reference)?;
    let root = root.canonicalize()?;
    let mut path = root.clone();
    for part in reference.split('/') {
        path.push(part);
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            return Err(ChartError::Invalid(
                "Hitsound links are not supported".into(),
            ));
        }
    }
    if !path.is_file() {
        return Err(ChartError::Invalid("Hitsound is not a file".into()));
    }
    Ok(path)
}
pub fn import_hitsound_asset(root: &Path, source: &Path, name: &str) -> Result<String, ChartError> {
    let root = root.canonicalize()?;
    let meta = fs::metadata(source)?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_HITSOUND_BYTES {
        return Err(ChartError::Invalid(
            "Hitsound audio must contain 1 byte–8 MiB".into(),
        ));
    }
    let directory = root.join("phistudio-hitsounds");
    match fs::create_dir(&directory) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    let meta = fs::symlink_metadata(&directory)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(ChartError::Invalid(
            "Invalid hitsound resource directory".into(),
        ));
    }
    let stem: String = Path::new(name)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(36)
        .collect();
    let extension = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty() && s.len() <= 10 && s.bytes().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("wav");
    let mut file = tempfile::Builder::new()
        .prefix(&format!("hit-{stem}-"))
        .suffix(&format!(".{extension}"))
        .tempfile_in(&directory)?;
    let mut input = fs::File::open(source)?.take(MAX_HITSOUND_BYTES + 1);
    let count = std::io::copy(&mut input, &mut file)?;
    if count == 0 || count > MAX_HITSOUND_BYTES {
        return Err(ChartError::Invalid(
            "Hitsound audio exceeds its size limit".into(),
        ));
    }
    file.flush()?;
    file.as_file().sync_all()?;
    let path = file.into_temp_path().keep().map_err(|e| e.error)?;
    Ok(format!(
        "phistudio-hitsounds/{}",
        path.file_name().unwrap().to_string_lossy()
    ))
}
pub fn list_hitsound_assets(root: &Path, exclude: Option<&str>) -> Result<Vec<String>, ChartError> {
    let root = root.canonicalize()?;
    let mut todo = vec![(root.clone(), 0)];
    let mut names = Vec::new();
    let mut seen = 0;
    while let Some((directory, depth)) = todo.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            seen += 1;
            if seen > 10_000 {
                return Err(ChartError::Invalid(
                    "Resource catalog exceeds 10,000 entries".into(),
                ));
            }
            let meta = fs::symlink_metadata(entry.path())?;
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() && depth < 8 {
                todo.push((entry.path(), depth + 1));
            } else if meta.is_file() && meta.len() <= MAX_HITSOUND_BYTES && meta.len() > 0 {
                let path = entry.path();
                let ext = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if ["wav", "ogg", "mp3", "flac", "m4a", "aac", "opus"].contains(&ext.as_str()) {
                    let name = path
                        .strip_prefix(&root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");
                    if Some(name.as_str()) != exclude && validate_hitsound(&name).is_ok() {
                        names.push(name);
                    }
                }
            }
        }
    }
    names.sort();
    names.truncate(256);
    Ok(names)
}
