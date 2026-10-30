//! Sparse multi-track Storyboard operations and portable raw-data clipboard.
use crate::{
    Beat, ChartDocument, ChartError, Editor, LineEvent, StoryEvent, StoryKind,
    advanced::AdvancedDelta,
    array_edit::{ArrayBatch, Segment},
    tools::Transform,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StoryId {
    pub line: usize,
    pub kind: StoryKind,
    pub index: usize,
}

fn invalid(message: &str) -> ChartError {
    ChartError::Invalid(message.into())
}
fn raw(doc: &ChartDocument, id: StoryId) -> Result<Value, ChartError> {
    doc.root["judgeLineList"]
        .get(id.line)
        .and_then(|l| l.get("extended"))
        .and_then(|e| e.get(id.kind.key()))
        .and_then(|v| v.get(id.index))
        .cloned()
        .ok_or_else(|| invalid("Storyboard event no longer exists"))
}
pub fn checked(doc: &ChartDocument, ids: &[StoryId]) -> Result<Vec<StoryId>, ChartError> {
    let ids: Vec<_> = ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if ids.len() > crate::tools::MAX_OBJECTS {
        return Err(invalid("Select at most 20,000 Storyboard events"));
    }
    for id in &ids {
        if !StoryKind::EDITABLE.contains(&id.kind) {
            return Err(invalid("This Storyboard track is read only"));
        }
        serde_json::from_value::<StoryEvent>(raw(doc, *id)?)?.validate(id.kind)?;
    }
    Ok(ids)
}
pub fn all(doc: &ChartDocument, line: usize) -> Result<Vec<StoryId>, ChartError> {
    let mut ids = vec![];
    for kind in StoryKind::EDITABLE {
        ids.extend(
            (0..doc.story_events(line, kind)?.len()).map(|index| StoryId { line, kind, index }),
        );
    }
    checked(doc, &ids)
}
pub fn range(
    doc: &ChartDocument,
    line: usize,
    start: Beat,
    end: Beat,
) -> Result<Vec<StoryId>, ChartError> {
    if end < start {
        return Err(invalid("Selection range must be ordered"));
    }
    let mut ids = vec![];
    for kind in StoryKind::EDITABLE {
        ids.extend(
            doc.story_events(line, kind)?
                .into_iter()
                .enumerate()
                .filter(|(_, e)| e.start_time <= end && e.end_time >= start)
                .map(|(index, _)| StoryId { line, kind, index }),
        );
    }
    checked(doc, &ids)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoryObject {
    pub line: usize,
    pub kind: StoryKind,
    pub value: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoryClipboard {
    pub first: Beat,
    pub last: Beat,
    pub origin_line: usize,
    pub objects: Vec<StoryObject>,
}
impl StoryClipboard {
    pub fn copy(doc: &ChartDocument, ids: &[StoryId]) -> Result<Self, ChartError> {
        let ids = checked(doc, ids)?;
        if ids.is_empty() {
            return Err(invalid("Select Storyboard events first"));
        }
        let objects: Vec<_> = ids
            .iter()
            .map(|id| {
                Ok(StoryObject {
                    line: id.line,
                    kind: id.kind,
                    value: raw(doc, *id)?,
                })
            })
            .collect::<Result<_, ChartError>>()?;
        let mut first = None;
        let mut last = Beat::ZERO;
        for object in &objects {
            let event: StoryEvent = serde_json::from_value(object.value.clone())?;
            first = Some(first.map_or(event.start_time, |v: Beat| v.min(event.start_time)));
            last = last.max(event.end_time);
        }
        Ok(Self {
            first: first.unwrap(),
            last,
            origin_line: ids.iter().map(|i| i.line).min().unwrap(),
            objects,
        })
    }
    pub fn paste(
        &self,
        doc: &ChartDocument,
        line: usize,
        at: Beat,
    ) -> Result<StoryEdit, ChartError> {
        if self.objects.len() > crate::tools::MAX_OBJECTS {
            return Err(invalid("Pattern exceeds 20,000 Storyboard events"));
        }
        let shift = at.checked_sub(self.first)?;
        let mut edit = StoryEdit::new("Paste Storyboard events");
        for object in &self.objects {
            let target = object
                .line
                .checked_sub(self.origin_line)
                .and_then(|v| line.checked_add(v))
                .ok_or_else(|| invalid("Invalid destination line"))?;
            if target >= doc.line_count() {
                return Err(invalid("The destination needs more judge lines"));
            }
            let mut event: StoryEvent = serde_json::from_value(object.value.clone())?;
            event.start_time = event.start_time.checked_add(shift)?;
            event.end_time = event.end_time.checked_add(shift)?;
            edit.add(
                target,
                object.kind,
                event.merge(object.kind, Some(&object.value))?,
            );
        }
        Ok(edit)
    }
}

#[derive(Clone, Debug, Default)]
struct Changes {
    remove: BTreeSet<usize>,
    add: Vec<Value>,
}
#[derive(Clone, Debug)]
pub struct StoryEdit {
    changes: BTreeMap<(usize, StoryKind), Changes>,
    pub(crate) label: &'static str,
}
type PreparedStoryEdit = (AdvancedDelta, Vec<StoryId>, &'static str);
impl StoryEdit {
    pub fn new(label: &'static str) -> Self {
        Self {
            changes: BTreeMap::new(),
            label,
        }
    }
    pub fn add(&mut self, line: usize, kind: StoryKind, value: Value) {
        self.changes
            .entry((line, kind))
            .or_default()
            .add
            .push(value);
    }
    pub fn remove(&mut self, id: StoryId) {
        self.changes
            .entry((id.line, id.kind))
            .or_default()
            .remove
            .insert(id.index);
    }
    pub fn apply(self, editor: &mut Editor) -> Result<Option<Vec<StoryId>>, ChartError> {
        editor.execute_story_tool(self)
    }
    pub(crate) fn prepare_apply(
        self,
        doc: &mut ChartDocument,
    ) -> Result<Option<PreparedStoryEdit>, ChartError> {
        if self
            .changes
            .values()
            .map(|v| v.remove.len() + v.add.len())
            .sum::<usize>()
            > 40_000
        {
            return Err(invalid("Storyboard operation exceeds 40,000 changes"));
        }
        let mut edits: Vec<ArrayBatch> = vec![];
        let mut selected = vec![];
        for ((line, kind), change) in self.changes {
            let result = (|| {
                if !StoryKind::EDITABLE.contains(&kind) {
                    return Err(invalid("This Storyboard track is read only"));
                }
                let before = doc.story_events(line, kind)?;
                if change.remove.iter().any(|i| *i >= before.len()) {
                    return Err(invalid("Storyboard event no longer exists"));
                }
                let mut intervals: Vec<_> = before
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| !change.remove.contains(i))
                    .map(|(_, e)| (e.start_time, e.end_time, false))
                    .collect();
                for value in &change.add {
                    let event: StoryEvent = serde_json::from_value(value.clone())?;
                    event.validate(kind)?;
                    if event.start_time < Beat::ZERO {
                        return Err(invalid("Storyboard events cannot start before beat zero"));
                    }
                    intervals.push((event.start_time, event.end_time, true));
                }
                intervals.sort_unstable();
                let mut last_old = None;
                let mut last_new = None;
                for (start, end, added) in intervals {
                    if last_new.is_some_and(|v| start < v)
                        || (added && last_old.is_some_and(|v| start < v))
                    {
                        return Err(invalid("Storyboard events in one track cannot overlap"));
                    }
                    let last = if added { &mut last_new } else { &mut last_old };
                    *last = Some(last.map_or(end, |v: Beat| v.max(end)));
                }
                let sort = !change.add.is_empty();
                ArrayBatch::prepare(
                    &doc.root,
                    vec![
                        Segment::Field("judgeLineList"),
                        Segment::Index(line),
                        Segment::Field("extended"),
                        Segment::Field(kind.key()),
                    ],
                    &change.remove.into_iter().collect::<Vec<_>>(),
                    change.add,
                    sort,
                    true,
                )
            })();
            let edit = match result {
                Ok(edit) => edit,
                Err(error) => {
                    for edit in edits.iter().rev() {
                        edit.apply(&mut doc.root, false);
                    }
                    return Err(error);
                }
            };
            selected.extend(edit.added_indices.iter().map(|index| StoryId {
                line,
                kind,
                index: *index,
            }));
            edit.apply(&mut doc.root, true);
            edits.push(edit);
        }
        Ok((!edits.is_empty()).then_some((AdvancedDelta::StoryBatch(edits), selected, self.label)))
    }
}

pub fn delete(doc: &ChartDocument, ids: &[StoryId]) -> Result<StoryEdit, ChartError> {
    let mut edit = StoryEdit::new("Delete Storyboard selection");
    for id in checked(doc, ids)? {
        edit.remove(id);
    }
    Ok(edit)
}
pub fn properties(
    doc: &ChartDocument,
    ids: &[StoryId],
    kind: StoryKind,
    template: &StoryEvent,
) -> Result<StoryEdit, ChartError> {
    template.validate(kind)?;
    let mut edit = StoryEdit::new("Edit Storyboard selection properties");
    for id in checked(doc, ids)?.into_iter().filter(|id| id.kind == kind) {
        let value = raw(doc, id)?;
        let mut event: StoryEvent = serde_json::from_value(value.clone())?;
        event.start = template.start.clone();
        event.end = template.end.clone();
        event.easing_type = template.easing_type;
        event.easing_left = template.easing_left;
        event.easing_right = template.easing_right;
        event.bezier = template.bezier;
        event.bezier_points = template.bezier_points;
        event.linkgroup = template.linkgroup;
        let changed = event.merge(kind, Some(&value))?;
        if changed != value {
            edit.remove(id);
            edit.add(id.line, id.kind, changed);
        }
    }
    Ok(edit)
}
pub fn transform(
    doc: &ChartDocument,
    ids: &[StoryId],
    operation: Transform,
) -> Result<StoryEdit, ChartError> {
    transform_bounds(doc, ids, operation, None)
}
pub(crate) fn transform_bounds(
    doc: &ChartDocument,
    ids: &[StoryId],
    operation: Transform,
    range: Option<(Beat, Beat)>,
) -> Result<StoryEdit, ChartError> {
    let ids = checked(doc, ids)?;
    let mut clipboard = StoryClipboard::copy(doc, &ids)?;
    if let Some((first, last)) = range {
        clipboard.first = first;
        clipboard.last = last;
    }
    let mut edit = StoryEdit::new("Transform Storyboard selection");
    for id in ids {
        let value = raw(doc, id)?;
        let mut event: StoryEvent = serde_json::from_value(value.clone())?;
        match operation {
            Transform::Shift { beats, .. } => {
                event.start_time = event.start_time.checked_add(beats)?;
                event.end_time = event.end_time.checked_add(beats)?;
            }
            Transform::ScaleTime(factor) => {
                if factor <= Beat::ZERO {
                    return Err(invalid("Time scale must be positive"));
                }
                event.start_time = clipboard.first.checked_add(
                    event
                        .start_time
                        .checked_sub(clipboard.first)?
                        .checked_mul(factor)?,
                )?;
                event.end_time = clipboard.first.checked_add(
                    event
                        .end_time
                        .checked_sub(clipboard.first)?
                        .checked_mul(factor)?,
                )?;
            }
            Transform::Quantize(division) => {
                event.start_time = Beat::from_grid(event.start_time.as_f64(), division)?;
                event.end_time = Beat::from_grid(event.end_time.as_f64(), division)?
                    .max(event.start_time.checked_add(Beat::new(0, 1, division)?)?);
            }
            Transform::AlignTime => {
                event.end_time = clipboard
                    .first
                    .checked_add(event.end_time.checked_sub(event.start_time)?)?;
                event.start_time = clipboard.first;
            }
            Transform::ReverseTime => {
                let start = clipboard
                    .first
                    .checked_add(clipboard.last.checked_sub(event.end_time)?)?;
                event.end_time = clipboard
                    .first
                    .checked_add(clipboard.last.checked_sub(event.start_time)?)?;
                event.start_time = start;
                std::mem::swap(&mut event.start, &mut event.end);
                let mut curve = LineEvent {
                    easing_type: event.easing_type,
                    easing_left: event.easing_left,
                    easing_right: event.easing_right,
                    bezier: event.bezier,
                    bezier_points: event.bezier_points,
                    ..Default::default()
                };
                crate::tools::reversed_easing(&mut curve);
                event.easing_type = curve.easing_type;
                event.easing_left = curve.easing_left;
                event.easing_right = curve.easing_right;
                event.bezier = curve.bezier;
                event.bezier_points = curve.bezier_points;
            }
            Transform::ReverseValues => std::mem::swap(&mut event.start, &mut event.end),
            Transform::Easing(easing) => {
                event.easing_type = easing;
                event.bezier = 0;
            }
            Transform::Curve {
                easing,
                window,
                bezier,
            } => {
                event.easing_type = easing;
                event.easing_left = window[0];
                event.easing_right = window[1];
                event.bezier = u8::from(bezier.is_some());
                event.bezier_points = bezier.unwrap_or([0., 0., 1., 1.]);
            }
            _ => {
                return Err(invalid(
                    "This transform does not apply to Storyboard events",
                ));
            }
        }
        let changed = event.merge(id.kind, Some(&value))?;
        if changed != value {
            edit.remove(id);
            edit.add(id.line, id.kind, changed);
        }
    }
    Ok(edit)
}

pub fn repeat(
    doc: &ChartDocument,
    ids: &[StoryId],
    count: usize,
    spacing: Beat,
    target_line: Option<usize>,
    move_source: bool,
) -> Result<StoryEdit, ChartError> {
    let ids = checked(doc, ids)?;
    let clipboard = StoryClipboard::copy(doc, &ids)?;
    if count == 0
        || count
            .checked_mul(ids.len())
            .is_none_or(|n| n > crate::tools::MAX_OBJECTS)
    {
        return Err(invalid("Use 1–20,000 repeated Storyboard objects"));
    }
    let mut edit = StoryEdit::new(if move_source {
        "Move Storyboard selection"
    } else {
        "Clone Storyboard selection"
    });
    if move_source {
        for id in ids {
            edit.remove(id);
        }
    }
    for repetition in 1..=count {
        let step = Beat::new(
            i32::try_from(repetition).map_err(|_| invalid("Too many repetitions"))?,
            0,
            1,
        )?;
        let at = clipboard.first.checked_add(spacing.checked_mul(step)?)?;
        let pasted = clipboard.paste(doc, target_line.unwrap_or(clipboard.origin_line), at)?;
        for ((line, kind), change) in pasted.changes {
            for value in change.add {
                edit.add(line, kind, value);
            }
        }
    }
    Ok(edit)
}
