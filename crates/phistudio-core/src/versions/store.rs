use super::*;

pub(crate) fn folder(root: &Path, workspace: &Path) -> Result<PathBuf, ChartError> {
    let workspace = library::checked_workspace(root, workspace)?;
    let folder = root.join("versions").join(workspace.file_name().unwrap());
    fs::create_dir_all(&folder)?;
    if !folder.canonicalize()?.starts_with(root.canonicalize()?) {
        return Err(invalid("Version library is outside private storage"));
    }
    Ok(folder)
}

pub(super) fn directory(root: &Path, workspace: &Path, id: &str) -> Result<PathBuf, ChartError> {
    if !id.starts_with("version-") || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
        return Err(invalid("Invalid version ID"));
    }
    let parent = folder(root, workspace)?;
    let path = parent.join(id);
    let m = fs::symlink_metadata(&path)?;
    if !m.is_dir() || m.file_type().is_symlink() {
        return Err(invalid("Invalid version directory"));
    }
    Ok(path)
}

pub fn load(root: &Path, workspace: &Path, id: &str) -> Result<Version, ChartError> {
    let dir = directory(root, workspace, id)?;
    let path = dir.join("version.json");
    if fs::symlink_metadata(&path)?.file_type().is_symlink() {
        return Err(invalid("Invalid version metadata"));
    }
    let mut bytes = vec![];
    fs::File::open(path)?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(invalid("Version metadata exceeds 4 MiB"));
    }
    let v: Version = serde_json::from_slice(&bytes)?;
    if ![1, 2].contains(&v.version)
        || v.id != id
        || v.files.len() > 2049
        || v.parents.len() > 2
        || (v.version == 2 && (v.files.len() != v.blobs.len() || v.folders.len() > 65536))
    {
        return Err(invalid("Invalid version metadata"));
    }
    package::relative(&v.chart.to_string_lossy())?;
    package::relative(&v.assets.to_string_lossy())?;
    for name in v.files.keys().chain(v.folders.iter()) {
        package::relative(name)?;
    }
    if v.version == 2
        && (v.files.keys().any(|k| !v.blobs.contains_key(k))
            || v.blobs.values().any(|h| !objects::valid_hash(h)))
    {
        return Err(invalid("Invalid version objects"));
    }
    Ok(v)
}

pub fn list(root: &Path, workspace: &Path) -> Result<Vec<Version>, ChartError> {
    let mut versions = vec![];
    for (scanned, entry) in fs::read_dir(folder(root, workspace)?)?.enumerate() {
        if scanned >= 65536 {
            return Err(invalid("Repository directory exceeds 65536 entries"));
        }
        let entry = entry?;
        if let Some(id) = entry.file_name().to_str()
            && let Ok(version) = load(root, workspace, id)
        {
            versions.push(version);
            if versions.len() > 8192 {
                return Err(invalid("Repository history exceeds 8192 entries"));
            }
        }
    }
    versions.sort_by(|a, b| {
        b.created_ms
            .cmp(&a.created_ms)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(versions)
}

pub(crate) fn stats(doc: &ChartDocument) -> (usize, usize, usize) {
    let (mut events, mut story) = (0, 0);
    if let Some(lines) = doc.as_json()["judgeLineList"].as_array() {
        for (line_index, line) in lines.iter().enumerate() {
            if let Some(layers) = line["eventLayers"].as_array() {
                for layer in layers {
                    if let Some(map) = layer.as_object() {
                        for (key, value) in map {
                            if key.ends_with("Events") {
                                events += value.as_array().map_or(0, Vec::len);
                            }
                        }
                    }
                }
            }
            for kind in crate::StoryKind::ALL {
                story += doc.story_events(line_index, kind).map_or(0, |e| e.len());
            }
        }
    }
    (doc.note_count(), events, story)
}

pub(crate) fn transfer(
    input: &mut impl Read,
    out: &mut impl Write,
) -> Result<Fingerprint, ChartError> {
    let (mut bytes, mut hash) = (0, 0xcbf29ce484222325u64);
    let mut buf = [0u8; 65536];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        if bytes > MAX_FILE {
            return Err(invalid("Version file exceeds 256 MiB"));
        }
        for b in &buf[..n] {
            hash = (hash ^ *b as u64).wrapping_mul(0x100000001b3);
        }
        out.write_all(&buf[..n])?;
    }
    Ok(Fingerprint { bytes, hash })
}

pub fn save(
    root: &Path,
    workspace: &Path,
    name: &str,
    doc: &ChartDocument,
    location: &ProjectLocation,
    options: &ProjectOptions,
) -> Result<Version, ChartError> {
    let workspace = library::checked_workspace(root, workspace)?;
    let base = package::active_root(&workspace, location)?;
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
    let assets = if assets.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        assets
    };
    let book = crate::branches::load(root, &workspace)?;
    let parents = book.head().into_iter().map(str::to_owned).collect();
    let version = snapshot(
        root, &workspace, name, doc, &base, chart, assets, options, parents,
    )?;
    crate::branches::advance(root, &workspace, &version.id)?;
    Ok(version)
}

