//! Temporary identities exist only inside a merge; user documents never receive IDs.
use super::*;
use crate::branches::merge_logic::merge;
use sha2::{Digest, Sha256};
fn key(value: &Value, rename: bool) -> String {
    let mut value = value.clone();
    if let Some(map) = value.as_object_mut() {
        map.remove("father");
        if rename {
            map.remove("Name");
        }
    }
    value.to_string()
}
fn unique(
    lines: &[Value],
    signature: impl Fn(&Value) -> Option<String>,
) -> BTreeMap<String, usize> {
    let mut map = BTreeMap::new();
    let mut duplicates = BTreeSet::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(k) = signature(line)
            && map.insert(k.clone(), i).is_some()
        {
            duplicates.insert(k);
        }
    }
    for k in duplicates {
        map.remove(&k);
    }
    map
}
fn identities(base: &[Value], side: &[Value], prefix: &str) -> Option<Vec<String>> {
    let mut ids = vec![String::new(); side.len()];
    let mut used = BTreeSet::new();
    for mode in 0..3 {
        let signature = |v: &Value| match mode {
            0 => v["Name"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            1 => Some(key(v, false)),
            _ => Some(key(v, true)),
        };
        let old = unique(base, signature);
        let new = unique(side, signature);
        for (k, j) in new {
            if let Some(i) = old.get(&k)
                && ids[j].is_empty()
                && used.insert(*i)
            {
                ids[j] = format!("base-{i}");
            }
        }
    }
    let names = unique(side, |v| v["Name"].as_str().map(str::to_owned));
    for (j, id) in ids.iter_mut().enumerate() {
        if id.is_empty() {
            // A renamed and edited anonymous/duplicate line has no safe identity.
            if base
                .iter()
                .enumerate()
                .any(|(i, b)| !used.contains(&i) && b["Name"] == side[j]["Name"])
            {
                return None;
            }
            *id = if side[j]["Name"]
                .as_str()
                .is_some_and(|n| names.get(n) == Some(&j))
            {
                format!("new-{:x}", Sha256::digest(side[j]["Name"].to_string()))
            } else {
                format!("{prefix}-{j}")
            };
        }
    }
    Some(ids)
}
fn normalize(chart: &Value, ids: &[String], structural: bool) -> Value {
    let mut chart = chart.clone();
    let lines = chart["judgeLineList"].as_array().unwrap();
    let mut map = serde_json::Map::new();
    for (line, id) in lines.iter().zip(ids) {
        let mut line = line.clone();
        if structural && let Some(parent) = line.get_mut("father") {
            let index = parent.as_i64().or_else(|| {
                parent
                    .as_f64()
                    .filter(|v| v.fract() == 0.)
                    .map(|n| n as i64)
            });
            *parent = index
                .and_then(|i| usize::try_from(i).ok())
                .and_then(|i| ids.get(i))
                .map_or(Value::Null, |id| json!(id));
        }
        map.insert(id.clone(), line);
    }
    chart["judgeLineList"] = Value::Object(map);
    if structural {
        for pointer in ["/effects", "/shaderEvents", "/META/effects"] {
            if let Some(values) = chart.pointer_mut(pointer).and_then(Value::as_array_mut) {
                for value in values {
                    let owner = crate::line_management::effect_owner(value, ids.len())
                        .and_then(|i| ids.get(i));
                    let mut data = value.clone();
                    if owner.is_some() {
                        data.as_object_mut().unwrap().remove("line");
                    }
                    *value = json!({"data":data,"owner":owner});
                }
            }
        }
    }
    chart
}
fn order(
    base: &[String],
    ours: &[String],
    theirs: &[String],
    choices: &BTreeMap<String, Side>,
    conflicts: &mut Vec<Conflict>,
) -> Vec<String> {
    let common: BTreeSet<_> = base
        .iter()
        .filter(|id| ours.contains(id) && theirs.contains(id))
        .collect();
    let filtered = |v: &[String]| {
        v.iter()
            .filter(|id| common.contains(id))
            .cloned()
            .collect::<Vec<_>>()
    };
    let (b, o, t) = (filtered(base), filtered(ours), filtered(theirs));
    let incoming = if o == b {
        true
    } else if t == b || o == t {
        false
    } else {
        let choice = choices.get("/chart/judgeLineList/$order").copied();
        conflicts.push(Conflict {
            path: "/chart/judgeLineList/$order".into(),
            base: Some(json!(base)),
            current: Some(json!(ours)),
            incoming: Some(json!(theirs)),
            choice,
        });
        choice == Some(Side::Incoming)
    };
    let (primary, secondary) = if incoming {
        (theirs, ours)
    } else {
        (ours, theirs)
    };
    let mut result = primary.to_vec();
    for (i, id) in secondary.iter().enumerate() {
        if !result.contains(id) {
            let at = secondary[i + 1..]
                .iter()
                .find_map(|next| result.iter().position(|v| v == next))
                .unwrap_or(result.len());
            result.insert(at, id.clone());
        }
    }
    for id in base {
        if !result.contains(id) {
            result.push(id.clone());
        }
    }
    result
}
fn track_maps(values: [&Value; 3]) -> Option<[Value; 3]> {
    use crate::branches::merge_tracks::align;
    let [b, o, t] = values.map(Value::as_array);
    let (b, o, t) = (b?, o?, t?);
    if b == o && b == t {
        return None;
    }
    let (oa, ta) = (align(b, o)?, align(b, t)?);
    let base: serde_json::Map<_, _> = b
        .iter()
        .enumerate()
        .map(|(i, v)| (format!("base-{i}"), v.clone()))
        .collect();
    let convert = |side: &[Value], alignment: crate::branches::merge_tracks::Alignment| {
        let mut map = serde_json::Map::new();
        for (i, j) in alignment.objects.iter().enumerate() {
            if let Some(j) = j {
                map.insert(format!("base-{i}"), side[*j].clone());
            }
        }
        let mut occurrences = BTreeMap::<String, usize>::new();
        for j in alignment.inserted.iter().flatten() {
            let v = &side[*j];
            let identity = json!([
                v.get("type"),
                v.get("startTime"),
                v.get("positionX"),
                v.get("above")
            ]);
            let hash = format!("{:x}", Sha256::digest(identity.to_string()));
            let n = occurrences.entry(hash.clone()).or_default();
            let id = format!("new-{hash}-{n}");
            *n += 1;
            map.insert(id, v.clone());
        }
        Value::Object(map)
    };
    Some([Value::Object(base), convert(o, oa), convert(t, ta)])
}
fn time(value: &Value) -> f64 {
    value["startTime"]
        .as_array()
        .filter(|v| v.len() == 3)
        .map(|v| {
            v[0].as_f64().unwrap_or(0.)
                + v[1].as_f64().unwrap_or(0.) / v[2].as_f64().filter(|v| *v != 0.).unwrap_or(1.)
        })
        .unwrap_or(0.)
}
fn track_paths(line: &Value) -> Vec<String> {
    let mut paths = vec!["/notes".into()];
    if let Some(layers) = line["eventLayers"].as_array() {
        for (i, layer) in layers.iter().enumerate() {
            if let Some(map) = layer.as_object() {
                for k in map.keys().filter(|k| k.ends_with("Events")) {
                    paths.push(format!("/eventLayers/{i}/{k}"));
                }
            }
        }
    }
    if let Some(map) = line["extended"].as_object() {
        for k in map.keys().filter(|k| k.ends_with("Events")) {
            paths.push(format!("/extended/{k}"));
        }
    }
    paths
}
pub(super) fn chart(
    base: &Value,
    ours: &Value,
    theirs: &Value,
    choices: &BTreeMap<String, Side>,
    conflicts: &mut Vec<Conflict>,
) -> Result<Value, ChartError> {
    if ours == theirs || base == theirs {
        return Ok(ours.clone());
    }
    if base == ours {
        return Ok(theirs.clone());
    }
    let (Some(b), Some(o), Some(t)) = (
        base["judgeLineList"].as_array(),
        ours["judgeLineList"].as_array(),
        theirs["judgeLineList"].as_array(),
    ) else {
        return Ok(merge(
            Some(base),
            Some(ours),
            Some(theirs),
            "/chart",
            choices,
            conflicts,
            0,
        )?
        .unwrap());
    };
    let Some((oi, ti)) = identities(b, o, "ours").zip(identities(b, t, "theirs")) else {
        let choice = choices.get("/chart").copied();
        conflicts.push(Conflict {
            path: "/chart".into(),
            base: Some(base.clone()),
            current: Some(ours.clone()),
            incoming: Some(theirs.clone()),
            choice,
        });
        return Ok(if choice == Some(Side::Incoming) {
            theirs.clone()
        } else {
            ours.clone()
        });
    };
    let bi: Vec<_> = (0..b.len()).map(|i| format!("base-{i}")).collect();
    let structural = bi != oi || bi != ti;
    let ids = order(&bi, &oi, &ti, choices, conflicts);
    let mut charts = [
        normalize(base, &bi, structural),
        normalize(ours, &oi, structural),
        normalize(theirs, &ti, structural),
    ];
    let bpm = charts.each_ref().map(|chart| chart["BPMList"].as_array());
    let bpm_maps = if let [Some(a), Some(b), Some(c)] = bpm {
        let convert = |values: &[Value]| -> Option<Value> {
            let mut map = serde_json::Map::new();
            for v in values {
                let key = v.get("startTime")?.to_string();
                if map
                    .insert(format!("bpm-{:x}", Sha256::digest(key)), v.clone())
                    .is_some()
                {
                    return None;
                }
            }
            Some(Value::Object(map))
        };
        convert(a)
            .zip(convert(b))
            .zip(convert(c))
            .map(|((a, b), c)| [a, b, c])
    } else {
        None
    };
    let mapped_bpm = bpm_maps.is_some();
    if let Some(maps) = bpm_maps {
        for (chart, value) in charts.iter_mut().zip(maps) {
            chart["BPMList"] = value;
        }
    }
    let mut tracks = vec![];
    for id in &bi {
        let root = format!("/judgeLineList/{id}");
        for path in track_paths(&charts[0]["judgeLineList"][id]) {
            let full = format!("{root}{path}");
            if let [Some(a), Some(b), Some(c)] = charts.each_ref().map(|v| v.pointer(&full))
                && let Some(mapped) = track_maps([a, b, c])
            {
                for (chart, value) in charts.iter_mut().zip(mapped) {
                    *chart.pointer_mut(&full).unwrap() = value;
                }
                if path == "/notes" {
                    for chart in &mut charts {
                        chart["judgeLineList"][id]["numOfNotes"] = json!(0);
                    }
                }
                tracks.push((full, id.clone(), path == "/notes"));
            }
        }
    }
    let mut result = merge(
        Some(&charts[0]),
        Some(&charts[1]),
        Some(&charts[2]),
        "/chart",
        choices,
        conflicts,
        0,
    )?
    .unwrap();
    if mapped_bpm {
        let mut values: Vec<_> = result["BPMList"]
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect();
        values.sort_by(|a, b| time(a).total_cmp(&time(b)));
        result["BPMList"] = json!(values);
    }
    for (full, id, notes) in tracks {
        if let Some(value) = result.pointer_mut(&full)
            && let Some(map) = value.as_object()
        {
            let mut values: Vec<_> = map.values().cloned().collect();
            values.sort_by(|a, b| time(a).total_cmp(&time(b)));
            *value = Value::Array(values);
            if notes {
                result["judgeLineList"][&id]["numOfNotes"] =
                    json!(result.pointer(&full).unwrap().as_array().unwrap().len());
            }
        }
    }
    let mut lines = result["judgeLineList"].as_object().unwrap().clone();
    let ids: Vec<_> = ids
        .into_iter()
        .filter(|id| lines.contains_key(id))
        .collect();
    let mut ordered = vec![];
    for id in &ids {
        let mut line = lines.remove(id).unwrap();
        if structural && let Some(parent) = line.get_mut("father") {
            *parent = json!(
                parent
                    .as_str()
                    .and_then(|p| ids.iter().position(|id| id == p))
                    .map_or(-1, |i| i as i64)
            );
        }
        ordered.push(line);
    }
    result["judgeLineList"] = json!(ordered);
    if structural {
        for pointer in ["/effects", "/shaderEvents", "/META/effects"] {
            if let Some(values) = result.pointer_mut(pointer).and_then(Value::as_array_mut) {
                let mut out = vec![];
                for value in values.iter() {
                    let mut data = value["data"].clone();
                    if let Some(owner) = value["owner"].as_str() {
                        let Some(index) = ids.iter().position(|id| id == owner) else {
                            continue;
                        };
                        data["line"] = json!(index);
                    }
                    out.push(data);
                }
                *values = out;
            }
        }
    }
    Ok(result)
}
