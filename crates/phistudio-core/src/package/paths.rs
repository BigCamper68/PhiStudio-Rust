use super::*;

/// Recognize only native-owned worktrees; similarly named user folders remain data.
pub(crate) fn private_vcs_path(path: &Path) -> bool {
    let (marker, workspace) = match path.file_name().and_then(|n| n.to_str()) {
        Some(".phistudio-vcs-drafts") => (path.join("owner"), path.parent()),
        Some(".phistudio-vcs-draft.json") => (
            path.with_file_name(".phistudio-vcs-draft-owner"),
            path.ancestors().nth(3),
        ),
        Some(".phistudio-vcs-draft-owner") => (path.to_owned(), path.ancestors().nth(3)),
        _ => return false,
    };
    let Some(id) = workspace.and_then(Path::file_name).and_then(|s| s.to_str()) else {
        return false;
    };
    if !fs::symlink_metadata(&marker)
        .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink() && m.len() < 4096)
    {
        return false;
    }
    let expected = format!("PhiStudio project worktrees v1\n{id}\n");
    fs::read(marker).ok().as_deref() == Some(expected.as_bytes())
}

pub(crate) fn relative(name: &str) -> Result<PathBuf, ChartError> {
    let normalized = name.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.as_os_str().is_empty()
        || normalized.contains(':')
        || normalized.contains('\0')
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(invalid("Invalid path in ZIP"));
    }
    Ok(path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s),
            _ => None,
        })
        .collect())
}

pub(super) fn common_directory(files: &[PathBuf]) -> Result<PathBuf, ChartError> {
    let first = files
        .first()
        .ok_or_else(|| invalid("Package contains no files"))?;
    let mut common = first.parent().unwrap_or(Path::new("")).to_path_buf();
    for path in files {
        while !path.starts_with(&common) {
            if !common.pop() {
                break;
            }
        }
    }
    Ok(common)
}

pub(super) fn safe_inside(
    workspace: &Path,
    path: &Path,
    directory: bool,
) -> Result<PathBuf, ChartError> {
    let normalized = relative(
        path.to_str()
            .ok_or_else(|| invalid("Invalid project path"))?,
    )?;
    let full = workspace.join(normalized).canonicalize()?;
    if !full.starts_with(workspace.canonicalize()?)
        || if directory {
            !full.is_dir()
        } else {
            !full.is_file()
        }
    {
        return Err(invalid("Project description path is outside its workspace"));
    }
    Ok(full)
}

pub(crate) fn inventory(workspace: &Path) -> Result<Vec<PathBuf>, ChartError> {
    fn walk(
        base: &Path,
        directory: &Path,
        files: &mut Vec<PathBuf>,
        depth: usize,
        count: &mut usize,
        total: &mut u64,
    ) -> Result<(), ChartError> {
        if depth > 66 {
            return Err(invalid("Project folders are nested too deeply"));
        }
        for entry in fs::read_dir(directory)? {
            *count += 1;
            if *count > 70_000 {
                return Err(invalid("Project contains too many filesystem entries"));
            }
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                return Err(invalid("Project contains a symbolic link"));
            }
            if private_vcs_path(&entry.path()) {
                continue;
            }
            if kind.is_dir() {
                walk(base, &entry.path(), files, depth + 1, count, total)?;
            } else if kind.is_file() {
                let size = entry.metadata()?.len();
                *total += size;
                if size > 128 * MIB || *total > 1024 * MIB + 8 * MIB {
                    return Err(invalid("Project exceeds the workspace size limit"));
                }
                files.push(entry.path().strip_prefix(base).unwrap().to_owned());
                if files.len() > 2049 {
                    return Err(invalid("Project contains too many files"));
                }
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(workspace, workspace, &mut files, 0, &mut 0, &mut 0)?;
    Ok(files)
}
