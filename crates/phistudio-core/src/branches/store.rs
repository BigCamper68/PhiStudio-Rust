use super::*;

pub(super) fn read<T: serde::de::DeserializeOwned>(
    path: &Path,
    limit: u64,
) -> Result<T, ChartError> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_file() || m.file_type().is_symlink() || m.len() > limit {
        return Err(invalid("Invalid branch metadata"));
    }
    let mut data = vec![];
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut data)?;
    if data.len() as u64 > limit {
        return Err(invalid("Invalid branch metadata"));
    }
    Ok(serde_json::from_slice(&data)?)
}

pub(super) fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)
}

pub(super) fn validate(root: &Path, workspace: &Path, book: &Branches) -> Result<(), ChartError> {
    if book.version != 1
        || book.heads.is_empty()
        || book.heads.len() > 64
        || !book.heads.contains_key(&book.current)
    {
        return Err(invalid("Invalid branch metadata"));
    }
    for (name, id) in &book.heads {
        if !valid_name(name) {
            return Err(invalid("Use a branch name of 1–128 bytes"));
        }
        if let Some(id) = id {
            versions::load(root, workspace, id)?;
        }
    }
    Ok(())
}

pub fn load(root: &Path, workspace: &Path) -> Result<Branches, ChartError> {
    let folder = versions::folder(root, workspace)?;
    let journal = folder.join("branches.pending.json");
    if journal.exists() {
        let pending: Pending = read(&journal, 256 * 1024)?;
        if !pending.generation.starts_with("generation-")
            || !pending
                .generation
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(invalid("Invalid branch journal"));
        }
        // Only the atomic descriptor proves publication. A legacy directory scan
        // can discover an unpublished generation and must never select its refs.
        let descriptor = workspace.join(".phistudio-workspace.json");
        let active = if descriptor.exists() {
            let value: Value = read(&descriptor, 4 * 1024 * 1024)?;
            value["charts"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|chart| {
                    chart["working"].as_str().is_some_and(|name| {
                        Path::new(name)
                            .components()
                            .next()
                            .is_some_and(|part| part.as_os_str() == pending.generation.as_str())
                    })
                })
        } else {
            false
        };
        let book = if active {
            pending.after
        } else {
            pending.before
        };
        validate(root, workspace, &book)?;
        if storage::write_json(&folder.join("branches.json"), &book).is_ok() {
            let _ = fs::remove_file(journal);
        }
        return Ok(book);
    }
    let path = folder.join("branches.json");
    let book = if path.exists() {
        read(&path, 128 * 1024)?
    } else {
        Branches::default()
    };
    validate(root, workspace, &book)?;
    Ok(book)
}

pub(super) fn write(root: &Path, workspace: &Path, book: &Branches) -> Result<(), ChartError> {
    validate(root, workspace, book)?;
    storage::write_json(
        &versions::folder(root, workspace)?.join("branches.json"),
        book,
    )
}

pub(crate) fn advance(root: &Path, workspace: &Path, id: &str) -> Result<(), ChartError> {
    let mut book = load(root, workspace)?;
    book.heads.insert(book.current.clone(), Some(id.into()));
    write(root, workspace, &book)
}

pub(crate) fn publish(
    root: &Path,
    workspace: &Path,
    generation: &Path,
    before: Branches,
    after: Branches,
) -> Result<ImportedProject, ChartError> {
    validate(root, workspace, &before)?;
    validate(root, workspace, &after)?;
    let journal = versions::folder(root, workspace)?.join("branches.pending.json");
    let name = generation
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| invalid("Invalid branch journal"))?
        .to_owned();
    storage::write_json(
        &journal,
        &Pending {
            before,
            after,
            generation: name,
        },
    )?;
    let result = package::publish_generation(workspace, generation);
    if result.is_err() {
        let _ = load(root, workspace);
        return result;
    }
    let _ = load(root, workspace);
    result
}

pub fn create(
    root: &Path,
    workspace: &Path,
    name: &str,
    doc: &ChartDocument,
    location: &ProjectLocation,
    options: &ProjectOptions,
) -> Result<Branches, ChartError> {
    let book = load(root, workspace)?;
    let name = name.trim();
    if !valid_name(name) {
        return Err(invalid("Use a branch name of 1–128 bytes"));
    }
    if book.heads.contains_key(name) || book.heads.len() >= 64 {
        return Err(invalid(
            "Branch already exists or the 64-branch limit was reached",
        ));
    }
    let version = versions::save(root, workspace, name, doc, location, options)?;
    let mut book = load(root, workspace)?;
    book.heads.insert(name.into(), Some(version.id));
    book.current = name.into();
    write(root, workspace, &book)?;
    Ok(book)
}

pub fn remove(root: &Path, workspace: &Path, name: &str) -> Result<Branches, ChartError> {
    let mut book = load(root, workspace)?;
    if name == book.current {
        return Err(invalid("Cannot delete the active branch"));
    }
    if book.heads.remove(name).is_none() {
        return Err(invalid("Branch no longer exists"));
    }
    write(root, workspace, &book)?;
    Ok(book)
}

pub(crate) fn check_delete(root: &Path, workspace: &Path, id: &str) -> Result<(), ChartError> {
    let book = load(root, workspace)?;
    if book.heads.values().any(|v| v.as_deref() == Some(id))
        || versions::list(root, workspace)?
            .iter()
            .any(|v| v.parents.iter().any(|p| p == id))
    {
        return Err(invalid(
            "Version is a branch head or ancestor; keep it to preserve history",
        ));
    }
    Ok(())
}

pub(super) fn stage(root: &Path) -> Result<tempfile::TempDir, ChartError> {
    let dir = root.join("staging");
    fs::create_dir_all(&dir)?;
    if !dir.canonicalize()?.starts_with(root.canonicalize()?) {
        return Err(invalid("Staging is outside private storage"));
    }
    Ok(tempfile::Builder::new()
        .prefix("generation-")
        .tempdir_in(dir)?)
}

pub fn checkout(
    root: &Path,
    workspace: &Path,
    name: &str,
    doc: &ChartDocument,
    location: &ProjectLocation,
    options: &ProjectOptions,
) -> Result<ImportedProject, ChartError> {
    let workspace = crate::library::checked_workspace(root, workspace)?;
    let book = load(root, &workspace)?;
    if name == book.current {
        return Err(invalid("This branch is already active"));
    }
    let id = book
        .heads
        .get(name)
        .and_then(|s| s.as_deref())
        .ok_or_else(|| invalid("Save a version before switching to this branch"))?;
    let version = versions::load(root, &workspace, id)?;
    let stage = stage(root)?;
    versions::expand(root, &workspace, &version, stage.path())?;
    package::generation_description(
        stage.path(),
        &version.chart,
        &version.assets,
        version.options,
    )?;
    versions::save_backup(
        root,
        &workspace,
        "Before branch switch",
        doc,
        location,
        options,
    )?;
    let before = load(root, &workspace)?;
    let mut after = before.clone();
    after.current = name.into();
    let generation = workspace.join(stage.path().file_name().unwrap());
    fs::rename(stage.path(), &generation)?;
    publish(root, &workspace, &generation, before, after)
}
