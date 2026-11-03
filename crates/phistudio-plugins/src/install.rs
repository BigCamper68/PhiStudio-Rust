//! Bounded package publication. The validator runs before the package is visible.
use crate::Package;
use phistudio_sdk::{Backend, valid_id, validate_package_path};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_FILES: usize = 2048;
const MAX_BYTES: u64 = 128 * 1024 * 1024;

pub fn install_package<T>(
    source: &Path,
    library: &Path,
    validate: impl FnOnce(&Package) -> Result<T, String>,
) -> Result<(Package, T), String> {
    fs::create_dir_all(library).map_err(|e| e.to_string())?;
    let staging = tempfile::Builder::new()
        .prefix(".plugin-install-")
        .tempdir_in(library)
        .map_err(|e| e.to_string())?;
    let mut total = 0;
    let mut count = 0;
    if source.is_dir() {
        copy_directory(source, staging.path(), &mut total, &mut count, 0)?;
    } else {
        let file = fs::File::open(source).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > MAX_BYTES {
            return Err("Plugin archive exceeds 128 MiB".into());
        }
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        if archive.len() > MAX_FILES {
            return Err("Plugin archive exceeds 2048 entries".into());
        }
        let mut names = std::collections::BTreeSet::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = std::str::from_utf8(entry.name_raw())
                .map_err(|_| "Plugin paths must be UTF-8")?
                .to_owned();
            let directory = entry.is_dir();
            let name = if directory {
                name.trim_end_matches('/')
            } else {
                &name
            };
            validate_package_path(name)?;
            if name.split('/').count() > 64 {
                return Err("Plugin paths exceed 64 levels".into());
            }
            if !names.insert(name.to_owned()) {
                return Err("Duplicate plugin archive path".into());
            }
            if entry
                .unix_mode()
                .is_some_and(|m| !matches!(m & 0o170000, 0 | 0o100000 | 0o040000))
            {
                return Err("Plugin archives cannot contain links or special files".into());
            }
            let target = staging.path().join(name);
            if directory {
                fs::create_dir_all(target).map_err(|e| e.to_string())?;
            } else {
                copy_file(&mut entry, &target, &mut total, &mut count)?;
            }
        }
    }
    let package = Package::open(staging.path())?;
    if !matches!(
        package.manifest.backend,
        Backend::Wasm { .. } | Backend::Native { .. }
    ) {
        return Err("Unsupported external plugin backend".into());
    }
    let target = library.join(&package.manifest.id);
    if target.exists() {
        return Err(
            "Plugin is already installed; remove it before installing another package".into(),
        );
    }
    let validated = validate(&package)?;
    fs::rename(staging.path(), &target).map_err(|e| e.to_string())?;
    Ok((Package::open(&target)?, validated))
}

fn copy_directory(
    source: &Path,
    target: &Path,
    total: &mut u64,
    count: &mut usize,
    depth: usize,
) -> Result<(), String> {
    if depth >= 64 {
        return Err("Plugin paths exceed 64 levels".into());
    }
    for item in fs::read_dir(source).map_err(|e| e.to_string())? {
        let item = item.map_err(|e| e.to_string())?;
        let kind = item.file_type().map_err(|e| e.to_string())?;
        let name = item.file_name();
        let name = name.to_str().ok_or("Plugin paths must be UTF-8")?;
        // Development build directories and VCS metadata are not package assets.
        if name == "target" || name.starts_with('.') {
            continue;
        }
        validate_package_path(name)?;
        let output = target.join(name);
        if kind.is_symlink() {
            return Err("Plugin packages cannot contain symlinks".into());
        }
        if kind.is_dir() {
            *count += 1;
            if *count > MAX_FILES {
                return Err("Plugin package exceeds 2048 entries".into());
            }
            fs::create_dir(&output).map_err(|e| e.to_string())?;
            copy_directory(&item.path(), &output, total, count, depth + 1)?;
        } else if kind.is_file() {
            copy_file(
                &mut fs::File::open(item.path()).map_err(|e| e.to_string())?,
                &output,
                total,
                count,
            )?;
        } else {
            return Err("Plugin packages must contain only regular files".into());
        }
    }
    Ok(())
}
fn copy_file(
    source: &mut dyn Read,
    target: &Path,
    total: &mut u64,
    count: &mut usize,
) -> Result<(), String> {
    *count += 1;
    if *count > MAX_FILES {
        return Err("Plugin package exceeds 2048 files".into());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|e| e.to_string())?;
    let written = std::io::copy(
        &mut source.take(MAX_BYTES.saturating_sub(*total) + 1),
        &mut output,
    )
    .map_err(|e| e.to_string())?;
    *total += written;
    if *total > MAX_BYTES {
        return Err("Plugin package exceeds 128 MiB".into());
    }
    output.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Remove from discovery atomically; personal storage and project data survive.
pub fn retire_package(library: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err("Invalid plugin ID".into());
    }
    let path = library.join(id);
    let canonical = path.canonicalize().map_err(|e| e.to_string())?;
    if canonical.parent() != Some(library.canonicalize().map_err(|e| e.to_string())?.as_path()) {
        return Err("Installed plugin leaves its library".into());
    }
    let trash = library.join(".removed");
    fs::create_dir_all(&trash).map_err(|e| e.to_string())?;
    let holding = tempfile::Builder::new()
        .prefix(id)
        .tempdir_in(&trash)
        .map_err(|e| e.to_string())?;
    let destination = holding.path().join(id);
    fs::rename(path, &destination).map_err(|e| e.to_string())?;
    let _ = holding.keep();
    Ok(destination)
}
