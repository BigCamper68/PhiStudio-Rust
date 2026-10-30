use super::*;

pub fn difference(
    root: &Path,
    workspace: &Path,
    id: &str,
    doc: &ChartDocument,
    location: &ProjectLocation,
) -> Result<Difference, ChartError> {
    let v = load(root, workspace, id)?;
    let base = package::active_root(&library::checked_workspace(root, workspace)?, location)?;
    let mut current = BTreeMap::new();
    let chart = location
        .chart
        .canonicalize()?
        .strip_prefix(&base)
        .map_err(|_| invalid("Chart is outside its workspace"))?
        .to_owned();
    let override_bytes = serde_json::to_vec(doc.as_json())?;
    for relative in package::inventory(&base)? {
        let fp = if relative == chart {
            transfer(&mut override_bytes.as_slice(), &mut std::io::sink())?
        } else {
            transfer(
                &mut fs::File::open(base.join(&relative))?.take(MAX_FILE + 1),
                &mut std::io::sink(),
            )?
        };
        current.insert(relative.to_string_lossy().replace('\\', "/"), fp);
    }
    let changed_files = v
        .files
        .keys()
        .chain(current.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|k| v.files.get(*k) != current.get(*k))
        .cloned()
        .collect();
    let (n, e, s) = stats(doc);
    let chart_changed = v.files.get(&v.chart.to_string_lossy().replace('\\', "/"))
        != current.get(&chart.to_string_lossy().replace('\\', "/"));
    let bytes = objects::read_file(
        root,
        workspace,
        &v,
        &v.chart.to_string_lossy().replace('\\', "/"),
    )?;
    let old = ChartDocument::parse(
        std::str::from_utf8(&bytes).map_err(|_| invalid("Invalid version chart text"))?,
    )?;
    let mut chart_changes = vec![];
    let before = old.as_json().as_object().unwrap();
    let after = doc.as_json().as_object().unwrap();
    for key in before.keys().chain(after.keys()).collect::<BTreeSet<_>>() {
        if key != "judgeLineList" && before.get(key) != after.get(key) {
            chart_changes.push(format!("Chart: {key} changed"));
        }
    }
    let before = old.as_json()["judgeLineList"].as_array().unwrap();
    let after = doc.as_json()["judgeLineList"].as_array().unwrap();
    for index in 0..before.len().max(after.len()) {
        if chart_changes.len() >= 512 {
            chart_changes.push("Further line changes omitted".into());
            break;
        }
        match (before.get(index), after.get(index)) {
            (None, Some(_)) => chart_changes.push(format!("Line {index}: added")),
            (Some(_), None) => chart_changes.push(format!("Line {index}: removed")),
            (Some(a), Some(b)) if a != b => {
                let fields = a
                    .as_object()
                    .unwrap()
                    .keys()
                    .chain(b.as_object().unwrap().keys())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .filter(|key| a.get(*key) != b.get(*key))
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(", ");
                chart_changes.push(format!("Line {index}: {fields}"));
            }
            _ => {}
        }
    }
    Ok(Difference {
        notes: [v.notes, n],
        events: [v.events, e],
        story: [v.story, s],
        chart_changed,
        changed_files,
        chart_changes,
    })
}
