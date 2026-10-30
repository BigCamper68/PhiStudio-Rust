use super::*;
use crate::{
    package,
    versions::{Fingerprint, objects},
};
#[derive(Clone)]
pub(super) struct Tree {
    pub document: ChartDocument,
    pub options: ProjectOptions,
    pub chart: PathBuf,
    pub assets: PathBuf,
    pub files: BTreeMap<String, Fingerprint>,
    pub blobs: BTreeMap<String, String>,
    pub folders: Vec<String>,
    pub project: Value,
}
fn read_project(path: &Path) -> Result<Value, ChartError> {
    match fs::File::open(path) {
        Ok(file) => crate::json_io::read(file.take(4 * 1024 * 1024 + 1)).map_err(Into::into),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Null),
        Err(e) => Err(e.into()),
    }
}
pub(super) fn base(workspace: &Path, working: &Working<'_>) -> Result<PathBuf, ChartError> {
    package::active_root(workspace, working.location)
}
pub(super) fn capture(
    root: &Path,
    workspace: &Path,
    working: &Working<'_>,
    kind: &str,
) -> Result<Version, ChartError> {
    let base = base(workspace, working)?;
    let chart = working
        .location
        .chart
        .canonicalize()?
        .strip_prefix(&base)
        .map_err(|_| invalid("Chart is outside its project"))?
        .to_owned();
    let assets = working
        .location
        .assets
        .as_deref()
        .unwrap_or(&base)
        .canonicalize()?
        .strip_prefix(&base)
        .map_err(|_| invalid("Assets are outside their project"))?
        .to_owned();
    let mut v = versions::snapshot(
        root,
        workspace,
        kind,
        working.document,
        &base,
        chart,
        if assets.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            assets
        },
        working.options,
        vec![],
    )?;
    v.kind = kind.into();
    objects::write_version(root, workspace, &v)?;
    Ok(v)
}
pub(super) fn read(root: &Path, workspace: &Path, v: &Version) -> Result<Tree, ChartError> {
    let chart = v.chart.to_string_lossy().replace('\\', "/");
    let bytes = objects::read_file(root, workspace, v, &chart)?;
    let document = ChartDocument::parse(
        std::str::from_utf8(&bytes).map_err(|_| invalid("Invalid version chart text"))?,
    )?;
    let project = if v.files.contains_key(".phistudio-workspace.json") {
        serde_json::from_slice(&objects::read_file(
            root,
            workspace,
            v,
            ".phistudio-workspace.json",
        )?)?
    } else {
        Value::Null
    };
    let mut blobs = v.blobs.clone();
    let mut folders = v.folders.clone();
    if v.version == 1 {
        let mut dirs = BTreeSet::new();
        for name in v.files.keys() {
            let bytes = objects::read_file(root, workspace, v, name)?;
            let (_, hash) = objects::fingerprint(&mut bytes.as_slice())?;
            blobs.insert(name.clone(), hash);
            for parent in Path::new(name).ancestors().skip(1) {
                if !parent.as_os_str().is_empty() {
                    dirs.insert(parent.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        folders = dirs.into_iter().collect();
    }
    Ok(Tree {
        document,
        options: v.options.clone(),
        chart: v.chart.clone(),
        assets: v.assets.clone(),
        files: v.files.clone(),
        blobs,
        folders,
        project,
    })
}
pub(super) fn working(workspace: &Path, w: &Working<'_>) -> Result<Tree, ChartError> {
    let base = base(workspace, w)?;
    let chart = w
        .location
        .chart
        .canonicalize()?
        .strip_prefix(&base)
        .map_err(|_| invalid("Chart is outside its project"))?
        .to_owned();
    let assets = w
        .location
        .assets
        .as_deref()
        .unwrap_or(&base)
        .canonicalize()?
        .strip_prefix(&base)
        .map_err(|_| invalid("Assets are outside their project"))?
        .to_owned();
    let bytes = serde_json::to_vec(w.document.as_json())?;
    let mut files = BTreeMap::new();
    let mut blobs = BTreeMap::new();
    for file in package::inventory(&base)? {
        let (fp, hash) = if file == chart {
            objects::fingerprint(&mut bytes.as_slice())?
        } else {
            objects::fingerprint(&mut fs::File::open(base.join(&file))?)?
        };
        let name = file.to_string_lossy().replace('\\', "/");
        blobs.insert(name.clone(), hash);
        files.insert(name, fp);
    }
    Ok(Tree {
        document: w.document.clone(),
        options: w.options.clone(),
        chart,
        assets: if assets.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            assets
        },
        files,
        blobs,
        folders: objects::folders(&base)?,
        project: read_project(&base.join(".phistudio-workspace.json"))?,
    })
}
fn project_extensions(raw: &Value) -> Value {
    let mut value = raw.as_object().cloned().unwrap_or_default();
    for key in ["phistudio_workspace", "source_name", "assets", "suggested"] {
        value.remove(key);
    }
    if let Some(charts) = value.get_mut("charts").and_then(Value::as_array_mut) {
        let known = serde_json::to_value(ProjectOptions::default()).unwrap();
        for chart in charts.iter_mut() {
            if let Some(map) = chart.as_object_mut() {
                for key in ["source", "working", "format"] {
                    map.remove(key);
                }
                if let Some(options) = map.get_mut("options").and_then(Value::as_object_mut) {
                    for key in known.as_object().unwrap().keys() {
                        options.remove(key);
                    }
                }
                if map
                    .get("options")
                    .is_some_and(|v| v.as_object().is_some_and(|m| m.is_empty()))
                {
                    map.remove("options");
                }
            }
        }
        if charts
            .iter()
            .all(|v| v.as_object().is_some_and(|m| m.is_empty()))
        {
            value.remove("charts");
        }
    }
    Value::Object(value)
}
pub(super) fn envelope(t: &Tree) -> Result<Value, ChartError> {
    let chart_name = t.chart.to_string_lossy().replace('\\', "/");
    let files: BTreeMap<_, _> = t
        .files
        .iter()
        .filter(|(name, _)| {
            name.as_str() != chart_name && name.as_str() != ".phistudio-workspace.json"
        })
        .map(|(name, fp)| {
            (
                name,
                json!({"bytes":fp.bytes,"hash":fp.hash,"sha256":t.blobs.get(name)}),
            )
        })
        .collect();
    Ok(
        json!({"chart":t.document.serialization_value(),"options":t.options,"project":project_extensions(&t.project),"files":files,"folders":t.folders,"chartName":t.chart,"assets":t.assets}),
    )
}
pub(super) fn same(a: &Tree, b: &Tree) -> Result<bool, ChartError> {
    Ok(envelope(a)? == envelope(b)?)
}
pub(super) fn restore_project(raw: &Value, extensions: &Value) -> Value {
    let mut result = raw.clone();
    if !result.is_object() {
        result = json!({});
    }
    let old = project_extensions(&result);
    fn remove(target: &mut Value, old: &Value) {
        if let (Some(a), Some(b)) = (target.as_object_mut(), old.as_object()) {
            for (k, v) in b {
                if k == "charts"
                    && let (Some(ta), Some(tb)) =
                        (a.get_mut(k).and_then(Value::as_array_mut), v.as_array())
                {
                    for (x, y) in ta.iter_mut().zip(tb) {
                        remove(x, y);
                    }
                    continue;
                }
                if k == "options" {
                    if let Some(t) = a.get_mut(k) {
                        remove(t, v);
                    }
                    continue;
                }
                a.remove(k);
            }
        }
    }
    fn add(target: &mut Value, value: &Value) {
        if let (Some(a), Some(b)) = (target.as_object_mut(), value.as_object()) {
            for (k, v) in b {
                if k == "charts"
                    && let (Some(ta), Some(tb)) =
                        (a.get_mut(k).and_then(Value::as_array_mut), v.as_array())
                {
                    for (x, y) in ta.iter_mut().zip(tb) {
                        add(x, y);
                    }
                    continue;
                }
                if k == "options"
                    && let Some(t) = a.get_mut(k)
                {
                    add(t, v);
                    continue;
                }
                a.insert(k.clone(), v.clone());
            }
        }
    }
    remove(&mut result, &old);
    add(&mut result, extensions);
    result
}
pub(super) fn difference(a: &Tree, b: &Tree) -> Result<Difference, ChartError> {
    let sa = versions::stats(&a.document);
    let sb = versions::stats(&b.document);
    let mut diff = Difference {
        notes: [sa.0, sb.0],
        events: [sa.1, sb.1],
        story: [sa.2, sb.2],
        ..Default::default()
    };
    let av = envelope(a)?;
    let bv = envelope(b)?;
    let af = av["files"].as_object().unwrap();
    let bf = bv["files"].as_object().unwrap();
    for path in af.keys().chain(bf.keys()).collect::<BTreeSet<_>>() {
        let before = af.get(path);
        let after = bf.get(path);
        if before == after {
            continue;
        }
        diff.files.push(FileChange {
            path: path.clone(),
            kind: if before.is_none() {
                ChangeKind::Added
            } else if after.is_none() {
                ChangeKind::Deleted
            } else {
                ChangeKind::Modified
            },
            before_bytes: before.and_then(|v| v["bytes"].as_u64()).unwrap_or(0),
            after_bytes: after.and_then(|v| v["bytes"].as_u64()).unwrap_or(0),
        });
    }
    fn walk(a: Option<&Value>, b: Option<&Value>, path: &str, out: &mut Difference, depth: usize) {
        if a == b {
            return;
        }
        if out.values.len() >= 1024 || depth > 64 {
            out.omitted = true;
            return;
        }
        if let (Some(Value::Object(x)), Some(Value::Object(y))) = (a, b) {
            for key in x.keys().chain(y.keys()).collect::<BTreeSet<_>>() {
                walk(
                    x.get(key),
                    y.get(key),
                    &format!("{}/{}", path, key.replace('~', "~0").replace('/', "~1")),
                    out,
                    depth + 1,
                );
            }
            return;
        }
        if let (Some(Value::Array(x)), Some(Value::Array(y))) = (a, b)
            && x.len() == y.len()
            && x.len() <= 20000
        {
            for i in 0..x.len() {
                walk(x.get(i), y.get(i), &format!("{path}/{i}"), out, depth + 1);
            }
            return;
        }
        let summarize = |v: Option<&Value>| {
            v.map(|v| {
                let bytes = serde_json::to_vec(v).unwrap_or_default();
                if bytes.len() > 1024 {
                    json!(format!("{} bytes", bytes.len()))
                } else {
                    v.clone()
                }
            })
        };
        out.values.push(ValueChange {
            path: path.into(),
            kind: if a.is_none() {
                ChangeKind::Added
            } else if b.is_none() {
                ChangeKind::Deleted
            } else {
                ChangeKind::Modified
            },
            before: summarize(a),
            after: summarize(b),
        });
    }
    for key in [
        "chart",
        "options",
        "project",
        "folders",
        "chartName",
        "assets",
    ] {
        walk(av.get(key), bv.get(key), &format!("/{key}"), &mut diff, 0);
    }
    Ok(diff)
}
