//! X/Y time binding uses unique equal intervals; values/easings stay independent.
use crate::tools::Selection;
use crate::{Beat, ChartDocument, ChartError, EventId, EventType};
use std::collections::{BTreeMap, BTreeSet};
impl ChartDocument {
    pub fn xy_pair(&self, id: EventId) -> Result<EventId, ChartError> {
        let kind = match id.kind {
            EventType::MoveX => EventType::MoveY,
            EventType::MoveY => EventType::MoveX,
            _ => {
                return Err(ChartError::Invalid(
                    "Choose a Move X or Move Y event".into(),
                ));
            }
        };
        let event = self.event(id)?;
        let pairs: Vec<_> = self
            .events(id.line, id.layer, kind)?
            .into_iter()
            .filter(|(_, e)| e.start_time == event.start_time && e.end_time == event.end_time)
            .collect();
        if pairs.len() != 1 {
            return Err(ChartError::Invalid(
                "X/Y timing needs one matching interval per axis; synchronize this layer first"
                    .into(),
            ));
        }
        Ok(EventId {
            kind,
            index: pairs[0].0,
            ..id
        })
    }
    pub fn validate_xy_layer(&self, line: usize, layer: usize) -> Result<(), ChartError> {
        let mut sets = Vec::new();
        for kind in [EventType::MoveX, EventType::MoveY] {
            let mut intervals: Vec<_> = self
                .events(line, layer, kind)?
                .into_iter()
                .map(|(_, e)| (e.start_time, e.end_time))
                .collect();
            intervals.sort();
            if intervals.iter().any(|(a, b)| *a < Beat::ZERO || b <= a)
                || intervals.windows(2).any(|w| w[0].1 > w[1].0)
            {
                return Err(ChartError::Invalid(
                    "X/Y events need ordered nonoverlapping positive intervals".into(),
                ));
            }
            sets.push(intervals);
        }
        if sets[0] != sets[1] {
            return Err(ChartError::Invalid(
                "X/Y intervals do not match; synchronize this layer before enabling the link"
                    .into(),
            ));
        }
        Ok(())
    }
}
pub fn linked_selection(
    doc: &ChartDocument,
    selection: &Selection,
) -> Result<Selection, ChartError> {
    let mut selection = selection.checked(doc)?;
    type Intervals = BTreeMap<(Beat, Beat), Vec<EventId>>;
    let mut indices: BTreeMap<(usize, usize, EventType), Intervals> = BTreeMap::new();
    let mut add = Vec::new();
    for id in &selection.events {
        let kind = match id.kind {
            EventType::MoveX => EventType::MoveY,
            EventType::MoveY => EventType::MoveX,
            _ => continue,
        };
        let key = (id.line, id.layer, kind);
        if let std::collections::btree_map::Entry::Vacant(entry) = indices.entry(key) {
            let mut intervals: Intervals = BTreeMap::new();
            for (index, e) in doc.events(id.line, id.layer, kind)? {
                intervals
                    .entry((e.start_time, e.end_time))
                    .or_default()
                    .push(EventId { kind, index, ..*id });
            }
            entry.insert(intervals);
        }
        let event = doc.event(*id)?;
        let pairs = indices[&key]
            .get(&(event.start_time, event.end_time))
            .filter(|v| v.len() == 1)
            .ok_or_else(|| {
                ChartError::Invalid(
                    "X/Y timing needs one matching interval per axis; synchronize this layer first"
                        .into(),
                )
            })?;
        add.push(pairs[0]);
    }
    selection.events.extend(add);
    selection.checked(doc)
}
type XyTracks = BTreeMap<EventType, Vec<(EventId, crate::LineEvent)>>;
pub(crate) fn xy_intervals(
    doc: &ChartDocument,
    line: usize,
    layer: usize,
) -> Result<(BTreeSet<Beat>, XyTracks), ChartError> {
    if layer > 3 {
        return Err(ChartError::Invalid(
            "Only event layers 0–3 are editable".into(),
        ));
    }
    let mut cuts = BTreeSet::new();
    let mut tracks = BTreeMap::new();
    for kind in [EventType::MoveX, EventType::MoveY] {
        let mut events = doc.events(line, layer, kind)?;
        events.sort_by_key(|(_, e)| e.start_time);
        for (_, event) in &events {
            event.validate(kind)?;
            if event.end_time <= event.start_time {
                return Err(ChartError::Invalid(
                    "Resolve zero-length X/Y events before synchronizing".into(),
                ));
            }
            cuts.insert(event.start_time);
            cuts.insert(event.end_time);
        }
        if events
            .windows(2)
            .any(|w| w[0].1.end_time > w[1].1.start_time)
        {
            return Err(ChartError::Invalid(
                "Resolve overlaps before synchronizing X/Y".into(),
            ));
        }
        tracks.insert(
            kind,
            events
                .into_iter()
                .map(|(index, e)| {
                    (
                        EventId {
                            line,
                            layer,
                            kind,
                            index,
                        },
                        e,
                    )
                })
                .collect(),
        );
    }
    Ok((cuts, tracks))
}
