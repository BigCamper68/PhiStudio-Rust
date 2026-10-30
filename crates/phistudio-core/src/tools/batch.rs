use super::*;

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum ValueField {
    EventEasing,
    X,
    Size,
    Speed,
    Alpha,
    VisibleTime,
    YOffset,
    EventStart,
    EventEnd,
    EventBoth,
    LinkGroup,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum ValueMode {
    Set,
    Add,
    Multiply,
    Min,
    Max,
}

impl ValueMode {
    fn apply(self, old: f64, value: f64) -> f64 {
        match self {
            Self::Set => value,
            Self::Add => old + value,
            Self::Multiply => old * value,
            Self::Min => old.min(value),
            Self::Max => old.max(value),
        }
    }
}

pub fn batch_values(
    doc: &ChartDocument,
    selection: &Selection,
    field: ValueField,
    mode: ValueMode,
    values: &[f64],
) -> Result<ToolEdit, ChartError> {
    if values.is_empty() || values.len() > MAX_OBJECTS || values.iter().any(|v| !v.is_finite()) {
        return Err(invalid("Value profile must contain finite numbers"));
    }
    let mut items = objects(doc, selection)?;
    bounds(&items)?;
    let event_field = matches!(
        field,
        ValueField::EventEasing
            | ValueField::EventStart
            | ValueField::EventEnd
            | ValueField::EventBoth
            | ValueField::LinkGroup
    );
    let mut indices: Vec<usize> = (0..items.len())
        .filter(|i| items[*i].event.is_some() == event_field)
        .collect();
    if indices.is_empty() {
        return Err(invalid("Select objects matching the chosen field"));
    }
    indices.sort_by_key(|i| (items[*i].start().unwrap(), *i));
    for (at, i) in indices.into_iter().enumerate() {
        let item = &mut items[i];
        let value = values[at % values.len()];
        if let Some((_, kind)) = item.event {
            let mut event = LineEvent::read(kind, &item.value)?;
            match field {
                ValueField::EventEasing => {
                    event.easing_type =
                        mode.apply(f64::from(event.easing_type), value).round() as i32;
                    event.bezier = 0;
                    event.easing_left = 0.;
                    event.easing_right = 1.;
                }
                ValueField::EventStart => event.start = mode.apply(event.start, value),
                ValueField::EventEnd => event.end = mode.apply(event.end, value),
                ValueField::EventBoth => {
                    event.start = mode.apply(event.start, value);
                    event.end = mode.apply(event.end, value);
                }
                ValueField::LinkGroup => {
                    let result = mode.apply(f64::from(event.link_group), value).round();
                    if !result.is_finite() || !(0. ..=i32::MAX as f64).contains(&result) {
                        return Err(invalid("Link group is outside the integer range"));
                    }
                    event.link_group = result as i32;
                }
                _ => unreachable!(),
            }
            if event.constant {
                if field == ValueField::EventEnd {
                    event.start = event.end;
                } else {
                    event.end = event.start;
                }
            }
            item.value = event.merge(kind, Some(&item.value))?;
        } else {
            let mut note: Note = serde_json::from_value(item.value.clone())?;
            let target = match field {
                ValueField::X => &mut note.x,
                ValueField::Size => &mut note.size,
                ValueField::Speed => &mut note.speed,
                ValueField::VisibleTime => &mut note.visible_time,
                ValueField::YOffset => &mut note.y_offset,
                ValueField::Alpha => {
                    let result = mode.apply(f64::from(note.alpha), value).round();
                    if !result.is_finite() || !(0. ..=u16::MAX as f64).contains(&result) {
                        return Err(invalid("Note alpha is outside the native integer range"));
                    }
                    note.alpha = result as u16;
                    item.value = note.merge_into(Some(&item.value))?;
                    continue;
                }
                _ => unreachable!(),
            };
            *target = mode.apply(*target, value);
            item.value = note.merge_into(Some(&item.value))?;
        }
    }
    replacement(doc, selection, items, "Batch values")
}
