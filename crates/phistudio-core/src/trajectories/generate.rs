use super::*;

pub fn recipes(doc: &ChartDocument, line: usize) -> Result<Vec<Recipe>, ChartError> {
    let value = doc.as_json()["judgeLineList"]
        .get(line)
        .ok_or_else(|| invalid("Judge line no longer exists"))?;
    match value.get(RECIPE_FIELD) {
        None => Ok(vec![]),
        Some(v) => serde_json::from_value(v.clone()).map_err(Into::into),
    }
}

pub(super) fn raw_recipes(doc: &ChartDocument, line: usize) -> Result<Vec<Value>, ChartError> {
    let value = doc.as_json()["judgeLineList"]
        .get(line)
        .ok_or_else(|| invalid("Judge line no longer exists"))?;
    value.get(RECIPE_FIELD).map_or_else(
        || Ok(vec![]),
        |v| {
            v.as_array()
                .cloned()
                .ok_or_else(|| invalid("Invalid trajectory parameters"))
        },
    )
}

pub(super) fn owned(
    doc: &ChartDocument,
    line: usize,
    recipe: &Recipe,
) -> Result<Vec<crate::EventId>, ChartError> {
    let mut result = Vec::new();
    let mut indexed = std::collections::BTreeMap::new();
    for item in &recipe.generated {
        if let std::collections::btree_map::Entry::Vacant(entry) = indexed.entry(item.kind) {
            let mut values = std::collections::HashMap::<String, Vec<usize>>::new();
            for (i, value) in doc
                .event_values(line, recipe.layer, item.kind)?
                .iter()
                .enumerate()
            {
                values
                    .entry(serde_json::to_string(value)?)
                    .or_default()
                    .push(i);
            }
            entry.insert(values);
        }
    }
    for item in &recipe.generated {
        let hits = indexed[&item.kind]
            .get(&serde_json::to_string(&item.value)?)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if hits.len() != 1 {
            return Err(invalid(
                "Generated events changed; detach the recipe before replacing them",
            ));
        }
        result.push(crate::EventId {
            line,
            layer: recipe.layer,
            kind: item.kind,
            index: hits[0],
        });
    }
    Ok(result)
}

pub fn detach(
    doc: &ChartDocument,
    line: usize,
    id: &str,
    delete_events: bool,
) -> Result<Edit, ChartError> {
    let mut list = raw_recipes(doc, line)?;
    let at = list
        .iter()
        .position(|r| r["id"].as_str() == Some(id))
        .ok_or_else(|| invalid("Trajectory no longer exists"))?;
    let old = list.remove(at);
    let tool = if delete_events {
        Some(tools::generated_event_edit(
            doc,
            &owned(doc, line, &serde_json::from_value::<Recipe>(old)?)?,
            &[],
            "Delete trajectory",
        )?)
    } else {
        None
    };
    Ok(Edit {
        line,
        before: doc.as_json()["judgeLineList"][line]
            .get(RECIPE_FIELD)
            .cloned(),
        after: if list.is_empty() {
            None
        } else {
            Some(serde_json::to_value(list)?)
        },
        tool,
        points: vec![],
    })
}

