//! Selection tools retain raw RPE extensions and prepare one reversible transaction.
mod selection;
pub use selection::Selection;
mod transaction;
pub use transaction::delete;
use transaction::{plan, replacement};
mod clipboard;
pub use clipboard::ChartClipboard;
mod transform;
pub(crate) use transform::transform_bounds;
pub use transform::{Transform, relocate, remove_duplicates, repeat, reversed_easing, transform};
use transform::{mirror, nonlinear_speed};
mod batch;
pub use batch::{ValueField, ValueMode, batch_values};
mod events;
pub use events::{
    connect_events, create_event, join_events, subdivide_events, synchronize_xy, update_event,
    update_event_auto,
};
mod audio_notes;
mod generate;
use crate::{
    Beat, ChartDocument, ChartError, Editor, EventId, EventType, LineEvent, Note, NoteId, NoteKind,
};
pub use audio_notes::{
    AudioCollision, AudioGeneratedNote, AudioMarker, AudioNoteOptions, AudioNotePreview,
    AudioNoteSpec, AudioNoteStats, AudioPosition, AudioSide, insert_audio_notes,
    preview_audio_notes,
};
pub(crate) use generate::generated_event_edit;
pub use generate::{NotePattern, Pattern, Random, generate_motion, generate_notes};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_OBJECTS: usize = 20_000;
fn invalid(message: &str) -> ChartError {
    ChartError::Invalid(message.into())
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Object {
    line: usize,
    event: Option<(usize, EventType)>,
    value: Value,
}
impl Object {
    fn start(&self) -> Result<Beat, ChartError> {
        if let Some((_, kind)) = self.event {
            Ok(LineEvent::read(kind, &self.value)?.start_time)
        } else {
            Ok(serde_json::from_value::<Note>(self.value.clone())?.start_time)
        }
    }
    fn end(&self) -> Result<Beat, ChartError> {
        if let Some((_, kind)) = self.event {
            Ok(LineEvent::read(kind, &self.value)?.end_time)
        } else {
            Ok(serde_json::from_value::<Note>(self.value.clone())?.end_time)
        }
    }
    fn extent_end(&self) -> Result<Beat, ChartError> {
        if self.event.is_some()
            || serde_json::from_value::<Note>(self.value.clone())?.kind == NoteKind::Hold
        {
            self.end()
        } else {
            self.start()
        }
    }
    fn with_times(&mut self, start: Beat, end: Beat) -> Result<(), ChartError> {
        if start < Beat::ZERO {
            return Err(invalid(
                "The operation would move an object before beat zero",
            ));
        }
        if self.start()? != start {
            self.value["startTime"] = serde_json::to_value(start)?;
        }
        if self.end()? != end {
            self.value["endTime"] = serde_json::to_value(end)?;
        }
        Ok(())
    }
}
fn objects(doc: &ChartDocument, selection: &Selection) -> Result<Vec<Object>, ChartError> {
    let selection = selection.checked(doc)?;
    let mut items = Vec::with_capacity(selection.len());
    for id in selection.notes {
        items.push(Object {
            line: id.line,
            event: None,
            value: doc.as_json()["judgeLineList"][id.line]["notes"][id.index].clone(),
        });
    }
    for id in selection.events {
        items.push(Object {
            line: id.line,
            event: Some((id.layer, id.kind)),
            value: doc.event_values(id.line, id.layer, id.kind)?[id.index].clone(),
        });
    }
    Ok(items)
}
fn bounds(items: &[Object]) -> Result<(Beat, Beat), ChartError> {
    let mut first = None;
    let mut last = Beat::ZERO;
    for item in items {
        let start = item.start()?;
        first = Some(first.map_or(start, |b: Beat| b.min(start)));
        last = last.max(item.extent_end()?);
    }
    Ok((
        first.ok_or_else(|| invalid("Select notes or events first"))?,
        last,
    ))
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Track {
    Notes(usize),
    Events(usize, usize, EventType),
}
impl Track {
    fn path(self) -> Vec<crate::array_edit::Segment> {
        use crate::array_edit::Segment::*;
        match self {
            Self::Notes(line) => vec![Field("judgeLineList"), Index(line), Field("notes")],
            Self::Events(line, layer, kind) => kind.path(line, layer),
        }
    }
}
struct TrackEdit {
    remove: Vec<usize>,
    add: Vec<Value>,
}
pub struct ToolEdit {
    label: &'static str,
    tracks: BTreeMap<Track, TrackEdit>,
}
#[derive(Debug)]
pub(crate) struct ToolDelta {
    arrays: Vec<crate::array_edit::ArrayBatch>,
    counts: Vec<(usize, Option<Value>, Value)>,
}

#[path = "../clone_events.rs"]
mod clone_operation;
pub use clone_operation::{CloneSpec, ValueProfile, clone_events};