pub(crate) fn save_backup(
    root: &Path,
    workspace: &Path,
    name: &str,
    doc: &ChartDocument,
    location: &ProjectLocation,
    options: &ProjectOptions,
) -> Result<Version, ChartError> {
    let mut version = save(root, workspace, name, doc, location, options)?;
    version.system_name = Some(name.into());
    storage::write_json(
        &directory(root, workspace, &version.id)?.join("version.json"),
        &version,
    )?;
    Ok(version)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn snapshot(
    root: &Path,
    workspace: &Path,
    name: &str,
    doc: &ChartDocument,
    base: &Path,
    chart: PathBuf,
    assets: PathBuf,
    options: &ProjectOptions,
    parents: Vec<String>,
) -> Result<Version, ChartError> {
    if name.trim().is_empty() || name.len() > 512 {
        return Err(invalid("Use a version name of 1–512 bytes"));
    }
    let files = package::inventory(base)?;
    let stage = tempfile::Builder::new()
        .prefix("version-")
        .tempdir_in(folder(root, workspace)?)?;
    let mut fingerprints = BTreeMap::new();
    let mut blobs = BTreeMap::new();
    let mut total = 0;
    let override_bytes = serde_json::to_vec(doc.as_json())?;
    if override_bytes.len() as u64 > MAX_FILE {
        return Err(invalid("Chart exceeds 256 MiB"));
    }
    for relative in files {
        let name = relative.to_string_lossy().replace('\\', "/");
        let (fingerprint, hash) = if relative == chart {
            objects::put(root, workspace, &mut override_bytes.as_slice())?
        } else {
            objects::put(
                root,
                workspace,
                &mut fs::File::open(base.join(&relative))?.take(MAX_FILE + 1),
            )?
        };
        total += fingerprint.bytes;
        if total > MAX_TOTAL {
            return Err(invalid("Version exceeds 1032 MiB"));
        }
        blobs.insert(name.clone(), hash);
        fingerprints.insert(name, fingerprint);
    }
    let (notes, events, story) = stats(doc);
    let v = Version {
        version: 2,
        id: stage
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        name: name.trim().into(),
        system_name: None,
        parents,
        created_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        chart,
        assets,
        options: options.clone(),
        notes,
        events,
        story,
        files: fingerprints,
        blobs,
        folders: objects::folders(base)?,
        author: Author::default(),
        kind: "legacy".into(),
    };
    objects::check_metadata(&v)?;
    storage::write_json(&stage.path().join("version.json"), &v)?;
    let _ = stage.keep();
    Ok(v)
}

pub(crate) fn expand(
    root: &Path,
    workspace: &Path,
    v: &Version,
    destination: &Path,
) -> Result<(), ChartError> {
    if v.version == 2 {
        return objects::expand(root, workspace, v, destination);
    }
    let archive_path = directory(root, workspace, &v.id)?.join("workspace.zip");
    if fs::symlink_metadata(&archive_path)?
        .file_type()
        .is_symlink()
    {
        return Err(invalid("Invalid version archive"));
    }
    let mut archive =
        zip::ZipArchive::new(fs::File::open(archive_path)?).map_err(|e| invalid(e.to_string()))?;
    if archive.len() > 2049 {
        return Err(invalid("Version contains too many files"));
    }
    let mut seen = BTreeSet::new();
    let mut total = 0;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| invalid(e.to_string()))?;
        let relative = package::relative(entry.name())?;
        if relative.as_os_str().is_empty()
            || !seen.insert(relative.clone())
            || entry.is_dir()
            || entry.size() > MAX_FILE
            || entry
                .unix_mode()
                .is_some_and(|m| ![0, 0o100000].contains(&(m & 0o170000)))
        {
            return Err(invalid("Invalid file in version archive"));
        }
        let target = destination.join(&relative);
        fs::create_dir_all(target.parent().unwrap())?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        let fingerprint = transfer(&mut (&mut entry).take(MAX_FILE + 1), &mut file)?;
        file.sync_all()?;
        total += fingerprint.bytes;
        if total > MAX_TOTAL
            || v.files.get(&relative.to_string_lossy().replace('\\', "/")) != Some(&fingerprint)
        {
            return Err(invalid("Version data does not match its manifest"));
        }
    }
    if seen.len() != v.files.len() {
        return Err(invalid("Version is missing files"));
    }
    Ok(())
}
