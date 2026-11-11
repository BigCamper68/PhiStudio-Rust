use super::*;

#[derive(Clone, Debug)]
pub enum Transform {
    Shift {
        beats: Beat,
        x: f64,
    },
    MirrorX(f64),
    ScaleX {
        anchor: f64,
        factor: f64,
    },
    ReverseTime,
    ScaleTime(Beat),
    Quantize(i32),
    FlipSide,
    AlignTime,
    ArrangeX {
        from: f64,
        to: f64,
    },
    NoteType {
        kind: NoteKind,
        hold_length: Beat,
    },
    Side(u8),
    Fake(bool),
    Easing(i32),
    Curve {
        easing: i32,
        window: [f64; 2],
        bezier: Option<[f64; 4]>,
    },
    ReverseValues,
}

pub(super) fn mirror(item: &mut Object, center: f64) -> Result<(), ChartError> {
    if !center.is_finite() {
        return Err(invalid("Mirror center must be finite"));
    }
    if let Some((_, EventType::MoveX)) = item.event {
        let mut event = LineEvent::read(EventType::MoveX, &item.value)?;
        event.start = 2. * center - event.start;
        event.end = 2. * center - event.end;
        item.value = event.merge(EventType::MoveX, Some(&item.value))?;
    } else if item.event.is_none() {
        let mut note: Note = serde_json::from_value(item.value.clone())?;
        note.x = 2. * center - note.x;
        item.value = note.merge_into(Some(&item.value))?;
    }
    Ok(())
}

pub(super) fn nonlinear_speed(item: &Object, event: &LineEvent) -> bool {
    item.event.is_some_and(|(_, kind)| kind == EventType::Speed)
        && event.start != event.end
        && (item
            .value
            .get("easingType")
            .and_then(Value::as_i64)
            .is_some_and(|v| v > 1)
            || item
                .value
                .get("bezier")
                .and_then(Value::as_u64)
                .is_some_and(|v| v != 0))
}

pub fn reversed_easing(event: &mut LineEvent) {
    if let Some(binding) = &mut event.custom_easing {
        binding.reverse([event.start, event.end]);
        if let (Ok(start), Ok(end)) = (
            binding.scalar_value([event.start, event.end], 0.),
            binding.scalar_value([event.start, event.end], 1.),
        ) {
            event.start = start;
            event.end = end;
            binding.canonicalize_scalar([start, end]);
        }
        return;
    }
    event.easing_type = match event.easing_type {
        2 => 3,
        3 => 2,
        4 => 5,
        5 => 4,
        8 => 9,
        9 => 8,
        10 => 11,
        11 => 10,
        14 => 15,
        15 => 14,
        16 => 17,
        17 => 16,
        18 => 19,
        19 => 18,
        20 => 21,
        21 => 20,
        24 => 25,
        25 => 24,
        26 => 27,
        27 => 26,
        other => other,
    };
    let left = event.easing_left;
    event.easing_left = 1. - event.easing_right;
    event.easing_right = 1. - left;
    let [x1, y1, x2, y2] = event.bezier_points;
    event.bezier_points = [1. - x2, 1. - y2, 1. - x1, 1. - y1];
}

pub fn transform(
    doc: &ChartDocument,
    selection: &Selection,
    operation: Transform,
) -> Result<ToolEdit, ChartError> {
    transform_bounds(doc, selection, operation, None)
}

