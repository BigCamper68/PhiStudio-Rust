use super::*;
use crate::{Beat, ChartDocument, Editor, EventId, EventType, LineEvent, TempoMap, tools};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const RECIPE_KEY: &str = "phiStudioEasing";
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Recipe {
    pub(super) version: u32,
    pub(super) curve: Curve,
    pub(super) source: Value,
    pub(super) count: usize,
    pub(super) signature: String,
}
/// A logical event can span several native intervals after custom easing.
#[derive(Clone, Debug)]
pub struct Source {
    pub id: EventId,
    pub members: Vec<EventId>,
    pub raw: Value,
    pub event: LineEvent,
    pub curve: Option<Curve>,
    pub next_start: Option<Beat>,
}
impl Source {
    pub fn capture(doc: &ChartDocument, id: EventId) -> Result<Self, ChartError> {
        if id.layer > 3 {
            return Err(invalid("Only event layers 0–3 are editable"));
        }
        let track = doc.event_values(id.line, id.layer, id.kind)?;
        let raw = track
            .get(id.index)
            .ok_or_else(|| invalid("The easing source event no longer exists"))?;
        if track.len() > tools::MAX_OBJECTS {
            return Err(invalid("The easing source track exceeds 20,000 events"));
        }
        let event = LineEvent::read(id.kind, raw)?;
        if let Some(binding) = &event.custom_easing {
            event.validate(id.kind)?;
            return Ok(Self {
                id,
                members: vec![id],
                raw: raw.clone(),
                curve: Some(binding.curve.clone()),
                next_start: next_start(track, id.kind, event.end_time, id.index..id.index + 1)?,
                event,
            });
        }
        let bpm = &doc.as_json()["BPMList"];
        for (first, value) in track.iter().enumerate().take(id.index + 1) {
            let Some(recipe) = value.get(RECIPE_KEY) else {
                continue;
            };
            // Check the member count before cloning an untrusted recipe body.
            let Some(count) = recipe
                .get("count")
                .and_then(Value::as_u64)
                .filter(|c| *c > 0 && *c <= tools::MAX_OBJECTS as u64)
                .map(|c| c as usize)
            else {
                continue;
            };
            let Some(end) = first
                .checked_add(count)
                .filter(|end| *end <= track.len() && id.index < *end)
            else {
                continue;
            };
            let Ok(recipe) = serde_json::from_value::<Recipe>(recipe.clone()) else {
                continue;
            };
            if recipe.version != 1 || recipe.curve.validate().is_err() {
                continue;
            }
            let Ok(event) = LineEvent::read(id.kind, &recipe.source) else {
                continue;
            };
            if event.validate(id.kind).is_err() {
                continue;
            }
            let pieces = &track[first..end];
            let Ok(signature) = signature(id.kind, &recipe.curve, &recipe.source, bpm, pieces)
            else {
                continue;
            };
            if signature != recipe.signature {
                continue;
            }
            let bound = event.end_time;
            let members = (first..end).map(|index| EventId { index, ..id }).collect();
            return Ok(Self {
                id: EventId { index: first, ..id },
                members,
                raw: recipe.source,
                event,
                curve: Some(recipe.curve),
                next_start: next_start(track, id.kind, bound, first..end)?,
            });
        }
        let event = LineEvent::read(id.kind, raw)?;
        event.validate(id.kind)?;
        let mut source = raw.clone();
        // Invalid hints are never nested into the next authoring recipe.
        source.as_object_mut().unwrap().shift_remove(RECIPE_KEY);
        Ok(Self {
            id,
            members: vec![id],
            raw: source,
            curve: None,
            next_start: next_start(track, id.kind, event.end_time, id.index..id.index + 1)?,
            event,
        })
    }
}
fn next_start(
    track: &[Value],
    kind: EventType,
    bound: Beat,
    excluded: std::ops::Range<usize>,
) -> Result<Option<Beat>, ChartError> {
    let mut result = None;
    for (i, raw) in track.iter().enumerate() {
        let event = LineEvent::read(kind, raw)?;
        if !excluded.contains(&i) && event.start_time >= bound {
            result = Some(result.map_or(event.start_time, |b: Beat| b.min(event.start_time)));
        }
    }
    Ok(result)
}
pub(super) fn signature(
    kind: EventType,
    curve: &Curve,
    source: &Value,
    bpm: &Value,
    values: &[Value],
) -> Result<String, ChartError> {
    let mut hasher = Sha256::new();
    for part in [
        serde_json::to_vec(&kind)?,
        serde_json::to_vec(curve)?,
        serde_json::to_vec(source)?,
        serde_json::to_vec(bpm)?,
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    for value in values {
        let mut value = value.clone();
        if let Some(object) = value.as_object_mut() {
            object.shift_remove(RECIPE_KEY);
        }
        let part = serde_json::to_vec(&value)?;
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
/// Bounded worker snapshot; it owns selected events rather than a chart clone.
#[derive(Clone)]
pub struct Snapshot {
    pub sources: Vec<Source>,
    tempo: TempoMap,
    bpm: Value,
    revision: u64,
    expected: Vec<(EventId, Value)>,
}
pub struct Edit {
    revision: u64,
    bpm: Value,
    expected: Vec<(EventId, Value)>,
    additions: Vec<(usize, usize, EventType, Value)>,
    compiled: Vec<(usize, usize, EventType, Value)>,
}
impl Snapshot {
    /// Stage an inspector draft while retaining the original source guards.
    pub fn capture_draft(
        doc: &ChartDocument,
        id: EventId,
        revision: u64,
        event: &LineEvent,
    ) -> Result<Self, ChartError> {
        let mut snapshot = Self::capture(doc, &[id], revision)?;
        let source = &mut snapshot.sources[0];
        let raw = event.merge(id.kind, Some(&source.raw))?;
        source.event = LineEvent::read(id.kind, &raw)?;
        source.event.validate(id.kind)?;
        source.curve = source.event.custom_easing.as_ref().map(|b| b.curve.clone());
        source.next_start = next_start(
            doc.event_values(id.line, id.layer, id.kind)?,
            id.kind,
            source.event.end_time,
            id.index..id.index + 1,
        )?;
        Ok(snapshot)
    }
    pub fn capture(
        doc: &ChartDocument,
        ids: &[EventId],
        revision: u64,
    ) -> Result<Self, ChartError> {
        if ids.is_empty() || ids.len() > tools::MAX_OBJECTS {
            return Err(invalid("Select 1–20,000 events for custom easing"));
        }
        let mut seen = BTreeSet::new();
        let mut sources = Vec::new();
        let mut expected = Vec::new();
        for &id in ids {
            if seen.contains(&(id.line, id.layer, id.kind, id.index)) {
                continue;
            }
            let source = Source::capture(doc, id)?;
            for member in &source.members {
                if !seen.insert((member.line, member.layer, member.kind, member.index)) {
                    return Err(invalid(
                        "The easing selection contains overlapping curve groups",
                    ));
                }
                expected.push((
                    *member,
                    doc.event_values(member.line, member.layer, member.kind)?[member.index].clone(),
                ));
            }
            if expected.len() > tools::MAX_OBJECTS {
                return Err(invalid("The easing selection exceeds 20,000 events"));
            }
            sources.push(source);
        }
        Ok(Self {
            sources,
            tempo: doc.tempo().clone(),
            bpm: doc.as_json()["BPMList"].clone(),
            revision,
            expected,
        })
    }
    pub fn prepare(self, curve: &Curve, canceled: impl Fn() -> bool) -> Result<Edit, ChartError> {
        self.prepare_impl(curve, false, canceled)
    }
    pub fn prepare_preserving_domains(
        self,
        curve: &Curve,
        canceled: impl Fn() -> bool,
    ) -> Result<Edit, ChartError> {
        self.prepare_impl(curve, true, canceled)
    }
    fn prepare_impl(
        self,
        curve: &Curve,
        preserve: bool,
        canceled: impl Fn() -> bool,
    ) -> Result<Edit, ChartError> {
        curve.validate()?;
        let mut additions = Vec::new();
        let mut compiled = Vec::new();
        for source in &self.sources {
            if canceled() {
                return Err(invalid("Custom easing preparation canceled"));
            }
            let mut event = source.event.clone();
            let mut binding = if preserve {
                event
                    .custom_easing
                    .clone()
                    .map(|binding| *binding)
                    .unwrap_or_else(|| Binding::new(curve.clone()))
            } else {
                Binding::new(curve.clone())
            };
            binding.curve = curve.clone();
            let ends = [event.start, event.end];
            if binding.source.is_some() {
                event.start = binding.scalar_value(ends, 0.)?;
                event.end = binding.scalar_value(ends, 1.)?;
            }
            event.custom_easing = Some(Box::new(binding));
            event.easing_type = 1;
            event.easing_left = 0.;
            event.easing_right = 1.;
            event.bezier = 0;
            let mut authored = event.merge(source.id.kind, Some(&source.raw))?;
            if source.id.kind == EventType::Speed {
                let canonical = serde_json::to_value(&event)?;
                for key in [
                    "easingType",
                    "easingLeft",
                    "easingRight",
                    "bezier",
                    "bezierPoints",
                ] {
                    authored[key] = canonical[key].clone();
                }
            }
            authored.as_object_mut().unwrap().shift_remove(RECIPE_KEY);
            let pieces = super::bake::bake_bound(
                &authored,
                source.id.kind,
                &self.tempo,
                source.next_start,
                &canceled,
            )?;
            if compiled.len() + pieces.len() > tools::MAX_OBJECTS {
                return Err(invalid("The custom easing exceeds 20,000 events"));
            }
            additions.push((source.id.line, source.id.layer, source.id.kind, authored));
            compiled.extend(
                pieces
                    .into_iter()
                    .map(|raw| (source.id.line, source.id.layer, source.id.kind, raw)),
            );
        }
        Ok(Edit {
            revision: self.revision,
            bpm: self.bpm,
            expected: self.expected,
            additions,
            compiled,
        })
    }
}

impl Edit {
    /// Logical authored records, kept separate from the native preview pieces.
    pub fn authored_events(&self) -> impl Iterator<Item = (usize, usize, EventType, &Value)> {
        self.additions
            .iter()
            .map(|(line, layer, kind, raw)| (*line, *layer, *kind, raw))
    }
    pub fn event_count(&self) -> usize {
        self.compiled.len()
    }
    pub fn preview_events(&self) -> impl Iterator<Item = (usize, usize, EventType, &Value)> {
        self.compiled
            .iter()
            .map(|(line, layer, kind, raw)| (*line, *layer, *kind, raw))
    }
    pub fn apply(
        self,
        editor: &mut Editor,
        linked: bool,
    ) -> Result<Option<tools::Selection>, ChartError> {
        if editor.revision() != self.revision
            || editor.document().as_json()["BPMList"] != self.bpm
            || self.expected.iter().any(|(id, raw)| {
                editor
                    .document()
                    .event_values(id.line, id.layer, id.kind)
                    .ok()
                    .and_then(|v| v.get(id.index))
                    != Some(raw)
            })
        {
            return Err(invalid(
                "The chart changed. Reopen custom easing before applying",
            ));
        }
        let remove: Vec<_> = self.expected.iter().map(|(id, _)| *id).collect();
        let edit = tools::generated_event_edit(
            editor.document(),
            &remove,
            &self.additions,
            "Apply custom easing",
        )?;
        if linked {
            edit.apply_linked(editor)
        } else {
            edit.apply(editor)
        }
    }
}
