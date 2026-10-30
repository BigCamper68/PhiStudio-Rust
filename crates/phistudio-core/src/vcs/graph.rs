use super::*;
pub struct GraphRow {
    pub id: String,
    pub lane: usize,
    pub parents: Vec<(usize, usize)>,
    pub refs: Vec<String>,
}
pub fn graph(
    commits: &[Commit],
    state: &State,
    current_only: bool,
    include_unreferenced: bool,
) -> Vec<GraphRow> {
    let by_id: BTreeMap<_, _> = commits.iter().map(|v| (v.id.clone(), v)).collect();
    let mut visible = BTreeSet::new();
    let mut pending: Vec<String> = if current_only {
        state.head_id().into_iter().map(str::to_owned).collect()
    } else {
        state
            .branches
            .values()
            .cloned()
            .chain(state.tags.values().map(|v| v.commit.clone()))
            .chain(state.head_id().map(str::to_owned))
            .collect()
    };
    if include_unreferenced && !current_only {
        pending.extend(by_id.keys().cloned());
    }
    if !current_only {
        pending.extend(
            by_id
                .iter()
                .filter(|(_, v)| v.kind == "legacy")
                .map(|(id, _)| id.clone()),
        );
    }
    while let Some(id) = pending.pop() {
        if visible.insert(id.clone())
            && let Some(v) = by_id.get(&id)
        {
            pending.extend(v.parents.iter().cloned());
        }
    }
    visible.retain(|id| by_id.contains_key(id));
    let mut children: BTreeMap<String, usize> = visible.iter().map(|id| (id.clone(), 0)).collect();
    for id in &visible {
        for p in &by_id[id].parents {
            if let Some(n) = children.get_mut(p) {
                *n += 1;
            }
        }
    }
    let mut ready: BTreeSet<_> = children
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(id, _)| (by_id[id].created_ms, id.clone()))
        .collect();
    let mut order = vec![];
    while let Some((_, id)) = ready.pop_last() {
        order.push(id.clone());
        for p in &by_id[&id].parents {
            if let Some(n) = children.get_mut(p) {
                *n -= 1;
                if *n == 0 {
                    ready.insert((by_id[p].created_ms, p.clone()));
                }
            }
        }
    }
    let mut lanes: Vec<Option<String>> = vec![];
    let mut rows = vec![];
    for id in &order {
        let lane = lanes
            .iter()
            .position(|v| v.as_ref() == Some(id))
            .or_else(|| lanes.iter().position(Option::is_none))
            .unwrap_or(lanes.len());
        if lane == lanes.len() {
            lanes.push(None);
        }
        lanes[lane] = None;
        let mut parent_lanes = vec![];
        for (i, p) in by_id[id]
            .parents
            .iter()
            .filter(|p| visible.contains(*p))
            .enumerate()
        {
            let target = lanes
                .iter()
                .position(|v| v.as_ref() == Some(p))
                .unwrap_or_else(|| {
                    let target = if i == 0 && lanes[lane].is_none() {
                        lane
                    } else {
                        lanes
                            .iter()
                            .position(Option::is_none)
                            .unwrap_or(lanes.len())
                    };
                    if target == lanes.len() {
                        lanes.push(None);
                    }
                    lanes[target] = Some(p.clone());
                    target
                });
            parent_lanes.push((p.clone(), target));
        }
        rows.push((id.clone(), lane, parent_lanes));
    }
    let positions: BTreeMap<_, _> = order
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), i))
        .collect();
    rows.into_iter()
        .map(|(id, lane, parents)| {
            let mut refs = vec![];
            if state.head_id() == Some(&id) {
                refs.push("HEAD".into());
            }
            refs.extend(
                state
                    .branches
                    .iter()
                    .filter(|(_, v)| *v == &id)
                    .map(|(n, _)| n.clone()),
            );
            refs.extend(
                state
                    .tags
                    .iter()
                    .filter(|(_, v)| v.commit == id)
                    .map(|(n, _)| format!("#{n}")),
            );
            GraphRow {
                id,
                lane,
                parents: parents
                    .into_iter()
                    .filter_map(|(p, l)| positions.get(&p).map(|row| (*row, l)))
                    .collect(),
                refs,
            }
        })
        .collect()
}