pub(crate) fn transform_bounds(
    doc: &ChartDocument,
    selection: &Selection,
    operation: Transform,
    range: Option<(Beat, Beat)>,
) -> Result<ToolEdit, ChartError> {
    let mut items = objects(doc, selection)?;
    let (first, last) = range.map_or_else(|| bounds(&items), Ok)?;
    let mut ordered: Vec<usize> = (0..items.len())
        .filter(|i| items[*i].event.is_none())
        .collect();
    ordered.sort_by_key(|i| (items[*i].start().unwrap(), *i));
    let ranks: BTreeMap<usize, usize> = ordered
        .iter()
        .enumerate()
        .map(|(rank, i)| (*i, rank))
        .collect();
    for (i, item) in items.iter_mut().enumerate() {
        let start = item.start()?;
        let end = item.end()?;
        match operation {
            Transform::Shift { beats, x } => {
                if !x.is_finite() {
                    return Err(invalid("Horizontal shift must be finite"));
                }
                item.with_times(start.checked_add(beats)?, end.checked_add(beats)?)?;
                if item.event.is_none() && x != 0. {
                    let mut note: Note = serde_json::from_value(item.value.clone())?;
                    note.x += x;
                    item.value = note.merge_into(Some(&item.value))?;
                }
            }
            Transform::MirrorX(center) => mirror(item, center)?,
            Transform::ScaleX { anchor, factor } => {
                if !anchor.is_finite() || !factor.is_finite() {
                    return Err(invalid("Horizontal scale and anchor must be finite"));
                }
                if item.event.is_none() {
                    let mut note: Note = serde_json::from_value(item.value.clone())?;
                    let x = anchor + (note.x - anchor) * factor;
                    if x != note.x {
                        note.x = x;
                        note.validate()?;
                        item.value["positionX"] = serde_json::to_value(x)?;
                    }
                }
            }
            Transform::ReverseTime => {
                if let Some((_, kind)) = item.event
                    && nonlinear_speed(item, &LineEvent::read(kind, &item.value)?)
                {
                    return Err(invalid(
                        "Reverse requires Linear speed intervals; imported speed curve metadata is preserved",
                    ));
                }
                let end = item.extent_end()?;
                item.with_times(
                    first.checked_add(last.checked_sub(end)?)?,
                    first.checked_add(last.checked_sub(start)?)?,
                )?;
                if let Some((_, kind)) = item.event {
                    let mut event = LineEvent::read(kind, &item.value)?;
                    std::mem::swap(&mut event.start, &mut event.end);
                    if kind != EventType::Speed || event.custom_easing.is_some() {
                        reversed_easing(&mut event);
                    }
                    item.value = event.merge(kind, Some(&item.value))?;
                }
            }
            Transform::ScaleTime(factor) => {
                if factor <= Beat::ZERO {
                    return Err(invalid("Time scale must be positive"));
                }
                item.with_times(
                    first.checked_add(start.checked_sub(first)?.checked_mul(factor)?)?,
                    first.checked_add(end.checked_sub(first)?.checked_mul(factor)?)?,
                )?;
            }
            Transform::Quantize(division) => {
                let start = Beat::from_grid(start.as_f64(), division)?;
                let mut end = Beat::from_grid(end.as_f64(), division)?;
                let hold = item.event.is_some()
                    || serde_json::from_value::<Note>(item.value.clone())?.kind == NoteKind::Hold;
                if hold && end <= start {
                    end = start.checked_add(Beat::new(0, 1, division)?)?;
                }
                item.with_times(start, end)?;
            }
            Transform::AlignTime => {
                item.with_times(first, first.checked_add(end.checked_sub(start)?)?)?
            }
            Transform::ArrangeX { from, to } if item.event.is_none() => {
                let t = if ordered.len() < 2 {
                    0.
                } else {
                    ranks[&i] as f64 / (ordered.len() - 1) as f64
                };
                let mut note: Note = serde_json::from_value(item.value.clone())?;
                note.x = from + (to - from) * t;
                item.value = note.merge_into(Some(&item.value))?;
            }
            Transform::FlipSide
            | Transform::Side(_)
            | Transform::Fake(_)
            | Transform::NoteType { .. }
                if item.event.is_none() =>
            {
                let mut note: Note = serde_json::from_value(item.value.clone())?;
                match operation {
                    Transform::FlipSide => note.above = if note.above == 1 { 2 } else { 1 },
                    Transform::Side(side) => note.above = side,
                    Transform::Fake(fake) => note.is_fake = u8::from(fake),
                    Transform::NoteType { kind, hold_length } => {
                        if kind == NoteKind::Hold && hold_length <= Beat::ZERO {
                            return Err(invalid("Hold length must be positive"));
                        }
                        note.kind = kind;
                        note.end_time = if kind == NoteKind::Hold {
                            note.start_time.checked_add(hold_length)?
                        } else {
                            note.start_time
                        };
                    }
                    _ => unreachable!(),
                }
                item.value = note.merge_into(Some(&item.value))?;
            }
            Transform::Easing(easing) if item.event.is_some() => {
                let kind = item.event.unwrap().1;
                if kind == EventType::Speed && easing != 1 {
                    return Err(invalid(
                        "Speed events use Linear easing; select other event tracks",
                    ));
                }
                let mut event = LineEvent::read(kind, &item.value)?;
                event.easing_type = easing;
                event.custom_easing = None;
                event.bezier = 0;
                event.easing_left = 0.;
                event.easing_right = 1.;
                item.value = event.merge(kind, Some(&item.value))?;
            }
            Transform::Curve {
                easing,
                window,
                bezier,
            } if item.event.is_some() => {
                let kind = item.event.unwrap().1;
                if kind == EventType::Speed && (easing != 1 || bezier.is_some()) {
                    return Err(invalid(
                        "Speed events use Linear easing; select other event tracks",
                    ));
                }
                let mut event = LineEvent::read(kind, &item.value)?;
                event.custom_easing = None;
                event.easing_type = easing;
                event.easing_left = window[0];
                event.easing_right = window[1];
                event.bezier = u8::from(bezier.is_some());
                if let Some(points) = bezier {
                    event.bezier_points = points;
                }
                item.value = event.merge(kind, Some(&item.value))?;
            }
            Transform::ReverseValues if item.event.is_some() => {
                let kind = item.event.unwrap().1;
                let mut event = LineEvent::read(kind, &item.value)?;
                std::mem::swap(&mut event.start, &mut event.end);
                item.value = event.merge(kind, Some(&item.value))?;
            }
            _ => {}
        }
    }
    replacement(doc, selection, items, "Transform selection")
}

