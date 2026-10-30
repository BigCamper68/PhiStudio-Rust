use crate::{Beat, BpmChange, ChartDocument, ChartError, Command, EventId, LineEvent, TempoMap};
use crate::{
    array_edit::{ArrayEdit, Segment},
    line_properties::FieldChange,
};
use serde_json::json;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditTarget {
    Event(EventId),
    Bpm(usize),
    Line(usize),
}

#[derive(Debug)]
pub(crate) enum AuthoringDelta {
    Array {
        edit: Box<ArrayEdit>,
        tempo: Option<(TempoMap, TempoMap)>,
        target: Option<EditTarget>,
    },
    Properties {
        line: usize,
        fields: Vec<FieldChange>,
    },
}
impl AuthoringDelta {
    pub fn target(&self) -> Option<EditTarget> {
        match self {
            Self::Array { target, .. } => *target,
            Self::Properties { line, .. } => Some(EditTarget::Line(*line)),
        }
    }
    pub fn apply(&self, doc: &mut ChartDocument, forward: bool) {
        match self {
            Self::Array { edit, tempo, .. } => {
                edit.apply(&mut doc.root, forward);
                if let Some((before, after)) = tempo {
                    doc.set_tempo(if forward {
                        after.clone()
                    } else {
                        before.clone()
                    });
                }
            }
            Self::Properties { line, fields } => {
                for field in fields {
                    field.apply(
                        doc.root["judgeLineList"][*line].as_object_mut().unwrap(),
                        forward,
                    );
                }
            }
        }
    }
}
fn event_delta(
    doc: &ChartDocument,
    id: EventId,
    event: Option<LineEvent>,
    inserting: bool,
) -> Result<Option<AuthoringDelta>, ChartError> {
    if id.layer > 3 {
        return Err(ChartError::Invalid(
            "Only event layers 0–3 are editable".into(),
        ));
    }
    let raw = doc.event_values(id.line, id.layer, id.kind)?;
    let before = if inserting {
        None
    } else {
        Some(
            raw.get(id.index)
                .ok_or_else(|| ChartError::Invalid("Event no longer exists".into()))?,
        )
    };
    // Removing an entry must keep the indices/order of the remaining entries.
    // This also allows descending-index removals inside a transaction.
    let mut timing = event.is_some();
    let after = if let Some(event) = event {
        if let Some(before) = before {
            let old = LineEvent::read(id.kind, before)?;
            if old == event {
                return Ok(None);
            }
            timing = old.start_time != event.start_time || old.end_time != event.end_time;
        }
        doc.validate_event(
            id.line,
            id.layer,
            id.kind,
            &event,
            if inserting { None } else { Some(id.index) },
        )?;
        Some(event.merge(id.kind, before)?)
    } else {
        None
    };
    let deleting = after.is_none();
    let edit = ArrayEdit::prepare(
        &doc.root,
        id.kind.path(id.line, id.layer),
        id.index,
        !inserting,
        after,
        timing,
        true,
    )?;
    let target = (!deleting).then_some(EditTarget::Event(EventId {
        index: edit.after_index,
        ..id
    }));
    Ok(Some(AuthoringDelta::Array {
        edit: Box::new(edit),
        tempo: None,
        target,
    }))
}
fn bpm_delta(
    doc: &ChartDocument,
    index: usize,
    change: Option<BpmChange>,
    inserting: bool,
) -> Result<Option<AuthoringDelta>, ChartError> {
    let values = doc.bpm_changes();
    let first = values.iter().min_by_key(|(_, v)| v.start).unwrap().0;
    let original = if inserting {
        None
    } else {
        Some(
            values
                .get(index)
                .ok_or_else(|| ChartError::Invalid("BPM entry no longer exists".into()))?
                .1,
        )
    };
    let after = if let Some(change) = change {
        if original == Some(change) {
            return Ok(None);
        }
        if change.start < Beat::ZERO || !change.bpm.is_finite() || change.bpm <= 0. {
            return Err(ChartError::Invalid(
                "BPM must be positive and finite; its beat must be nonnegative".into(),
            ));
        }
        let earliest = values[first].1.start;
        if (!inserting && index == first && change.start != earliest)
            || ((inserting || index != first) && change.start <= earliest)
        {
            return Err(ChartError::Invalid("The first BPM beat is locked".into()));
        }
        if values
            .iter()
            .any(|(i, v)| (inserting || *i != index) && v.start == change.start)
        {
            return Err(ChartError::Invalid(
                "A BPM entry already starts at that beat".into(),
            ));
        }
        let mut raw = if inserting {
            json!({})
        } else {
            doc.root["BPMList"][index].clone()
        };
        if original.is_none_or(|v| v.start != change.start) {
            raw["startTime"] = serde_json::to_value(change.start)?;
        }
        if original.is_none_or(|v| v.bpm != change.bpm) {
            raw["bpm"] = json!(change.bpm);
        }
        Some(raw)
    } else {
        if index == first {
            return Err(ChartError::Invalid(
                "The first BPM entry cannot be deleted".into(),
            ));
        }
        None
    };
    let deleting = after.is_none();
    let edit = ArrayEdit::prepare(
        &doc.root,
        vec![Segment::Field("BPMList")],
        index,
        !inserting,
        after,
        true,
        false,
    )?;
    let mut shadow = json!({"BPMList":doc.root["BPMList"].clone()});
    edit.apply(&mut shadow, true);
    let next = TempoMap::new(serde_json::from_value(shadow["BPMList"].clone())?)?;
    let target = (!deleting).then_some(EditTarget::Bpm(edit.after_index));
    Ok(Some(AuthoringDelta::Array {
        edit: Box::new(edit),
        tempo: Some((doc.tempo().clone(), next)),
        target,
    }))
}

