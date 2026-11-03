//! Validated JSON edits with sparse, callback-free history for extensions.
use crate::{
    Beat, ChartDocument, ChartError, EventType, LineEvent, StoryEvent, StoryKind, TempoMap,
};
use phistudio_sdk::{MAX_PATCH_OPERATIONS, PatchOperation};
use serde_json::Value;

fn invalid(message: &str) -> ChartError {
    ChartError::Invalid(message.into())
}

#[derive(Debug)]
enum Change {
    Root {
        before: Value,
        after: Value,
    },
    Field {
        key: String,
        index: usize,
        before: Option<Value>,
        after: Option<Value>,
    },
    Element {
        index: usize,
        before: Option<Value>,
        after: Option<Value>,
    },
}

#[derive(Debug)]
struct Step {
    parent: Vec<String>,
    change: Change,
}

impl Step {
    /// Only prepared steps are replayed, in forward or reversed order.
    fn apply(&self, root: &mut Value, forward: bool) {
        if let Change::Root { before, after } = &self.change {
            *root = if forward { after } else { before }.clone();
            return;
        }
        let mut target = root;
        for part in &self.parent {
            target = match target {
                Value::Object(object) => object.get_mut(part).unwrap(),
                Value::Array(array) => &mut array[part.parse::<usize>().unwrap()],
                _ => unreachable!("Prepared patch parent"),
            };
        }
        match &self.change {
            Change::Field {
                key,
                index,
                before,
                after,
            } => {
                let object = target.as_object_mut().unwrap();
                if let Some(value) = if forward { after } else { before } {
                    object.shift_insert(*index, key.clone(), value.clone());
                } else {
                    object.shift_remove(key);
                }
            }
            Change::Element {
                index,
                before,
                after,
            } => {
                let (before, after) = if forward {
                    (before, after)
                } else {
                    (after, before)
                };
                let array = target.as_array_mut().unwrap();
                match (before, after) {
                    (None, Some(value)) => array.insert(*index, value.clone()),
                    (Some(_), None) => {
                        array.remove(*index);
                    }
                    (Some(_), Some(value)) => array[*index] = value.clone(),
                    _ => unreachable!("Prepared array change"),
                }
            }
            Change::Root { .. } => unreachable!(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct PatchDelta {
    steps: Vec<Step>,
    tempo: Option<(TempoMap, TempoMap)>,
}
impl PatchDelta {
    pub fn apply(&self, doc: &mut ChartDocument, forward: bool) {
        if forward {
            for step in &self.steps {
                step.apply(&mut doc.root, true);
            }
        } else {
            for step in self.steps.iter().rev() {
                step.apply(&mut doc.root, false);
            }
        }
        if let Some((before, after)) = &self.tempo {
            doc.set_tempo(if forward { after } else { before }.clone());
        }
    }
}

fn pointer(path: &str) -> Result<Vec<String>, ChartError> {
    if path.is_empty() {
        return Ok(Vec::new());
    }
    if !path.starts_with('/') || path.len() > 4096 {
        return Err(invalid("Invalid JSON pointer"));
    }
    let parts: Vec<_> = path[1..].split('/').collect();
    if parts.len() > 64 {
        return Err(invalid("JSON pointer exceeds 64 levels"));
    }
    parts
        .into_iter()
        .map(|s| {
            let mut result = String::new();
            let mut chars = s.chars();
            while let Some(c) = chars.next() {
                if c == '~' {
                    result.push(match chars.next() {
                        Some('0') => '~',
                        Some('1') => '/',
                        _ => return Err(invalid("Invalid JSON pointer escape")),
                    });
                } else {
                    result.push(c);
                }
            }
            Ok(result)
        })
        .collect()
}

fn index(part: &str) -> Result<usize, ChartError> {
    if part.is_empty()
        || (part.len() > 1 && part.starts_with('0'))
        || !part.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(invalid("Invalid JSON array index"));
    }
    part.parse()
        .map_err(|_| invalid("JSON array index is too large"))
}

fn get<'a>(mut root: &'a Value, path: &[String]) -> Result<&'a Value, ChartError> {
    for part in path {
        root = match root {
            Value::Object(object) => object.get(part),
            Value::Array(array) => array.get(index(part)?),
            _ => None,
        }
        .ok_or_else(|| invalid("JSON pointer target does not exist"))?;
    }
    Ok(root)
}

fn prepare_step(root: &Value, operation: PatchOperation) -> Result<Option<Step>, ChartError> {
    let (path, after, adding, testing) = match operation {
        PatchOperation::PluginData { .. } => {
            return Err(invalid(
                "Plugin data operations must be expanded before preparation",
            ));
        }
        PatchOperation::Add { path, value } => (path, Some(value), true, false),
        PatchOperation::Replace { path, value } => (path, Some(value), false, false),
        PatchOperation::Remove { path } => (path, None, false, false),
        PatchOperation::Test { path, value } => (path, Some(value), false, true),
    };
    let mut parts = pointer(&path)?;
    if testing {
        if Some(get(root, &parts)?) != after.as_ref() {
            return Err(invalid("Plugin transaction test failed"));
        }
        return Ok(None);
    }
    let Some(key) = parts.pop() else {
        let after = after.ok_or_else(|| invalid("Cannot remove the chart root"))?;
        return Ok((root != &after).then(|| Step {
            parent: vec![],
            change: Change::Root {
                before: root.clone(),
                after,
            },
        }));
    };
    let change = match get(root, &parts)? {
        Value::Object(object) => {
            let before = object.get(&key).cloned();
            if !adding && before.is_none() {
                return Err(invalid("JSON field does not exist"));
            }
            if before == after {
                return Ok(None);
            }
            let index = object
                .keys()
                .position(|k| k == &key)
                .unwrap_or(object.len());
            Change::Field {
                key,
                index,
                before,
                after,
            }
        }
        Value::Array(array) => {
            let i = if adding && key == "-" {
                array.len()
            } else {
                index(&key)?
            };
            if i > array.len() || (!adding && i == array.len()) {
                return Err(invalid("JSON array index is out of range"));
            }
            let before = if adding { None } else { Some(array[i].clone()) };
            if !adding && before == after {
                return Ok(None);
            }
            Change::Element {
                index: i,
                before,
                after,
            }
        }
        _ => return Err(invalid("JSON pointer parent must be an object or array")),
    };
    Ok(Some(Step {
        parent: parts,
        change,
    }))
}

fn validate_tracks(before: &ChartDocument, after: &ChartDocument) -> Result<(), ChartError> {
    fn disjoint(mut intervals: Vec<(Beat, Beat)>) -> Result<(), ChartError> {
        intervals.sort_unstable();
        if intervals.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(invalid("Plugin edits create overlapping events"));
        }
        Ok(())
    }
    for line in 0..after.line_count() {
        let old = &before.root["judgeLineList"][line];
        let new = &after.root["judgeLineList"][line];
        if [
            "Name",
            "Texture",
            "anchor",
            "Group",
            "bpmfactor",
            "father",
            "rotateWithFather",
            "isCover",
            "zOrder",
            "attachUI",
        ]
        .iter()
        .any(|key| old.get(*key) != new.get(*key))
        {
            after.line_properties(line)?.validate(after, line)?;
        }
        if old.get("eventLayers") != new.get("eventLayers") {
            if !new["eventLayers"].is_null() && !new["eventLayers"].is_array() {
                return Err(invalid("eventLayers must be an array"));
            }
            for layer in 0..after.event_layer_count(line) {
                let object = &new["eventLayers"][layer];
                if !object.is_object() && !object.is_null() {
                    return Err(invalid("Event layer must be an object or null"));
                }
                for kind in EventType::ALL {
                    let track = &new["eventLayers"][layer][kind.key()];
                    if old["eventLayers"][layer].get(kind.key())
                        == new["eventLayers"][layer].get(kind.key())
                    {
                        continue;
                    }
                    if track.is_null() {
                        continue;
                    }
                    let rows = after.event_values(line, layer, kind)?;
                    let mut extents = Vec::with_capacity(rows.len());
                    for row in rows {
                        let event = LineEvent::read(kind, row)?;
                        event.validate(kind)?;
                        extents.push((event.start_time, event.end_time));
                    }
                    disjoint(extents)?;
                }
            }
        }
        if old.get("extended") != new.get("extended") {
            if !new["extended"].is_null() && !new["extended"].is_object() {
                return Err(invalid("extended must be an object"));
            }
            for kind in StoryKind::ALL {
                if old["extended"].get(kind.key()) == new["extended"].get(kind.key()) {
                    continue;
                }
                let rows = &new["extended"][kind.key()];
                if rows.is_null() {
                    continue;
                }
                let mut extents = Vec::new();
                for row in rows
                    .as_array()
                    .ok_or_else(|| invalid("Storyboard track must be an array"))?
                {
                    let event = serde_json::from_value::<StoryEvent>(row.clone())?;
                    event.validate(kind)?;
                    extents.push((event.start_time, event.end_time));
                }
                disjoint(extents)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn prepare(
    doc: &ChartDocument,
    operations: Vec<PatchOperation>,
) -> Result<Option<PatchDelta>, ChartError> {
    if operations.len() > MAX_PATCH_OPERATIONS {
        return Err(invalid("Transaction exceeds 40,000 operations"));
    }
    if operations.is_empty() {
        return Ok(None);
    }
    let mut root = doc.root.clone();
    let mut steps = Vec::new();
    for operation in operations {
        let expanded = match operation {
            PatchOperation::PluginData { plugin, entry } => {
                crate::plugin_data::operations(&root, &plugin, entry)?
            }
            other => vec![other],
        };
        for operation in expanded {
            if let Some(step) = prepare_step(&root, operation)? {
                if steps.len() >= MAX_PATCH_OPERATIONS {
                    return Err(invalid("Expanded transaction exceeds 40,000 operations"));
                }
                step.apply(&mut root, true);
                steps.push(step);
            }
        }
    }
    // Counts are derived for changed note arrays, but authored counts on untouched
    // imported lines and every missing/null container remain unchanged.
    if let Some(lines) = root["judgeLineList"].as_array() {
        let counts: Vec<_> = lines
            .iter()
            .enumerate()
            .filter(|(i, line)| doc.root["judgeLineList"][*i].get("notes") != line.get("notes"))
            .map(|(i, line)| {
                (
                    i,
                    line.get("notes")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len),
                )
            })
            .collect();
        for (line, count) in counts {
            if let Some(step) = prepare_step(
                &root,
                PatchOperation::Add {
                    path: format!("/judgeLineList/{line}/numOfNotes"),
                    value: Value::from(count),
                },
            )? {
                step.apply(&mut root, true);
                steps.push(step);
            }
        }
    }
    if root == doc.root {
        return Ok(None);
    }
    if serde_json::to_vec(&root)?.len() > ChartDocument::MAX_JSON_BYTES {
        return Err(invalid("Chart JSON exceeds 256 MiB"));
    }
    let after = ChartDocument::from_edit_value(root)?;
    validate_tracks(doc, &after)?;
    for key in ["name", "level", "composer", "charter", "offset"] {
        if doc.root["META"].get(key) == after.root["META"].get(key) {
            continue;
        }
        if let Some(value) = after.root["META"].get(key) {
            let valid = if key == "offset" {
                value.as_f64().is_some_and(f64::is_finite)
            } else {
                value
                    .as_str()
                    .is_some_and(|s| s.len() <= 4096 && (key != "name" || !s.trim().is_empty()))
            };
            if !valid {
                return Err(invalid("Invalid chart metadata in plugin transaction"));
            }
        }
    }
    if doc.root.get(crate::noise_fields::LIST) != after.root.get(crate::noise_fields::LIST) {
        for field in crate::noise_fields::fields(&after)? {
            crate::noise_fields::validate(field)?;
        }
    }
    if doc.root.get(crate::shaders::EXTRA_KEY) != after.root.get(crate::shaders::EXTRA_KEY)
        && let Some(extra) = after.root.get(crate::shaders::EXTRA_KEY)
    {
        crate::shaders::validate_shader_extra(extra)?;
    }
    if doc.root.get(crate::shaders::SOURCES_KEY) != after.root.get(crate::shaders::SOURCES_KEY) {
        crate::shaders::shader_sources(&after)?;
    }
    let tempo = (doc.root.get("BPMList") != after.root.get("BPMList"))
        .then(|| (doc.tempo().clone(), after.tempo().clone()));
    Ok(Some(PatchDelta { steps, tempo }))
}
