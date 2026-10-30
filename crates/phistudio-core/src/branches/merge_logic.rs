use super::*;

pub(super) fn pointer(path: &str, key: &str) -> String {
    format!("{}/{}", path, key.replace('~', "~0").replace('/', "~1"))
}

pub(super) fn stable_lines(a: &[Value], b: &[Value], c: &[Value]) -> bool {
    if a.len() != b.len() || a.len() != c.len() {
        return false;
    }
    let names: Option<Vec<_>> = a.iter().map(|v| v["Name"].as_str()).collect();
    let Some(names) = names else {
        return false;
    };
    if names.iter().collect::<BTreeSet<_>>().len() != names.len() {
        return false;
    }
    a.iter()
        .zip(b)
        .zip(c)
        .all(|((x, y), z)| x["Name"] == y["Name"] && x["Name"] == z["Name"])
}

pub(crate) fn merge(
    base: Option<&Value>,
    current: Option<&Value>,
    incoming: Option<&Value>,
    path: &str,
    choices: &BTreeMap<String, Side>,
    conflicts: &mut Vec<Conflict>,
    depth: usize,
) -> Result<Option<Value>, ChartError> {
    if depth > 64 {
        return Err(invalid("Merge data is nested too deeply"));
    }
    if current == incoming || incoming == base {
        return Ok(current.cloned());
    }
    if current == base {
        return Ok(incoming.cloned());
    }
    if let (Some(Value::Object(a)), Some(Value::Object(b)), Some(Value::Object(c))) =
        (base, current, incoming)
    {
        let mut output = serde_json::Map::new();
        let mut seen = BTreeSet::new();
        let line_path: Vec<_> = path.split('/').collect();
        let recount = line_path.len() == 4
            && line_path[1..3] == ["chart", "judgeLineList"]
            && [a, b, c]
                .iter()
                .all(|m| m.get("notes").is_some_and(Value::is_array))
            && (a.get("notes") != b.get("notes") || a.get("notes") != c.get("notes"));
        for key in b.keys().chain(c.keys()).chain(a.keys()) {
            if !seen.insert(key) || recount && key == "numOfNotes" {
                continue;
            }
            if let Some(v) = merge(
                a.get(key),
                b.get(key),
                c.get(key),
                &pointer(path, key),
                choices,
                conflicts,
                depth + 1,
            )? {
                output.insert(key.clone(), v);
            }
        }
        if recount {
            output.insert(
                "numOfNotes".into(),
                json!(output["notes"].as_array().unwrap().len()),
            );
        }
        return Ok(Some(Value::Object(output)));
    }
    if let (Some(Value::Array(a)), Some(Value::Array(b)), Some(Value::Array(c))) =
        (base, current, incoming)
        && super::merge_tracks::is_track(path)
        && let Some(values) =
            super::merge_tracks::merge_track(a, b, c, path, choices, conflicts, depth)?
    {
        return Ok(Some(Value::Array(values)));
    }
    if let (Some(Value::Array(a)), Some(Value::Array(b)), Some(Value::Array(c))) =
        (base, current, incoming)
        && ((path == "/chart/judgeLineList" && stable_lines(a, b, c))
            || (path.ends_with("/eventLayers") && a.len() == b.len() && a.len() == c.len()))
    {
        let mut values = vec![];
        for i in 0..a.len() {
            values.push(
                merge(
                    a.get(i),
                    b.get(i),
                    c.get(i),
                    &pointer(path, &i.to_string()),
                    choices,
                    conflicts,
                    depth + 1,
                )?
                .unwrap_or(Value::Null),
            );
        }
        return Ok(Some(Value::Array(values)));
    }
    if conflicts.len() >= 512 {
        return Err(invalid(
            "Merge has more than 512 conflicts; merge smaller changes",
        ));
    }
    let choice = choices.get(path).copied();
    conflicts.push(Conflict {
        path: path.into(),
        base: base.cloned(),
        current: current.cloned(),
        incoming: incoming.cloned(),
        choice,
    });
    Ok(match choice {
        Some(Side::Incoming) => incoming.cloned(),
        _ => current.cloned(),
    })
}