pub fn relocate(
    doc: &ChartDocument,
    selection: &Selection,
    line: usize,
    layer: usize,
    copy: bool,
) -> Result<ToolEdit, ChartError> {
    let mut items = objects(doc, selection)?;
    bounds(&items)?;
    for item in &mut items {
        item.line = line;
        if let Some((_, kind)) = item.event {
            item.event = Some((layer, kind));
        }
    }
    if copy {
        plan(doc, &Selection::default(), items, "Copy to line")
    } else {
        replacement(doc, selection, items, "Move to line")
    }
}

pub fn repeat(
    doc: &ChartDocument,
    selection: &Selection,
    count: usize,
    spacing: Beat,
) -> Result<ToolEdit, ChartError> {
    let source = objects(doc, selection)?;
    bounds(&source)?;
    if count == 0
        || spacing <= Beat::ZERO
        || source
            .len()
            .checked_mul(count)
            .is_none_or(|n| n > MAX_OBJECTS)
    {
        return Err(invalid(
            "Repeat requires positive spacing and at most 20,000 resulting objects",
        ));
    }
    let mut items = Vec::with_capacity(source.len() * count);
    for i in 1..=count {
        let delta = spacing.checked_mul(Beat::new(i as i32, 0, 1)?)?;
        for mut item in source.clone() {
            item.with_times(
                item.start()?.checked_add(delta)?,
                item.end()?.checked_add(delta)?,
            )?;
            items.push(item);
        }
    }
    plan(doc, &Selection::default(), items, "Repeat selection")
}

pub fn remove_duplicates(
    doc: &ChartDocument,
    selection: &Selection,
) -> Result<ToolEdit, ChartError> {
    let selection = selection.checked(doc)?;
    let mut seen = BTreeSet::new();
    let mut remove = Selection::default();
    for id in selection.notes {
        let value = &doc.as_json()["judgeLineList"][id.line]["notes"][id.index];
        let mut value = value.clone();
        canonical_order(&mut value);
        if !seen.insert((id.line, serde_json::to_string(&value)?)) {
            remove.notes.push(id);
        }
    }
    plan(doc, &remove, vec![], "Remove exact duplicates")
}

pub(super) fn canonical_order(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.sort_keys();
            for value in object.values_mut() {
                canonical_order(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                canonical_order(value);
            }
        }
        _ => {}
    }
}
