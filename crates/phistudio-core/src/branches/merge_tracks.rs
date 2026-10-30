//! Conservative three-way sequence alignment for authored tracks. No IDs are
//! added to user JSON. Crossed anchors, duplicate objects and ambiguous gaps
//! keep the existing explicit whole-track conflict.
use super::*;

pub(super) fn is_track(path: &str) -> bool {
    let parts: Vec<_> = path.split('/').collect();
    parts.len() == 5 && parts[1..3] == ["chart", "judgeLineList"] && parts[4] == "notes"
        || parts.len() == 7
            && parts[1..3] == ["chart", "judgeLineList"]
            && parts[4] == "eventLayers"
            && parts[6].ends_with("Events")
        || parts.len() == 6
            && parts[1..3] == ["chart", "judgeLineList"]
            && parts[4] == "extended"
            && parts[5].ends_with("Events")
}

fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), canonical(v)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        _ => value.clone(),
    }
}
fn key(value: &Value) -> String {
    canonical(value).to_string()
}
fn timing(value: &Value) -> Option<String> {
    let start = value.get("startTime")?.as_array()?;
    let end = value.get("endTime")?.as_array()?;
    Some(json!([value.get("type"), start, end]).to_string())
}
fn unique(
    values: &[Value],
    signature: fn(&Value) -> Option<String>,
) -> BTreeMap<String, Option<usize>> {
    let mut map = BTreeMap::new();
    for (i, value) in values.iter().enumerate() {
        if let Some(key) = signature(value) {
            map.entry(key).and_modify(|v| *v = None).or_insert(Some(i));
        }
    }
    map
}
pub(crate) struct Alignment {
    pub objects: Vec<Option<usize>>,
    pub inserted: Vec<Vec<usize>>,
}
pub(crate) fn align(base: &[Value], side: &[Value]) -> Option<Alignment> {
    if !base.iter().chain(side).all(Value::is_object) {
        return None;
    }
    let exact = |v: &Value| Some(key(v));
    let old = unique(base, exact);
    let new = unique(side, exact);
    // Never give identical stacked objects arbitrary identities.
    if old.values().chain(new.values()).any(Option::is_none) {
        return None;
    }
    let mut anchors = BTreeMap::new();
    let mut used = BTreeSet::new();
    for (key, index) in &old {
        if let Some(Some(next)) = new.get(key) {
            anchors.insert(index.unwrap(), *next);
            used.insert(*next);
        }
    }
    let old_time = unique(base, timing);
    let new_time = unique(side, timing);
    for (key, index) in old_time {
        if let (Some(i), Some(Some(j))) = (index, new_time.get(&key))
            && !anchors.contains_key(&i)
            && !used.contains(j)
        {
            anchors.insert(i, *j);
            used.insert(*j);
        }
    }
    let mut result = Alignment {
        objects: vec![None; base.len()],
        inserted: vec![vec![]; base.len() + 1],
    };
    let (mut b, mut s) = (0, 0);
    for (i, j) in anchors
        .into_iter()
        .chain(std::iter::once((base.len(), side.len())))
    {
        if j < s {
            return None;
        } // Reordering would make positional matching unsafe.
        match (i - b, j - s) {
            (0, _) => result.inserted[i].extend(s..j),
            (_, 0) => {} // Deleted objects.
            (1, 1) => result.objects[b] = Some(s),
            _ => return None, // Multiple unmatched replacements: ask the user.
        }
        if i < base.len() {
            result.objects[i] = Some(j);
        }
        b = i + 1;
        s = j + 1;
    }
    Some(result)
}

pub(super) fn merge_track(
    base: &[Value],
    current: &[Value],
    incoming: &[Value],
    path: &str,
    choices: &BTreeMap<String, Side>,
    conflicts: &mut Vec<Conflict>,
    depth: usize,
) -> Result<Option<Vec<Value>>, ChartError> {
    let (Some(ours), Some(theirs)) = (align(base, current), align(base, incoming)) else {
        return Ok(None);
    };
    let mut output = vec![];
    for i in 0..=base.len() {
        let a: Vec<_> = ours.inserted[i]
            .iter()
            .map(|j| current[*j].clone())
            .collect();
        let b: Vec<_> = theirs.inserted[i]
            .iter()
            .map(|j| incoming[*j].clone())
            .collect();
        if a.is_empty() {
            output.extend(b);
        } else if b.is_empty() || a == b {
            output.extend(a);
        } else {
            // Concurrent different insertions into the same gap have no common
            // identity/order. Resolve this gap without discarding other changes.
            let inserted = merge_logic::merge(
                Some(&json!([])),
                Some(&json!(a)),
                Some(&json!(b)),
                &format!("{path}/insert-{i}"),
                choices,
                conflicts,
                depth + 1,
            )?
            .unwrap();
            output.extend(inserted.as_array().unwrap().iter().cloned());
        }
        if i < base.len()
            && let Some(value) = merge_logic::merge(
                Some(&base[i]),
                ours.objects[i].map(|j| &current[j]),
                theirs.objects[i].map(|j| &incoming[j]),
                &format!("{path}/{i}"),
                choices,
                conflicts,
                depth + 1,
            )?
        {
            output.push(value);
        }
    }
    Ok(Some(output))
}