pub(super) fn ancestors(
    root: &Path,
    workspace: &Path,
    id: &str,
) -> Result<BTreeMap<String, usize>, ChartError> {
    let mut found = BTreeMap::new();
    let mut pending = VecDeque::from([(id.to_owned(), 0)]);
    while let Some((id, depth)) = pending.pop_front() {
        if found.contains_key(&id) {
            continue;
        }
        if found.len() >= 1024 {
            return Err(invalid("Version ancestry exceeds 1024 entries"));
        }
        let v = versions::load(root, workspace, &id)?;
        found.insert(id, depth);
        for parent in v.parents {
            pending.push_back((parent, depth + 1));
        }
    }
    Ok(found)
}

pub(super) fn common(
    root: &Path,
    workspace: &Path,
    a: &str,
    b: &str,
) -> Result<String, ChartError> {
    let aa = ancestors(root, workspace, a)?;
    let bb = ancestors(root, workspace, b)?;
    aa.keys()
        .filter(|id| bb.contains_key(*id))
        .min_by_key(|id| (aa[*id] + bb[*id], aa[*id].max(bb[*id]), *id))
        .cloned()
        .ok_or_else(|| invalid("Branches have no common ancestor"))
}

pub(super) fn envelope(
    doc: &ChartDocument,
    options: &ProjectOptions,
    base: &Path,
    chart: &Path,
) -> Result<Value, ChartError> {
    let mut files = BTreeMap::new();
    for relative in package::inventory(base)? {
        if relative == chart || relative == Path::new(".phistudio-workspace.json") {
            continue;
        }
        let fp = versions::transfer(
            &mut fs::File::open(base.join(&relative))?,
            &mut std::io::sink(),
        )?;
        files.insert(relative.to_string_lossy().replace('\\', "/"), fp);
    }
    let description = base.join(".phistudio-workspace.json");
    let project = if description.exists() {
        read::<Value>(&description, 4 * 1024 * 1024)?
    } else {
        Value::Null
    };
    Ok(json!({"chart":doc.as_json(),"options":options,"project":project,"files":files}))
}

pub fn prepare_merge(
    root: &Path,
    workspace: &Path,
    incoming: &str,
    doc: &ChartDocument,
    location: &ProjectLocation,
    options: &ProjectOptions,
) -> Result<MergePlan, ChartError> {
    let workspace = crate::library::checked_workspace(root, workspace)?;
    let book = load(root, &workspace)?;
    if incoming == book.current {
        return Err(invalid("Choose a different branch to merge"));
    }
    let head = book
        .head()
        .ok_or_else(|| invalid("Save a version before merging branches"))?;
    let incoming_id = book
        .heads
        .get(incoming)
        .and_then(|v| v.as_deref())
        .ok_or_else(|| invalid("Branch no longer exists"))?;
    let base_id = common(root, &workspace, head, incoming_id)?;
    let incoming_version = versions::load(root, &workspace, incoming_id)?;
    let base_version = versions::load(root, &workspace, &base_id)?;
    let active = package::active_root(&workspace, location)?;
    let chart = location
        .chart
        .canonicalize()?
        .strip_prefix(&active)
        .map_err(|_| invalid("Chart is outside its project"))?
        .to_owned();
    let assets = location
        .assets
        .as_deref()
        .unwrap_or(&active)
        .canonicalize()?
        .strip_prefix(&active)
        .map_err(|_| invalid("Assets are outside their project"))?
        .to_owned();
    let assets = if assets.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        assets
    };
    if incoming_version.chart != chart
        || incoming_version.assets != assets
        || base_version.chart != chart
        || base_version.assets != assets
    {
        return Err(invalid(
            "Branch project layouts differ; restore or duplicate a version instead",
        ));
    }
    let source = stage(root)?;
    versions::expand(root, &workspace, &incoming_version, source.path())?;
    let base_dir = stage(root)?;
    versions::expand(root, &workspace, &base_version, base_dir.path())?;
    let ours = envelope(doc, options, &active, &chart)?;
    let base_doc = crate::load_chart(&base_dir.path().join(&chart))?;
    let theirs_doc = crate::load_chart(&source.path().join(&chart))?;
    let base = envelope(&base_doc, &base_version.options, base_dir.path(), &chart)?;
    let theirs = envelope(
        &theirs_doc,
        &incoming_version.options,
        source.path(),
        &chart,
    )?;
    let mut conflicts = vec![];
    let merged = merge(
        Some(&base),
        Some(&ours),
        Some(&theirs),
        "",
        &BTreeMap::new(),
        &mut conflicts,
        0,
    )?
    .unwrap();
    Ok(MergePlan {
        current_branch: book.current.clone(),
        incoming_branch: incoming.into(),
        base_id,
        conflicts,
        book,
        base,
        ours,
        theirs,
        merged,
        incoming: incoming_version,
        source,
        chart,
        assets,
    })
}

