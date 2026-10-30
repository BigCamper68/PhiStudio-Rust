use super::*;

pub fn subdivide_events(
    doc: &ChartDocument,
    selection: &Selection,
    parts: usize,
    at: Option<Beat>,
    mut slice: impl FnMut(EventType, &LineEvent, f64, f64) -> Result<LineEvent, ChartError>,
) -> Result<ToolEdit, ChartError> {
    if !(2..=256).contains(&parts) {
        return Err(invalid("Subdivision count must be 2–256"));
    }
    let mut items = objects(doc, selection)?;
    bounds(&items)?;
    let mut result = Vec::new();
    let mut changed = false;
    for item in items.drain(..) {
        let Some((_, kind)) = item.event else {
            result.push(item);
            continue;
        };
        let original = LineEvent::read(kind, &item.value)?;
        if nonlinear_speed(&item, &original) {
            return Err(invalid(
                "Subdivision requires Linear speed intervals; imported speed curve metadata is preserved",
            ));
        }
        let mut cuts = vec![original.start_time];
        if let Some(at) = at {
            if at > original.start_time && at < original.end_time {
                cuts.push(at);
            }
        } else {
            let duration = original.end_time.checked_sub(original.start_time)?;
            for i in 1..parts {
                cuts.push(
                    original
                        .start_time
                        .checked_add(duration.checked_mul(Beat::new(
                            0,
                            i as i32,
                            parts as i32,
                        )?)?)?,
                );
            }
        }
        cuts.push(original.end_time);
        if cuts.len() == 2 {
            result.push(item);
            continue;
        }
        changed = true;
        let time = |beat: Beat| doc.tempo().seconds_at(beat.as_f64());
        let start = time(original.start_time);
        let duration = time(original.end_time) - start;
        for pair in cuts.windows(2) {
            let mut event = slice(
                kind,
                &original,
                (time(pair[0]) - start) / duration,
                (time(pair[1]) - start) / duration,
            )?;
            event.start_time = pair[0];
            event.end_time = pair[1];
            result.push(Object {
                value: event.merge(kind, Some(&item.value))?,
                ..item.clone()
            });
        }
    }
    if !changed {
        return Err(invalid(
            "Select events containing the split beat, or subdivide the selection",
        ));
    }
    replacement(doc, selection, result, "Subdivide events")
}

pub fn connect_events(doc: &ChartDocument, selection: &Selection) -> Result<ToolEdit, ChartError> {
    let mut items = objects(doc, selection)?;
    bounds(&items)?;
    let mut order: Vec<usize> = (0..items.len())
        .filter(|i| items[*i].event.is_some())
        .collect();
    order.sort_by_key(|i| (items[*i].line, items[*i].event, items[*i].start().unwrap()));
    let mut previous: BTreeMap<(usize, usize, EventType), f64> = BTreeMap::new();
    for i in order {
        let item = &mut items[i];
        let (layer, kind) = item.event.unwrap();
        let mut event = LineEvent::read(kind, &item.value)?;
        if let Some(value) = previous.get(&(item.line, layer, kind)) {
            event.start = *value;
        }
        previous.insert((item.line, layer, kind), event.end);
        item.value = event.merge(kind, Some(&item.value))?;
    }
    replacement(doc, selection, items, "Connect event values")
}

pub fn join_events(doc: &ChartDocument, selection: &Selection) -> Result<ToolEdit, ChartError> {
    let mut items = objects(doc, selection)?;
    if items.len() < 2 || !selection.notes.is_empty() {
        return Err(invalid("Select at least two events in one track"));
    }
    items.sort_by_key(|item| item.start().unwrap());
    let base = &items[0];
    let key = (base.line, base.event);
    let kind = base.event.unwrap().1;
    let mut merged = LineEvent::read(kind, &base.value)?;
    if nonlinear_speed(base, &merged) {
        return Err(invalid(
            "Join requires Linear speed intervals; imported speed curve metadata is preserved",
        ));
    }
    let linear = |e: &LineEvent| {
        kind == EventType::Speed || e.start == e.end || (e.easing_type == 1 && e.bezier == 0)
    };
    if !linear(&merged) {
        return Err(invalid(
            "Join supports continuous constant or linear events; subdividing retains other curves",
        ));
    }
    let seconds = |b: Beat| doc.tempo().seconds_at(b.as_f64());
    let slope =
        (merged.end - merged.start) / (seconds(merged.end_time) - seconds(merged.start_time));
    for item in &items[1..] {
        let event = LineEvent::read(kind, &item.value)?;
        let next_slope =
            (event.end - event.start) / (seconds(event.end_time) - seconds(event.start_time));
        // Never silently discard different extension data or renderer metadata.
        let mut a = base.value.clone();
        let mut b = item.value.clone();
        for value in [&mut a, &mut b] {
            for field in ["startTime", "endTime", "start", "end"] {
                value.as_object_mut().unwrap().shift_remove(field);
            }
        }
        if (item.line, item.event) != key
            || merged.end_time != event.start_time
            || (merged.end - event.start).abs() > 1e-7
            || !linear(&event)
            || (slope - next_slope).abs() > 1e-7
            || a != b
        {
            return Err(invalid(
                "Join requires adjacent events with matching slopes, values and metadata",
            ));
        }
        merged.end_time = event.end_time;
        merged.end = event.end;
    }
    let value = merged.merge(kind, Some(&base.value))?;
    plan(
        doc,
        selection,
        vec![Object {
            value,
            ..base.clone()
        }],
        "Join events",
    )
}

