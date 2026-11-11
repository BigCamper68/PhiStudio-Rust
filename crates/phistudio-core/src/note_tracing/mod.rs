//! Sampled note motion becomes ordinary, reversible RPE line events.
mod prepare;
mod sampling;
use crate::{
    Beat, ChartDocument, ChartError, Editor, EventId, EventType, LineEvent, NoteId, TempoMap, tools,
};
pub use sampling::{TracePoint, TraceTime, sampling_times};
use serde_json::Value;

pub const MAX_SAMPLES: usize = 60_000;
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub source: NoteId,
    pub destination: Option<usize>,
    pub layer: usize,
    pub range: [Beat; 2],
    pub sample_rate: u32,
    pub rotation: bool,
    pub replace: bool,
    pub position_tolerance: f64,
    pub rotation_tolerance: f64,
}
impl Spec {
    pub fn validate(&self) -> Result<(), ChartError> {
        if self.layer > 3
            || self.range[0] < Beat::ZERO
            || self.range[1] <= self.range[0]
            || ![60, 120, 240].contains(&self.sample_rate)
            || !self.position_tolerance.is_finite()
            || self.position_tolerance <= 0.
            || !self.rotation_tolerance.is_finite()
            || self.rotation_tolerance <= 0.
        {
            return Err(invalid(
                "Choose an ordered nonnegative tracing range, layer 0–3 and positive tolerances",
            ));
        }
        Ok(())
    }
}
fn invalid(message: &str) -> ChartError {
    ChartError::Invalid(message.into())
}
struct Existing {
    id: EventId,
    raw: Value,
    event: LineEvent,
}
/// A focused worker snapshot owns only overlapping destination events and recipes.
pub struct Snapshot {
    pub spec: Spec,
    pub tempo: TempoMap,
    pub native_components: [Option<usize>; 3],
    pub(crate) line: usize,
    pub(crate) line_count: usize,
    existing: Vec<Existing>,
    recipes: Option<Value>,
    right_continuations: [bool; 3],
    next_events: [Option<Beat>; 3],
}
impl Snapshot {
    pub fn capture(doc: &ChartDocument, spec: Spec) -> Result<Self, ChartError> {
        spec.validate()?;
        doc.note(spec.source.line, spec.source.index)?;
        let line_count = doc.line_count();
        let line = spec.destination.unwrap_or(line_count);
        if spec.destination.is_some_and(|line| line >= line_count) {
            return Err(invalid("Tracing destination no longer exists"));
        }
        let created = spec
            .destination
            .is_none()
            .then(|| ChartDocument::new_line("Note trace"));
        let target = created
            .as_ref()
            .unwrap_or_else(|| &doc.as_json()["judgeLineList"][line]);
        let layers = target["eventLayers"].as_array();
        let mut native_components = [None; 3];
        let mut existing = Vec::new();
        let mut right_continuations = [false; 3];
        let mut next_events = [None; 3];
        for (axis, kind) in [EventType::MoveX, EventType::MoveY, EventType::Rotate]
            .into_iter()
            .enumerate()
        {
            let values = layers
                .and_then(|layers| layers.get(spec.layer))
                .and_then(|layer| layer[kind.key()].as_array());
            if let Some(values) = values {
                native_components[axis] = Some(
                    layers
                        .unwrap()
                        .iter()
                        .take(spec.layer)
                        .filter(|layer| layer[kind.key()].is_array())
                        .count(),
                );
                if kind == EventType::Rotate && !spec.rotation {
                    continue;
                }
                for (index, raw) in values.iter().enumerate() {
                    let a = raw
                        .get("startTime")
                        .map(|v| serde_json::from_value::<Beat>(v.clone()))
                        .transpose()?
                        .unwrap_or(Beat::ZERO);
                    let b = raw
                        .get("endTime")
                        .map(|v| serde_json::from_value::<Beat>(v.clone()))
                        .transpose()?
                        .unwrap_or(Beat::ONE);
                    right_continuations[axis] |=
                        a == spec.range[1] || (a < spec.range[1] && b > spec.range[1]);
                    if a > spec.range[1] {
                        next_events[axis] =
                            Some(next_events[axis].map_or(a, |old: Beat| old.min(a)));
                    }
                    if (a < spec.range[1] && b > spec.range[0])
                        || (a == b && a >= spec.range[0] && a < spec.range[1])
                    {
                        if existing.len() >= tools::MAX_OBJECTS {
                            return Err(invalid(
                                "Tracing replaces at most 20,000 events; choose a shorter range",
                            ));
                        }
                        existing.push(Existing {
                            id: EventId {
                                line,
                                layer: spec.layer,
                                kind,
                                index,
                            },
                            event: LineEvent::read(kind, raw)?,
                            raw: raw.clone(),
                        });
                    }
                }
            }
        }
        Ok(Self {
            spec,
            tempo: doc.tempo().clone(),
            native_components,
            line,
            line_count,
            existing,
            right_continuations,
            next_events,
            recipes: target.get(crate::trajectories::RECIPE_FIELD).cloned(),
        })
    }
    pub fn has_conflicts(&self) -> bool {
        self.spec.destination.is_some() && !self.existing.is_empty()
    }
}

pub struct Edit {
    pub(crate) revision: u64,
    pub(crate) line_count: usize,
    pub(crate) line: usize,
    pub(crate) new_line: bool,
    pub(crate) range: [Beat; 2],
    pub(crate) remove: Vec<EventId>,
    pub(crate) additions: Vec<(usize, usize, EventType, Value)>,
    pub(crate) recipe_change: Option<(Value, Option<Value>)>,
}
impl Edit {
    pub fn event_count(&self) -> usize {
        self.additions.len()
    }
    pub fn apply(self, editor: &mut Editor) -> Result<tools::Selection, ChartError> {
        editor.execute_note_trace(self)
    }
}
