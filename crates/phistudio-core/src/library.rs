//! A library of private workspaces, including imports made by older versions.
//! Records live outside the resource folders; package files are never rewritten.
use crate::{ChartDocument, ChartError, ImportedProject, ProjectLocation, load_chart, save_chart};
use crate::{
    package::inspect_workspace,
    storage::{checked_location, write_json},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug)]
pub struct ProjectSummary {
    pub workspace: PathBuf,
    pub name: String,
    pub source_name: String,
    pub notes: usize,
    pub events: Option<usize>,
    pub edit_time_ms: u64,
    pub last_opened_ms: u64,
    pub cover: Option<PathBuf>,
}
#[derive(Default, Debug)]
pub struct ProjectLibrary {
    pub projects: Vec<ProjectSummary>,
    pub issues: Vec<String>,
}
#[derive(Serialize, Deserialize)]
struct Record {
    version: u32,
    name: String,
    source_name: String,
    #[serde(default)]
    display_name: Option<String>,
    chart: PathBuf,
    assets: Option<PathBuf>,
    notes: usize,
    #[serde(default)]
    events: Option<usize>,
    last_opened_ms: u64,
    #[serde(default)]
    cover: Option<PathBuf>,
}
fn invalid(message: impl Into<String>) -> ChartError {
    ChartError::Invalid(message.into())
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
pub(crate) fn projects_root(root: &Path) -> Result<PathBuf, ChartError> {
    let base = root.canonicalize()?;
    let projects = root.join("projects");
    fs::create_dir_all(&projects)?;
    let projects = projects.canonicalize()?;
    if !projects.starts_with(base) {
        return Err(invalid("Projects folder is outside private storage"));
    }
    Ok(projects)
}
pub(crate) fn checked_workspace(root: &Path, workspace: &Path) -> Result<PathBuf, ChartError> {
    let projects = projects_root(root)?;
    let workspace = workspace.canonicalize()?;
    if workspace.parent() != Some(projects.as_path()) || !workspace.is_dir() {
        return Err(invalid("Project is outside the project library"));
    }
    Ok(workspace)
}
fn record_path(root: &Path, workspace: &Path) -> PathBuf {
    {
        let mut name = workspace.file_name().unwrap().to_os_string();
        name.push(".json");
        root.join("library").join(name)
    }
}
fn read_record(root: &Path, workspace: &Path) -> Result<Option<Record>, ChartError> {
    let file = match File::open(record_path(root, workspace)) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let record: Record = crate::json_io::read(file.take(64 * 1024))?;
    if record.version != 1 {
        return Err(invalid("Unsupported project record version"));
    }
    let relative = |p: &Path| {
        !p.as_os_str().is_empty()
            && p.components()
                .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
    };
    if !relative(&record.chart) || record.assets.as_deref().is_some_and(|p| !relative(p)) {
        return Err(invalid("Invalid path in project record"));
    }
    checked_location(
        workspace,
        ProjectLocation {
            chart: workspace.join(&record.chart),
            assets: record.assets.as_ref().map(|p| workspace.join(p)),
        },
    )?;
    Ok(Some(record))
}

fn checked_cover(workspace: &Path, assets: &Path, relative: &Path) -> Option<PathBuf> {
    let relative = crate::package::relative(relative.to_str()?).ok()?;
    let path = assets.join(relative).canonicalize().ok()?;
    (path.is_file() && path.starts_with(assets) && path.starts_with(workspace)).then_some(path)
}
fn discover_cover(workspace: &Path, chart: &Path, assets: &Path) -> Option<PathBuf> {
    let options = crate::load_project_options_with_assets(chart, Some(assets)).ok()?;
    checked_cover(
        workspace,
        assets,
        Path::new(options.illustration.as_deref()?),
    )
}

pub fn list_projects(root: &Path) -> Result<ProjectLibrary, ChartError> {
    let mut library = ProjectLibrary::default();
    let projects = projects_root(root)?;
    for (index, entry) in fs::read_dir(&projects)?.enumerate() {
        if index >= 2048 {
            library
                .issues
                .push("Only the first 2048 project folders were scanned".into());
            break;
        }
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let workspace = checked_workspace(root, &entry.path())?;
        let folder = entry.file_name().to_string_lossy().into_owned();
        let edit_time_ms = match crate::project_edit_time(root, &workspace) {
            Ok(ms) => ms,
            Err(error) => {
                library.issues.push(format!("{folder}: {error}"));
                0
            }
        };
        match read_record(root, &workspace) {
            Ok(Some(mut record)) => {
                if record.events.is_none() {
                    // Migrate a pre-count record on the library worker, retaining
                    // its name, source and last-opened metadata.
                    match load_chart(&workspace.join(&record.chart)) {
                        Ok(document) => {
                            record.events = Some(document.event_count());
                            record.notes = document.note_count();
                            if let Err(error) = write_json(&record_path(root, &workspace), &record)
                            {
                                library.issues.push(format!("{folder}: {error}"));
                            }
                        }
                        Err(error) => library.issues.push(format!("{folder}: {error}")),
                    }
                }
                let assets = record
                    .assets
                    .as_ref()
                    .map_or_else(|| workspace.clone(), |p| workspace.join(p));
                let cover = record
                    .cover
                    .as_deref()
                    .and_then(|p| checked_cover(&workspace, &workspace, p))
                    .or_else(|| {
                        discover_cover(&workspace, &workspace.join(&record.chart), &assets)
                    });
                library.projects.push(ProjectSummary {
                    cover,
                    workspace,
                    name: record.display_name.unwrap_or(record.name),
                    source_name: record.source_name,
                    notes: record.notes,
                    events: record.events,
                    edit_time_ms,
                    last_opened_ms: record.last_opened_ms,
                });
                continue;
            }
            Ok(None) => {}
            Err(error) => library.issues.push(format!("{folder}: {error}")),
        }
        match inspect_workspace(&workspace, &folder) {
            Ok(project) => {
                let choice = project.suggested.and_then(|i| project.charts.get(i));
                let cover = choice
                    .or_else(|| project.charts.first())
                    .and_then(|chart| discover_cover(&workspace, &chart.path, &project.assets));
                library.projects.push(ProjectSummary {
                    workspace,
                    cover,
                    name: choice.map_or_else(|| project.name.clone(), |c| c.name.clone()),
                    source_name: folder,
                    notes: project.charts.iter().map(|c| c.notes).sum(),
                    events: Some(project.charts.iter().map(|c| c.events).sum()),
                    edit_time_ms,
                    last_opened_ms: 0,
                });
            }
            Err(error) => library.issues.push(format!("{folder}: {error}")),
        }
    }
    library.projects.sort_by(|a, b| {
        b.last_opened_ms
            .cmp(&a.last_opened_ms)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.workspace.cmp(&b.workspace))
    });
    Ok(library)
}