pub(crate) fn prepare(
    doc: &ChartDocument,
    command: Command,
) -> Result<Option<(AuthoringDelta, &'static str)>, ChartError> {
    let (delta, label) = match command {
        Command::InsertEvent {
            line,
            layer,
            kind,
            value,
        } => {
            let event = LineEvent::read(kind, &value)?;
            doc.validate_event(line, layer, kind, &event, None)?;
            let index = doc.event_values(line, layer, kind)?.len();
            let edit = ArrayEdit::prepare(
                &doc.root,
                kind.path(line, layer),
                index,
                false,
                Some(value),
                true,
                true,
            )?;
            let target = Some(EditTarget::Event(EventId {
                line,
                layer,
                kind,
                index: edit.after_index,
            }));
            (
                Some(AuthoringDelta::Array {
                    edit: Box::new(edit),
                    tempo: None,
                    target,
                }),
                "Paste event",
            )
        }
        Command::AddEvent {
            line,
            layer,
            kind,
            event,
        } => {
            let index = doc.event_values(line, layer, kind)?.len();
            (
                event_delta(
                    doc,
                    EventId {
                        line,
                        layer,
                        kind,
                        index,
                    },
                    Some(event),
                    true,
                )?,
                "Add event",
            )
        }
        Command::UpdateEvent { id, event } => {
            (event_delta(doc, id, Some(event), false)?, "Edit event")
        }
        Command::DeleteEvent(id) => (event_delta(doc, id, None, false)?, "Delete event"),
        Command::AddBpm(change) => (
            bpm_delta(doc, doc.bpm_changes().len(), Some(change), true)?,
            "Add BPM",
        ),
        Command::UpdateBpm { index, change } => {
            (bpm_delta(doc, index, Some(change), false)?, "Edit BPM")
        }
        Command::DeleteBpm(index) => (bpm_delta(doc, index, None, false)?, "Delete BPM"),
        Command::EditLine { index, properties } => {
            let fields = properties.changes(doc, index)?;
            if fields.is_empty() {
                return Ok(None);
            }
            (
                Some(AuthoringDelta::Properties {
                    line: index,
                    fields,
                }),
                "Edit line",
            )
        }
        _ => unreachable!(),
    };
    Ok(delta.map(|delta| (delta, label)))
}