pub fn prepare(
    doc: &ChartDocument,
    line: usize,
    layer: usize,
    id: Option<&str>,
    name: &str,
    spec: Spec,
    ease: impl Fn(i32, f64) -> f64,
) -> Result<(Edit, usize), ChartError> {
    if layer > 3 || name.trim().is_empty() || name.len() > 512 {
        return Err(invalid("Invalid trajectory name or layer"));
    }
    let compiled = spec.compile()?;
    let mut times = vec![spec.start];
    let mut at = spec.start;
    while at < spec.end {
        at = at.checked_add(spec.step)?.min(spec.end);
        times.push(at);
        if times.len() > 6666 {
            return Err(invalid(
                "The motion exceeds 20,000 events; increase its step",
            ));
        }
    }
    if let Some(entries) = doc.as_json()["BPMList"].as_array() {
        for entry in entries {
            let beat: Beat = serde_json::from_value(entry["startTime"].clone())?;
            if beat > spec.start && beat < spec.end {
                times.push(beat);
            }
        }
    }
    times.sort();
    times.dedup();
    if times.len() > 6666 {
        return Err(invalid(
            "The motion exceeds 20,000 events; increase its step",
        ));
    }
    let duration = spec.end.checked_sub(spec.start)?.as_f64();
    let mut values = Vec::with_capacity(times.len());
    let mut last = None;
    for beat in &times {
        let mut value =
            compiled.sample(beat.checked_sub(spec.start)?.as_f64() / duration, &ease)?;
        if let Some(previous) = last {
            while value[2] - previous > 180. {
                value[2] -= 360.;
            }
            while value[2] - previous < -180. {
                value[2] += 360.;
            }
        }
        last = Some(value[2]);
        values.push(value);
    }
    let seconds: Vec<_> = times
        .iter()
        .map(|b| doc.tempo().seconds_at(b.as_f64()))
        .collect();
    let mut generated = Vec::new();
    let axes = if spec.tangent { 3 } else { 2 };
    let mut points: Vec<_> = values.iter().map(|v| [v[0], v[1]]).collect();
    for axis in 0..axes {
        let kind = [EventType::MoveX, EventType::MoveY, EventType::Rotate][axis];
        let mut pending = vec![(0, times.len() - 1)];
        let mut segments = Vec::new();
        while let Some((a, b)) = pending.pop() {
            let fits = spec.simplify
                && (a + 1..b).all(|i| {
                    let t = (seconds[i] - seconds[a]) / (seconds[b] - seconds[a]);
                    (values[a][axis] + t * (values[b][axis] - values[a][axis]) - values[i][axis])
                        .abs()
                        <= spec.tolerance[axis]
                });
            if a + 1 == b || fits {
                segments.push((a, b));
            } else {
                let middle = (a + b) / 2;
                pending.push((middle, b));
                pending.push((a, middle));
            }
        }
        for (a, b) in segments {
            if axis < 2 {
                for i in a..=b {
                    let t = (seconds[i] - seconds[a]) / (seconds[b] - seconds[a]);
                    points[i][axis] = values[a][axis] + t * (values[b][axis] - values[a][axis]);
                }
            }
            let event = LineEvent {
                start_time: times[a],
                end_time: times[b],
                start: values[a][axis],
                end: values[b][axis],
                ..Default::default()
            };
            generated.push(Generated {
                kind,
                value: event.merge(kind, None)?,
            });
        }
    }
    let mut list = raw_recipes(doc, line)?;
    let mut remove = Vec::new();
    let recipe_id = if let Some(id) = id {
        let old = list
            .iter()
            .find(|r| r["id"].as_str() == Some(id))
            .ok_or_else(|| invalid("Trajectory no longer exists"))?;
        remove = owned(doc, line, &serde_json::from_value(old.clone())?)?;
        id.to_owned()
    } else {
        if list.len() >= 128 {
            return Err(invalid("A line can store up to 128 trajectories"));
        }
        let next = list
            .iter()
            .filter_map(|r| {
                r["id"]
                    .as_str()?
                    .strip_prefix("trajectory-")?
                    .parse::<u64>()
                    .ok()
            })
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| invalid("Trajectory identifier limit reached"))?;
        format!("trajectory-{next}")
    };
    let additions: Vec<_> = generated
        .iter()
        .map(|g| (line, layer, g.kind, g.value.clone()))
        .collect();
    let tool = tools::generated_event_edit(doc, &remove, &additions, "Generate motion")?;
    let count = generated.len();
    let recipe = Recipe {
        id: recipe_id.clone(),
        name: name.trim().into(),
        layer,
        spec,
        generated,
    };
    let recipe = serde_json::to_value(recipe)?;
    if let Some(at) = list
        .iter()
        .position(|r| r["id"].as_str() == Some(&recipe_id))
    {
        let object = list[at].as_object_mut().unwrap();
        for (key, value) in recipe.as_object().unwrap() {
            if key == "spec"
                && let Some(old) = object.get_mut(key).and_then(Value::as_object_mut)
            {
                old.extend(value.as_object().unwrap().clone());
            } else {
                object.insert(key.clone(), value.clone());
            }
        }
    } else {
        list.push(recipe);
    }
    let before = doc.as_json()["judgeLineList"][line]
        .get(RECIPE_FIELD)
        .cloned();
    let after = Some(serde_json::to_value(list)?);
    let tool = if before == after { None } else { Some(tool) };
    Ok((
        Edit {
            line,
            before,
            after,
            tool,
            points,
        },
        count,
    ))
}
