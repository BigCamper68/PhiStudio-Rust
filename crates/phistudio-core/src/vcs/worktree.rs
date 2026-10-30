use super::*;
use crate::{library, package, storage};
const MARKER: &str = ".phistudio-vcs-draft.json";
#[derive(Serialize, Deserialize)]
struct Marker {
    version: u32,
    workspace: PathBuf,
    chart: PathBuf,
    assets: PathBuf,
    options: ProjectOptions,
    before: Value,
}
pub struct WorkingCopy {
    pub workspace: PathBuf,
    pub document: ChartDocument,
    pub location: ProjectLocation,
    pub options: ProjectOptions,
}
fn owner(path: &Path) -> bool {
    if path.file_name().and_then(|p| p.to_str()) == Some("owner") {
        path.parent().is_some_and(crate::package::private_vcs_path)
    } else {
        crate::package::private_vcs_path(path)
    }
}
fn owner_bytes(workspace: &Path) -> String {
    format!(
        "PhiStudio project worktrees v1\n{}\n",
        workspace.file_name().unwrap().to_string_lossy()
    )
}
fn locate(chart: &Path) -> Option<PathBuf> {
    chart
        .ancestors()
        .skip(1)
        .find(|p| p.join(MARKER).exists() && owner(&p.join(".phistudio-vcs-draft-owner")))
        .map(Path::to_owned)
}
pub fn is_working_copy(chart: &Path) -> bool {
    locate(chart).is_some_and(|dir| !published(&dir, chart))
}
fn published(dir: &Path, chart: &Path) -> bool {
    let Some(workspace) = dir.parent().and_then(Path::parent) else {
        return false;
    };
    let Ok(raw) = description(workspace) else {
        return false;
    };
    raw["charts"].as_array().is_some_and(|rows| {
        rows.iter().any(|row| {
            row["working"].as_str().is_some_and(|path| {
                workspace.join(path).canonicalize().ok() == chart.canonicalize().ok()
            })
        })
    })
}
fn read_marker(chart: &Path) -> Result<Option<(PathBuf, Marker)>, ChartError> {
    let Some(dir) = locate(chart) else {
        return Ok(None);
    };
    if published(&dir, chart) {
        return Ok(None);
    }
    let path = dir.join(MARKER);
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 4 * 1024 * 1024 {
        return Err(invalid("Invalid version working copy"));
    }
    let marker: Marker = crate::json_io::read(fs::File::open(path)?.take(4 * 1024 * 1024 + 1))?;
    let parent = dir
        .parent()
        .ok_or_else(|| invalid("Invalid version working copy"))?;
    let workspace = parent
        .parent()
        .ok_or_else(|| invalid("Invalid version working copy"))?;
    if marker.version != 1
        || parent.file_name().and_then(|p| p.to_str()) != Some(".phistudio-vcs-drafts")
        || !owner(&parent.join("owner"))
        || marker.workspace.canonicalize()? != workspace.canonicalize()?
        || dir
            .join(package::relative(&marker.chart.to_string_lossy())?)
            .canonicalize()?
            != chart.canonicalize()?
    {
        return Err(invalid("Version working copy is outside its project"));
    }
    let assets = dir
        .join(package::relative(&marker.assets.to_string_lossy())?)
        .canonicalize()?;
    if !assets.starts_with(dir.canonicalize()?) || !assets.is_dir() {
        return Err(invalid("Invalid working-copy assets"));
    }
    Ok(Some((dir, marker)))
}
pub(super) fn create(
    root: &Path,
    workspace: &Path,
    version: &Version,
    working: &Working<'_>,
) -> Result<WorkingCopy, ChartError> {
    let workspace = library::checked_workspace(root, workspace)?;
    let parent = workspace.join(".phistudio-vcs-drafts");
    if parent.exists() {
        if fs::symlink_metadata(&parent)?.file_type().is_symlink() || !owner(&parent.join("owner"))
        {
            return Err(invalid(
                "Version worktree folder already contains unrelated data",
            ));
        }
    } else {
        fs::create_dir(&parent)?;
        fs::write(parent.join("owner"), owner_bytes(&workspace))?;
    }
    let stage = tempfile::Builder::new()
        .prefix("generation-")
        .tempdir_in(&parent)?;
    versions::expand(root, &workspace, version, stage.path())?;
    package::generation_description(
        stage.path(),
        &version.chart,
        &version.assets,
        version.options.clone(),
    )?;
    let document = crate::load_chart(&stage.path().join(&version.chart))?;
    let before = if let Some((_, marker)) = read_marker(&working.location.chart)? {
        marker.before
    } else {
        description(&workspace)?
    };
    let marker = Marker {
        version: 1,
        workspace: workspace.clone(),
        chart: version.chart.clone(),
        assets: version.assets.clone(),
        options: version.options.clone(),
        before,
    };
    storage::write_json(&stage.path().join(MARKER), &marker)?;
    fs::write(
        stage.path().join(".phistudio-vcs-draft-owner"),
        owner_bytes(&workspace),
    )?;
    let dir = stage.keep();
    Ok(WorkingCopy {
        workspace,
        document,
        location: ProjectLocation {
            chart: dir.join(&version.chart),
            assets: Some(dir.join(&version.assets)),
        },
        options: version.options.clone(),
    })
}
fn description(workspace: &Path) -> Result<Value, ChartError> {
    let path = workspace.join(".phistudio-workspace.json");
    match fs::File::open(path) {
        Ok(f) => Ok(crate::json_io::read(f.take(4 * 1024 * 1024 + 1))?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Null),
        Err(e) => Err(e.into()),
    }
}
/// Save publishes the already validated worktree. Merely checking out never
/// moves the saved project descriptor or overwrites its assets.
pub fn save_working_copy(
    root: &Path,
    chart: &Path,
    document: &ChartDocument,
    options: &ProjectOptions,
) -> Result<bool, ChartError> {
    let Some((dir, marker)) = read_marker(chart)? else {
        return Ok(false);
    };
    let workspace = library::checked_workspace(root, &marker.workspace)?;
    if description(&workspace)? != marker.before {
        return Err(invalid(
            "Saved project changed; keep or export this working copy before reopening it",
        ));
    }
    crate::save_chart(document, chart)?;
    package::generation_description(&dir, &marker.chart, &marker.assets, options.clone())?;
    package::publish_generation(&workspace, &dir)?;
    // The descriptor is the commit point. Leftover markers after interruption
    // must be recognized as already published on a repeated Save.
    if fs::remove_file(dir.join(MARKER)).is_ok() {
        let _ = fs::remove_file(dir.join(".phistudio-vcs-draft-owner"));
    }
    Ok(true)
}