pub fn open_project(root: &Path, workspace: &Path) -> Result<ImportedProject, ChartError> {
    let workspace = checked_workspace(root, workspace)?;
    let record = read_record(root, &workspace).ok().flatten();
    let name = record.as_ref().map_or_else(
        || {
            workspace
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        },
        |r| r.display_name.clone().unwrap_or_else(|| r.name.clone()),
    );
    let mut project = inspect_workspace(&workspace, &name)?;
    if let Some(record) = record {
        let selected = workspace.join(&record.chart);
        if let Some(index) = project.charts.iter().position(|c| c.path == selected) {
            project.suggested = Some(index);
            project.assets = record
                .assets
                .map_or_else(|| workspace.clone(), |p| workspace.join(p));
        }
    }
    Ok(project)
}

pub(crate) fn record_opened_project(
    root: &Path,
    location: &ProjectLocation,
) -> Result<(), ChartError> {
    let projects = projects_root(root)?;
    let Ok(relative) = location.chart.strip_prefix(&projects) else {
        return Ok(());
    };
    let Some(folder) = relative.components().next() else {
        return Ok(());
    };
    let workspace = checked_workspace(root, &projects.join(folder))?;
    let location = checked_location(&workspace, location.clone())?;
    let document = load_chart(&location.chart)?;
    let source_name = crate::package::workspace_source_name(&workspace)?.unwrap_or_else(|| {
        read_record(root, &workspace).ok().flatten().map_or_else(
            || {
                workspace
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            },
            |r| r.source_name,
        )
    });
    let options =
        crate::load_project_options_with_assets(&location.chart, location.assets.as_deref())?;
    let cover = location
        .assets
        .as_deref()
        .and_then(|assets| {
            options
                .illustration
                .as_deref()
                .and_then(|path| checked_cover(&workspace, assets, Path::new(path)))
        })
        .and_then(|path| path.strip_prefix(&workspace).ok().map(Path::to_owned));
    let display_name = read_record(root, &workspace)
        .ok()
        .flatten()
        .and_then(|r| r.display_name);
    let record = Record {
        display_name,
        cover,
        version: 1,
        name: options.display_field(&document, "name").into(),
        source_name,
        chart: location.chart.strip_prefix(&workspace).unwrap().to_owned(),
        assets: location.assets.map(|p| {
            let relative = p.strip_prefix(&workspace).unwrap();
            if relative.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                relative.to_owned()
            }
        }),
        notes: document.note_count(),
        events: Some(document.event_count()),
        last_opened_ms: now_ms(),
    };
    let directory = root.join("library");
    fs::create_dir_all(&directory)?;
    if !directory.canonicalize()?.starts_with(root.canonicalize()?) {
        return Err(invalid("Library folder is outside private storage"));
    }
    write_json(&record_path(root, &workspace), &record)
}

pub fn create_project(
    root: &Path,
    document: &ChartDocument,
) -> Result<ProjectLocation, ChartError> {
    let projects = projects_root(root)?;
    let staged = tempfile::Builder::new()
        .prefix("project-")
        .tempdir_in(projects)?;
    save_chart(document, &staged.path().join("chart.json"))?;
    let workspace = staged.keep();
    Ok(ProjectLocation {
        chart: workspace.join("chart.json"),
        assets: Some(workspace),
    })
}

pub fn rename_project(root: &Path, workspace: &Path, name: &str) -> Result<(), ChartError> {
    if name.trim().is_empty() || name.len() > 512 {
        return Err(invalid("Use a project name of 1–512 bytes"));
    }
    let workspace = checked_workspace(root, workspace)?;
    if read_record(root, &workspace)?.is_none() {
        let project = open_project(root, &workspace)?;
        let chart = project
            .charts
            .get(project.suggested.unwrap_or(0))
            .ok_or_else(|| invalid("Project has no chart"))?;
        record_opened_project(
            root,
            &ProjectLocation {
                chart: chart.path.clone(),
                assets: Some(project.assets),
            },
        )?;
    }
    let mut record =
        read_record(root, &workspace)?.ok_or_else(|| invalid("Project record is missing"))?;
    record.display_name = Some(name.trim().into());
    write_json(&record_path(root, &workspace), &record)
}

pub fn refresh_project_record(root: &Path, location: &ProjectLocation) -> Result<(), ChartError> {
    record_opened_project(root, location)
}
