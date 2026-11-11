use super::*;

pub(super) fn read_description(workspace: &Path) -> Result<Option<Description>, ChartError> {
    let path = workspace.join(DESCRIPTION);
    let meta = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if meta.file_type().is_symlink() || !meta.is_file() || meta.len() > 8 * MIB {
        return Err(invalid("Invalid project description file"));
    }
    let value: serde_json::Value = crate::json_io::read(File::open(path)?)?;
    if value.get("phistudio_workspace").is_none() {
        return Ok(None);
    }
    let description: Description = serde_json::from_value(value)?;
    if description.phistudio_workspace != 1
        || description.charts.is_empty()
        || description.charts.len() > 1024
        || description
            .suggested
            .is_some_and(|i| i >= description.charts.len())
    {
        return Err(invalid("Unsupported or invalid project description"));
    }
    Ok(Some(description))
}

pub(super) fn described_project(
    workspace: &Path,
    description: &Description,
) -> Result<ImportedProject, ChartError> {
    let assets = safe_inside(workspace, &description.assets, true)?;
    let mut charts = Vec::new();
    for stored in &description.charts {
        let path = safe_inside(workspace, &stored.working, false)?;
        let source = safe_inside(workspace, &stored.source, false)?;
        let document = load_chart(&path)?;
        charts.push(ChartChoice {
            name: stored.options.display_field(&document, "name").into(),
            notes: document.note_count(),
            events: document.event_count(),
            path,
            format: stored.format,
            source,
        });
    }
    let name = description.suggested.map_or_else(
        || description.source_name.clone(),
        |i| charts[i].name.clone(),
    );
    Ok(ImportedProject {
        workspace: workspace.to_owned(),
        assets,
        name,
        charts,
        suggested: description.suggested,
    })
}

/// Load runtime package settings without rewriting authored chart META.
pub fn load_project_options(chart: &Path) -> Result<ProjectOptions, ChartError> {
    load_project_options_with_assets(chart, None)
}

pub fn load_project_options_with_assets(
    chart: &Path,
    asset_root: Option<&Path>,
) -> Result<ProjectOptions, ChartError> {
    let chart = chart.canonicalize()?;
    for ancestor in chart.ancestors().skip(1).take(66) {
        let Some(description) = read_description(ancestor)? else {
            continue;
        };
        for stored in &description.charts {
            if safe_inside(ancestor, &stored.working, false)? == chart {
                for asset in [&stored.options.music, &stored.options.illustration]
                    .into_iter()
                    .flatten()
                {
                    relative(asset)?;
                }
                safe_inside(ancestor, &description.assets, true)?;
                return Ok(stored.options.clone());
            }
        }
    }
    if let Some(root) = asset_root {
        let root = root.canonicalize()?;
        let files = inventory(&root)?;
        let manifests = manifest::read(&root, &files)?;
        let relative_chart = chart
            .strip_prefix(&root)
            .unwrap_or_else(|_| Path::new(chart.file_name().unwrap()));
        let active = manifest::relevant(&manifests, Path::new(""), relative_chart);
        let document = load_chart(&chart)?;
        return manifest::options(
            &active,
            &files,
            relative_chart,
            Path::new(""),
            &document,
            chart
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or("Chart"),
        );
    }
    Ok(ProjectOptions::default())
}

pub(crate) fn workspace_source_name(workspace: &Path) -> Result<Option<String>, ChartError> {
    Ok(read_description(workspace)?.map(|description| description.source_name))
}

pub(crate) fn inspect_workspace(
    workspace: &Path,
    name: &str,
) -> Result<ImportedProject, ChartError> {
    if let Some(description) = read_description(workspace)? {
        return described_project(workspace, &description);
    }
    let mut files = inventory(workspace)?;
    // Earlier Rust imports are flat RPE workspaces. Leave their saved paths intact.
    files.sort();
    let common = common_directory(&files)?;
    let manifests = manifest::read(workspace, &files)?;
    let mut charts = Vec::new();
    let mut hints = HashSet::new();
    for manifest in &manifests {
        if let Some(value) = manifest.get("chart") {
            hints.insert(manifest.resolve(value)?);
        }
    }
    for path in &files {
        let mut prefix = Vec::new();
        File::open(workspace.join(path))?
            .take(64 * 1024)
            .read_to_end(&mut prefix)?;
        if !crate::conversion::looks_like_chart(&prefix) {
            continue;
        }
        if let Ok(chart) = load_chart(&workspace.join(path)) {
            charts.push(ChartChoice {
                path: workspace.join(path),
                source: workspace.join(path),
                name: chart.name().into(),
                notes: chart.note_count(),
                events: chart.event_count(),
                format: ChartFormat::Rpe,
            });
        }
    }
    if charts.is_empty() {
        return Err(invalid(
            "No saved RPE chart found; import the original JSON, PEC, ZIP or PEZ package again",
        ));
    }
    let matching: Vec<_> = charts
        .iter()
        .enumerate()
        .filter(|(_, c)| hints.contains(c.path.strip_prefix(workspace).unwrap()))
        .map(|(i, _)| i)
        .collect();
    let suggested = if charts.len() == 1 {
        Some(0)
    } else if matching.len() == 1 {
        Some(matching[0])
    } else {
        None
    };
    Ok(ImportedProject {
        workspace: workspace.to_owned(),
        assets: workspace.join(common),
        name: name.into(),
        charts,
        suggested,
    })
}

pub(crate) fn export_chart_path(
    chart: &Path,
    assets: &Path,
) -> Result<Option<PathBuf>, ChartError> {
    let chart = chart.canonicalize()?;
    for ancestor in chart.ancestors().skip(1).take(66) {
        if let Some(description) = read_description(ancestor)? {
            for stored in description.charts {
                if stored.format == ChartFormat::Rpe
                    && safe_inside(ancestor, &stored.working, false)? == chart
                {
                    let source = safe_inside(ancestor, &stored.source, false)?;
                    return Ok(source.strip_prefix(assets).ok().map(Path::to_owned));
                }
            }
        }
    }
    Ok(chart.strip_prefix(assets).ok().map(Path::to_owned))
}
