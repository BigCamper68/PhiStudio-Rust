use super::*;

pub fn restore(
    root: &Path,
    workspace: &Path,
    id: &str,
    current: &ChartDocument,
    location: &ProjectLocation,
    options: &ProjectOptions,
) -> Result<ImportedProject, ChartError> {
    let workspace = library::checked_workspace(root, workspace)?;
    let v = load(root, &workspace, id)?;
    // Expand outside the active workspace so the automatic backup cannot
    // accidentally include the generation that is about to replace it.
    let staging = root.join("staging");
    fs::create_dir_all(&staging)?;
    if !staging.canonicalize()?.starts_with(root.canonicalize()?) {
        return Err(invalid("Staging is outside private storage"));
    }
    let stage = tempfile::Builder::new()
        .prefix("generation-")
        .tempdir_in(staging)?;
    expand(root, &workspace, &v, stage.path())?;
    package::generation_description(stage.path(), &v.chart, &v.assets, v.options.clone())?;
    save_backup(
        root,
        &workspace,
        "Before restore",
        current,
        location,
        options,
    )?;
    // Keep the complete directory before pointer publication. A failed publication
    // can leave an unused generation, but never a pointer to a removed directory.
    let generation = workspace.join(stage.path().file_name().unwrap());
    if generation.exists() {
        return Err(invalid("Generation already exists"));
    }
    fs::rename(stage.path(), &generation)?;
    let before = crate::branches::load(root, &workspace)?;
    let mut after = before.clone();
    after.heads.insert(after.current.clone(), Some(v.id));
    crate::branches::publish(root, &workspace, &generation, before, after)
}

pub fn delete(root: &Path, workspace: &Path, id: &str) -> Result<(), ChartError> {
    crate::branches::check_delete(root, workspace, id)?;
    load(root, workspace, id)?;
    fs::remove_dir_all(directory(root, workspace, id)?)?;
    Ok(())
}

pub fn duplicate(
    root: &Path,
    workspace: &Path,
    name: &str,
    doc: &ChartDocument,
    location: &ProjectLocation,
    options: &ProjectOptions,
) -> Result<ImportedProject, ChartError> {
    if name.trim().is_empty() || name.len() > 512 {
        return Err(invalid("Use a project name of 1–512 bytes"));
    }
    let workspace = library::checked_workspace(root, workspace)?;
    let base = package::active_root(&workspace, location)?;
    let stage = tempfile::Builder::new()
        .prefix("project-")
        .tempdir_in(library::projects_root(root)?)?;
    for relative in package::inventory(&base)? {
        let target = stage.path().join(&relative);
        fs::create_dir_all(target.parent().unwrap())?;
        let mut out = fs::File::create(&target)?;
        transfer(
            &mut fs::File::open(base.join(&relative))?.take(MAX_FILE + 1),
            &mut out,
        )?;
        out.sync_all()?;
    }
    let chart = location
        .chart
        .canonicalize()?
        .strip_prefix(&base)
        .map_err(|_| invalid("Chart is outside its project"))?
        .to_owned();
    let assets = location
        .assets
        .as_deref()
        .unwrap_or(&base)
        .canonicalize()?
        .strip_prefix(&base)
        .map_err(|_| invalid("Assets are outside their project"))?
        .to_owned();
    crate::save_chart(doc, &stage.path().join(&chart))?;
    package::generation_description(
        stage.path(),
        &chart,
        if assets.as_os_str().is_empty() {
            Path::new(".")
        } else {
            &assets
        },
        options.clone(),
    )?;
    let directory = stage.keep();
    library::rename_project(root, &directory, name)?;
    crate::open_project(root, &directory)
}
