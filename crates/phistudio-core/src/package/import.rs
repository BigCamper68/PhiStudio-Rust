use super::*;

pub fn import_project(
    source: &Path,
    projects: &Path,
    display_name: &str,
    package: bool,
) -> Result<ImportedProject, ChartError> {
    fs::create_dir_all(projects)?;
    let staged = tempfile::Builder::new()
        .prefix("project-")
        .tempdir_in(projects)?;
    let payload = staged.path().join("source");
    fs::create_dir_all(&payload)?;
    let mut files = Vec::new();
    if package {
        if source.metadata()?.len() > 256 * MIB {
            return Err(invalid("ZIP exceeds 256 MiB"));
        }
        let mut archive =
            zip::ZipArchive::new(File::open(source)?).map_err(|e| invalid(e.to_string()))?;
        if archive.len() > 1024 {
            return Err(invalid("ZIP contains more than 1024 entries"));
        }
        let mut names = HashSet::new();
        let mut total = 0;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|e| invalid(e.to_string()))?;
            let path = relative(entry.name())?;
            if path.as_os_str().is_empty() || !names.insert(path.clone()) {
                return Err(invalid("Duplicate path in ZIP"));
            }
            if entry
                .unix_mode()
                .is_some_and(|m| ![0, 0o100000, 0o040000].contains(&(m & 0o170000)))
            {
                return Err(invalid("ZIP symlinks and special files are not supported"));
            }
            let target = payload.join(&path);
            if entry.is_dir() {
                fs::create_dir_all(target)?;
                continue;
            }
            if entry.size() > 128 * MIB || entry.compressed_size() > 128 * MIB {
                return Err(invalid("ZIP entry exceeds 128 MiB"));
            }
            if total + entry.size() > 512 * MIB {
                return Err(invalid("Extracted package exceeds 512 MiB"));
            }
            fs::create_dir_all(target.parent().unwrap())?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            let copied = io::copy(
                &mut (&mut entry).take((128 * MIB).min(512 * MIB - total) + 1),
                &mut output,
            )?;
            if copied > 128 * MIB || total + copied > 512 * MIB {
                return Err(invalid("Package exceeds the extraction limit"));
            }
            total += copied;
            files.push(path);
        }
    } else {
        let mut input = File::open(source)?.take(ChartDocument::MAX_JSON_BYTES as u64 + 1);
        let mut output = File::create(payload.join("chart.data"))?;
        if io::copy(&mut input, &mut output)? > ChartDocument::MAX_JSON_BYTES as u64 {
            return Err(invalid("Chart exceeds 128 MiB"));
        }
        files.push(PathBuf::from("chart.data"));
    }
    let description = prepare_workspace(staged.path(), files, display_name)?;
    crate::storage::write_json(&staged.path().join(DESCRIPTION), &description)?;
    // Validate before publishing, including the persisted description paths.
    described_project(staged.path(), &description)?;
    let workspace = staged.keep();
    described_project(&workspace, &description)
}

pub(super) fn read_text(path: &Path) -> Result<String, ChartError> {
    let mut text = String::new();
    File::open(path)?
        .take(ChartDocument::MAX_JSON_BYTES as u64 + 1)
        .read_to_string(&mut text)?;
    if text.len() > ChartDocument::MAX_JSON_BYTES {
        return Err(invalid("Chart exceeds 128 MiB"));
    }
    Ok(text)
}

pub(super) fn prepare_workspace(
    workspace: &Path,
    mut files: Vec<PathBuf>,
    name: &str,
) -> Result<Description, ChartError> {
    files.sort();
    let common = common_directory(&files)?;
    let payload = workspace.join("source");
    let manifests = manifest::read(&payload, &files)?;
    let mut hints = HashSet::new();
    for manifest in &manifests {
        if manifest.path.parent() != Some(common.as_path()) {
            continue;
        }
        if let Some(value) = manifest.get("chart") {
            hints.insert(
                manifest
                    .resolve(value)
                    .map_err(|_| invalid(format!("Unsafe manifest chart path: {value}")))?,
            );
        }
    }
    fs::create_dir_all(workspace.join("working"))?;
    let mut charts = Vec::new();
    let mut errors = Vec::new();
    let mut total_generated = 0_u64;
    for path in &files {
        let full = payload.join(path);
        let mut prefix = Vec::new();
        File::open(&full)?
            .take(64 * 1024)
            .read_to_end(&mut prefix)?;
        if !crate::conversion::looks_like_chart(&prefix) && !(files.len() == 1 && hints.is_empty())
        {
            continue;
        }
        let decoded = read_text(&full).and_then(|s| decode_chart(&s));
        let mut decoded = match decoded {
            Ok(value) => value,
            Err(error) => {
                if hints.len() == 1 && hints.contains(path) {
                    return Err(invalid(format!("{}: {error}", path.display())));
                }
                errors.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        let active = manifest::relevant(&manifests, &common, path);
        let mut options =
            manifest::options(&active, &files, path, &common, &decoded.document, name)?;
        if decoded.format != ChartFormat::Rpe {
            let mut value = decoded.document.as_json().clone();
            for key in ["name", "composer", "charter", "level"] {
                if let Some(display) = options.display.get(key) {
                    value["META"][key] = serde_json::json!(display);
                }
            }
            if let Some(id) = active.iter().find_map(|m| m.get("id")) {
                value["META"]["id"] = serde_json::json!(id);
            }
            value["META"]["song"] = serde_json::json!(options.music.as_deref().unwrap_or(""));
            value["META"]["background"] =
                serde_json::json!(options.illustration.as_deref().unwrap_or(""));
            value["META"]["backgroundDim"] =
                serde_json::json!(options.background_dim.unwrap_or(0.6));
            decoded.document = ChartDocument::from_value(value)?;
            for key in ["name", "composer", "charter", "level"] {
                options.authored.insert(
                    key.into(),
                    decoded.document.as_json()["META"][key]
                        .as_str()
                        .unwrap_or("")
                        .into(),
                );
            }
        }
        let working = PathBuf::from(format!("working/chart-{:04}.rpe.json", charts.len()));
        save_chart(&decoded.document, &workspace.join(&working))?;
        let size = workspace.join(&working).metadata()?.len();
        total_generated += size;
        if size > 128 * MIB || total_generated > 512 * MIB {
            return Err(invalid(
                "Converted chart data exceeds the package size limit",
            ));
        }
        charts.push(StoredChart {
            source: PathBuf::from("source").join(path),
            working,
            format: decoded.format,
            options,
        });
    }
    if charts.is_empty() {
        return Err(invalid(format!(
            "No supported RPE, Phigros (v1/v3), or PEC chart was found. {}",
            errors.first().map(String::as_str).unwrap_or("")
        )));
    }
    let referenced: Vec<_> = charts
        .iter()
        .enumerate()
        .filter(|(_, c)| hints.contains(c.source.strip_prefix("source").unwrap()))
        .map(|(i, _)| i)
        .collect();
    let suggested = if referenced.len() == 1 {
        Some(referenced[0])
    } else if referenced.len() > 1 {
        None
    } else {
        let rpe: Vec<_> = charts
            .iter()
            .enumerate()
            .filter(|(_, c)| c.format == ChartFormat::Rpe)
            .map(|(i, _)| i)
            .collect();
        if rpe.len() == 1 {
            Some(rpe[0])
        } else if charts.len() == 1 {
            Some(0)
        } else {
            None
        }
    };
    Ok(Description {
        phistudio_workspace: 1,
        source_name: name.into(),
        assets: PathBuf::from("source").join(common),
        charts,
        suggested,
    })
}