pub fn create_event(
    doc: &ChartDocument,
    line: usize,
    layer: usize,
    kind: EventType,
    event: LineEvent,
    linked: bool,
) -> Result<ToolEdit, ChartError> {
    let mut items = vec![Object {
        line,
        event: Some((layer, kind)),
        value: event.merge(kind, None)?,
    }];
    if linked && matches!(kind, EventType::MoveX | EventType::MoveY) {
        let other = if kind == EventType::MoveX {
            EventType::MoveY
        } else {
            EventType::MoveX
        };
        let pair = doc.event_candidate(line, layer, other, event.start_time, event.end_time)?;
        items.push(Object {
            line,
            event: Some((layer, other)),
            value: pair.merge(other, None)?,
        });
    }
    plan(doc, &Selection::default(), items, "Add event")
}

pub fn update_event(
    doc: &ChartDocument,
    id: EventId,
    event: LineEvent,
    linked: bool,
) -> Result<ToolEdit, ChartError> {
    update_event_auto(doc, id, event, linked, false)
}

pub fn update_event_auto(
    doc: &ChartDocument,
    id: EventId,
    event: LineEvent,
    linked: bool,
    auto_connect: bool,
) -> Result<ToolEdit, ChartError> {
    let mut selection = Selection {
        notes: vec![],
        events: vec![id],
    };
    let old = doc.event(id)?;
    let timing = old.start_time != event.start_time || old.end_time != event.end_time;
    if linked && timing && matches!(id.kind, EventType::MoveX | EventType::MoveY) {
        selection = crate::linked_selection(doc, &selection)?;
    }
    let mut items = objects(doc, &selection)?;
    for item in &mut items {
        let kind = item.event.unwrap().1;
        if kind == id.kind {
            item.value = event.merge(kind, Some(&item.value))?;
        } else {
            item.with_times(event.start_time, event.end_time)?;
        }
    }
    if auto_connect {
        let next = doc
            .events(id.line, id.layer, id.kind)?
            .into_iter()
            .filter(|(i, e)| *i != id.index && e.start_time >= old.end_time && !e.constant)
            .min_by_key(|(_, e)| e.start_time);
        if let Some((index, mut next)) = next {
            let next_id = EventId { index, ..id };
            if !selection.events.contains(&next_id) {
                selection.events.push(next_id);
                let raw = &doc.event_values(id.line, id.layer, id.kind)?[index];
                next.start = event.end;
                items.push(Object {
                    line: id.line,
                    event: Some((id.layer, id.kind)),
                    value: next.merge(id.kind, Some(raw))?,
                });
            }
        }
    }
    replacement(doc, &selection, items, "Edit event")
}

pub fn synchronize_xy(
    doc: &ChartDocument,
    line: usize,
    layer: usize,
    mut slice: impl FnMut(EventType, &LineEvent, f64, f64) -> Result<LineEvent, ChartError>,
) -> Result<ToolEdit, ChartError> {
    if doc.validate_xy_layer(line, layer).is_ok() {
        return plan(doc, &Selection::default(), vec![], "Synchronize X/Y");
    }
    let (cuts, tracks) = crate::xy::xy_intervals(doc, line, layer)?;
    let cuts: Vec<_> = cuts.into_iter().collect();
    if cuts.len() > 10_000 {
        return Err(invalid("Synchronize at most 10,000 X/Y boundaries at once"));
    }
    if cuts.len() < 2 {
        return plan(doc, &Selection::default(), vec![], "Synchronize X/Y");
    }
    let mut selected = Selection::default();
    let mut items = Vec::new();
    for (kind, events) in tracks {
        selected.events.extend(events.iter().map(|(id, _)| *id));
        let mut cursor = 0;
        let mut previous = events.first().map_or(0., |(_, e)| e.start);
        let source_values = doc.event_values(line, layer, kind)?;
        for pair in cuts.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            while cursor < events.len() && events[cursor].1.end_time <= a {
                previous = events[cursor].1.end;
                cursor += 1;
            }
            let value = if let Some((id, source)) = events
                .get(cursor)
                .filter(|(_, e)| e.start_time <= a && e.end_time >= b)
            {
                let time = |v: Beat| doc.tempo().seconds_at(v.as_f64());
                let start = time(source.start_time);
                let duration = time(source.end_time) - start;
                let mut event = if source.start_time == a && source.end_time == b {
                    source.clone()
                } else {
                    slice(
                        kind,
                        source,
                        (time(a) - start) / duration,
                        (time(b) - start) / duration,
                    )?
                };
                event.start_time = a;
                event.end_time = b;
                event.merge(kind, Some(&source_values[id.index]))?
            } else {
                let value = previous;
                LineEvent {
                    start_time: a,
                    end_time: b,
                    start: value,
                    end: value,
                    ..Default::default()
                }
                .merge(kind, None)?
            };
            items.push(Object {
                line,
                event: Some((layer, kind)),
                value,
            });
        }
    }
    replacement(doc, &selected, items, "Synchronize X/Y")
}