pub fn apply_merge(
    root: &Path,
    workspace: &Path,
    plan: MergePlan,
    name: &str,
    doc: &ChartDocument,
    location: &ProjectLocation,
    options: &ProjectOptions,
) -> Result<ImportedProject, ChartError> {
    let workspace = crate::library::checked_workspace(root, workspace)?;
    if load(root, &workspace)? != plan.book {
        return Err(invalid("Branches changed; prepare the merge again"));
    }
    let active = package::active_root(&workspace, location)?;
    if envelope(doc, options, &active, &plan.chart)? != plan.ours {
        return Err(invalid("Project changed; prepare the merge again"));
    }
    let merged = plan.resolve()?;
    let result = ChartDocument::from_value(merged["chart"].clone())?;
    let merged_options: ProjectOptions = serde_json::from_value(merged["options"].clone())?;
    let stage = stage(root)?;
    let files: BTreeMap<String, Fingerprint> = serde_json::from_value(merged["files"].clone())?;
    let mut total = 0;
    for (name, fp) in files {
        let relative = package::relative(&name)?;
        let source = if plan.ours["files"].get(&name) == Some(&serde_json::to_value(&fp)?) {
            active.join(&relative)
        } else {
            plan.source.path().join(&relative)
        };
        let target = stage.path().join(&relative);
        fs::create_dir_all(target.parent().unwrap())?;
        let copied =
            versions::transfer(&mut fs::File::open(source)?, &mut fs::File::create(target)?)?;
        total += copied.bytes;
        if copied != fp || total > 1032 * 1024 * 1024 {
            return Err(invalid("Merge resources changed or exceed the size limit"));
        }
    }
    let target = stage.path().join(&plan.chart);
    fs::create_dir_all(target.parent().unwrap())?;
    crate::save_chart(&result, &target)?;
    if merged["project"].is_object() {
        storage::write_json(
            &stage.path().join(".phistudio-workspace.json"),
            &merged["project"],
        )?;
    }
    package::generation_description(
        stage.path(),
        &plan.chart,
        &plan.assets,
        merged_options.clone(),
    )?;
    let previous = versions::save_backup(root, &workspace, "Before merge", doc, location, options)?;
    let version = versions::snapshot(
        root,
        &workspace,
        name,
        &result,
        stage.path(),
        plan.chart,
        plan.assets,
        &merged_options,
        vec![previous.id, plan.incoming.id],
    )?;
    let before = load(root, &workspace)?;
    let mut after = before.clone();
    after.heads.insert(after.current.clone(), Some(version.id));
    let generation = workspace.join(stage.path().file_name().unwrap());
    fs::rename(stage.path(), &generation)?;
    publish(root, &workspace, &generation, before, after)
}
