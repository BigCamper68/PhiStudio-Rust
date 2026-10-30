use super::*;

pub(crate) fn describe_created_project(
    workspace: &Path,
    document: &ChartDocument,
    music: &str,
    illustration: &str,
) -> Result<(), ChartError> {
    let mut options = ProjectOptions {
        music: Some(music.into()),
        illustration: Some(illustration.into()),
        ..Default::default()
    };
    for key in ["name", "composer", "charter", "level"] {
        let text = document.as_json()["META"][key]
            .as_str()
            .unwrap_or("")
            .to_owned();
        options.authored.insert(key.into(), text.clone());
        options.display.insert(key.into(), text);
    }
    let description = Description {
        phistudio_workspace: 1,
        source_name: format!("{}.pez", document.name()),
        assets: PathBuf::from("."),
        charts: vec![StoredChart {
            source: PathBuf::from("chart.json"),
            working: PathBuf::from("chart.json"),
            format: ChartFormat::Rpe,
            options,
        }],
        suggested: Some(0),
    };
    crate::storage::write_json(&workspace.join(DESCRIPTION), &description)
}

pub(crate) fn active_root(
    workspace: &Path,
    location: &crate::ProjectLocation,
) -> Result<PathBuf, ChartError> {
    let workspace = workspace.canonicalize()?;
    let chart = location.chart.canonicalize()?;
    for ancestor in chart
        .ancestors()
        .skip(1)
        .take_while(|p| p.starts_with(&workspace))
    {
        if let Some(description) = read_description(ancestor)?
            && description
                .charts
                .iter()
                .any(|c| ancestor.join(&c.working) == chart)
        {
            return Ok(ancestor.to_owned());
        }
    }
    Ok(workspace)
}

/// Validate a complete new generation before publishing its single descriptor pointer.
pub(crate) fn generation_description(
    generation: &Path,
    chart: &Path,
    assets: &Path,
    options: ProjectOptions,
) -> Result<(), ChartError> {
    let mut description = read_description(generation)?.unwrap_or_else(|| Description {
        phistudio_workspace: 1,
        source_name: "Version.pez".into(),
        assets: assets.to_owned(),
        charts: vec![StoredChart {
            source: chart.to_owned(),
            working: chart.to_owned(),
            format: ChartFormat::Rpe,
            options: options.clone(),
        }],
        suggested: Some(0),
    });
    let index = description
        .charts
        .iter()
        .position(|c| c.working == chart)
        .ok_or_else(|| invalid("Version chart is not in its project description"))?;
    description.charts[index].options = options;
    description.assets = assets.to_owned();
    description.suggested = Some(index);
    described_project(generation, &description)?;
    crate::storage::write_json(
        &generation.join(DESCRIPTION),
        &preserved_description(generation, &description)?,
    )
}

pub(super) fn preserved_description(
    folder: &Path,
    description: &Description,
) -> Result<serde_json::Value, ChartError> {
    fn overlay(raw: &mut serde_json::Value, known: serde_json::Value, level: u8) {
        if let (Some(a), Some(b)) = (raw.as_object_mut(), known.as_object()) {
            for (k, v) in b {
                if level == 0 && k == "charts" {
                    if let Some(old) = a.get_mut(k).and_then(serde_json::Value::as_array_mut)
                        && let Some(values) = v.as_array()
                        && old.len() == values.len()
                    {
                        for (old, value) in old.iter_mut().zip(values) {
                            overlay(old, value.clone(), 1);
                        }
                        continue;
                    }
                } else if level == 1
                    && k == "options"
                    && let Some(old) = a.get_mut(k)
                {
                    overlay(old, v.clone(), 2);
                    continue;
                }
                a.insert(k.clone(), v.clone());
            }
        } else {
            *raw = known;
        }
    }
    let path = folder.join(DESCRIPTION);
    let mut raw = if path.exists() {
        crate::json_io::read(File::open(path)?.take(8 * MIB + 1))?
    } else {
        serde_json::Value::Null
    };
    overlay(&mut raw, serde_json::to_value(description)?, 0);
    Ok(raw)
}

pub(crate) fn publish_generation(
    workspace: &Path,
    generation: &Path,
) -> Result<ImportedProject, ChartError> {
    let mut description =
        read_description(generation)?.ok_or_else(|| invalid("Missing generation description"))?;
    described_project(generation, &description)?;
    let prefix = generation
        .strip_prefix(workspace)
        .map_err(|_| invalid("Generation is outside its project"))?;
    description.assets = prefix.join(description.assets);
    for chart in &mut description.charts {
        chart.source = prefix.join(&chart.source);
        chart.working = prefix.join(&chart.working);
    }
    let project = described_project(workspace, &description)?;
    crate::storage::write_json(
        &workspace.join(DESCRIPTION),
        &preserved_description(generation, &description)?,
    )?;
    Ok(project)
}
