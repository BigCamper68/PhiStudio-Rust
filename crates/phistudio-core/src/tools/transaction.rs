use super::*;

impl ToolDelta {
    pub fn apply(&self, doc: &mut ChartDocument, forward: bool) {
        if forward {
            for edit in &self.arrays {
                edit.apply(&mut doc.root, true);
            }
        } else {
            for edit in self.arrays.iter().rev() {
                edit.apply(&mut doc.root, false);
            }
        }
        for (line, before, after) in &self.counts {
            let object = doc.root["judgeLineList"][*line].as_object_mut().unwrap();
            if let Some(value) = if forward {
                Some(after)
            } else {
                before.as_ref()
            } {
                object.insert("numOfNotes".into(), value.clone());
            } else {
                object.shift_remove("numOfNotes");
            }
        }
    }
}

impl ToolEdit {
    pub fn with_note_defaults(
        mut self,
        defaults: &crate::NoteCreationDefaults,
        sounds: &crate::NoteCreationSounds,
        policy: crate::NoteCreationPolicy,
        canceled: impl Fn() -> bool,
    ) -> Result<Self, ChartError> {
        for (track, edit) in &mut self.tracks {
            if matches!(track, Track::Notes(_)) {
                if !edit.remove.is_empty() {
                    return Err(invalid(
                        "Creation defaults apply only to newly authored notes",
                    ));
                }
                for value in &mut edit.add {
                    if canceled() {
                        return Err(invalid("Note creation canceled"));
                    }
                    *value = defaults.decorate(value, sounds, policy)?;
                }
            }
        }
        Ok(self)
    }
    pub fn added_note_count(&self) -> usize {
        self.tracks
            .iter()
            .filter_map(|(track, edit)| matches!(track, Track::Notes(_)).then_some(edit.add.len()))
            .sum()
    }
    pub fn validate_preview(&self, doc: &ChartDocument) -> Result<(), ChartError> {
        for (track, edit) in &self.tracks {
            if let Track::Events(line, layer, kind) = track {
                validate_track(doc, *line, *layer, *kind, edit)?;
            }
        }
        Ok(())
    }

    pub fn apply(self, editor: &mut Editor) -> Result<Option<Selection>, ChartError> {
        editor.execute_tool(self, false)
    }
    pub fn apply_linked(self, editor: &mut Editor) -> Result<Option<Selection>, ChartError> {
        editor.execute_tool(self, true)
    }
    pub(crate) fn prepare_apply(
        self,
        doc: &mut ChartDocument,
        linked: bool,
    ) -> Result<Option<(ToolDelta, Selection, &'static str)>, ChartError> {
        if self.tracks.is_empty() {
            return Ok(None);
        }
        let mut delta = ToolDelta {
            arrays: Vec::new(),
            counts: Vec::new(),
        };
        let mut selection = Selection::default();
        let mut xy_layers = BTreeSet::new();
        let result = (|| {
            for (track, edit) in self.tracks {
                if let Track::Events(line, layer, kind) = track {
                    if matches!(kind, EventType::MoveX | EventType::MoveY) {
                        xy_layers.insert((line, layer));
                    }
                    if layer > 3 {
                        return Err(invalid("Only event layers 0–3 are editable"));
                    }
                    validate_track(doc, line, layer, kind, &edit)?;
                }
                let sort = matches!(track, Track::Events(..)) && !edit.add.is_empty();
                let after = crate::array_edit::ArrayBatch::prepare(
                    &doc.root,
                    track.path(),
                    &edit.remove,
                    edit.add,
                    sort,
                    matches!(track, Track::Events(..)),
                )?;
                match track {
                    Track::Notes(line) => {
                        let before = doc.root["judgeLineList"][line].get("numOfNotes").cloned();
                        let count = doc.note_values(line).map_or(0, Vec::len) - edit.remove.len()
                            + after.added_indices.len();
                        delta.counts.push((line, before, serde_json::json!(count)));
                        selection
                            .notes
                            .extend(after.added_indices.iter().map(|index| NoteId {
                                line,
                                index: *index,
                            }));
                    }
                    Track::Events(line, layer, kind) => {
                        selection
                            .events
                            .extend(after.added_indices.iter().map(|index| EventId {
                                line,
                                layer,
                                kind,
                                index: *index,
                            }))
                    }
                }
                after.apply(&mut doc.root, true);
                delta.arrays.push(after);
            }
            if linked {
                for (line, layer) in xy_layers {
                    doc.validate_xy_layer(line, layer)?;
                }
            }
            Ok::<_, ChartError>(())
        })();
        if let Err(error) = result {
            for edit in delta.arrays.iter().rev() {
                edit.apply(&mut doc.root, false);
            }
            return Err(error);
        }
        for (line, _, after) in &delta.counts {
            doc.root["judgeLineList"][*line]["numOfNotes"] = after.clone();
        }
        Ok(Some((delta, selection.normalized(), self.label)))
    }
}

pub(super) fn validate_track(
    doc: &ChartDocument,
    line: usize,
    layer: usize,
    kind: EventType,
    edit: &TrackEdit,
) -> Result<(), ChartError> {
    if edit.add.is_empty() {
        return Ok(());
    }
    let removed: BTreeSet<_> = edit.remove.iter().copied().collect();
    let mut intervals = Vec::new();
    for (i, value) in doc.event_values(line, layer, kind)?.iter().enumerate() {
        if !removed.contains(&i) {
            let event = LineEvent::read(kind, value)?;
            intervals.push((event.start_time, event.end_time, false));
        }
    }
    for value in &edit.add {
        let event = LineEvent::read(kind, value)?;
        event.validate(kind)?;
        intervals.push((event.start_time, event.end_time, true));
    }
    intervals.sort_by_key(|(start, _, _)| *start);
    let mut end = Beat::ZERO;
    let mut new_end = Beat::ZERO;
    for (start, last, new) in intervals {
        if (new && start < end) || (!new && start < new_end) {
            return Err(invalid("Event overlaps another event in this track"));
        }
        end = end.max(last);
        if new {
            new_end = new_end.max(last);
        }
    }
    Ok(())
}

pub(super) fn plan(
    doc: &ChartDocument,
    remove: &Selection,
    items: Vec<Object>,
    label: &'static str,
) -> Result<ToolEdit, ChartError> {
    let remove = remove.checked(doc)?;
    if items.len() > MAX_OBJECTS {
        return Err(invalid(
            "The operation would create more than 20,000 objects",
        ));
    }
    if remove.events.iter().any(|id| id.layer > 3) {
        return Err(invalid("Only event layers 0–3 are editable"));
    }
    let mut tracks: BTreeMap<Track, TrackEdit> = BTreeMap::new();
    for id in remove.notes {
        tracks
            .entry(Track::Notes(id.line))
            .or_insert_with(|| TrackEdit {
                remove: Vec::new(),
                add: Vec::new(),
            })
            .remove
            .push(id.index);
    }
    for id in remove.events {
        tracks
            .entry(Track::Events(id.line, id.layer, id.kind))
            .or_insert_with(|| TrackEdit {
                remove: Vec::new(),
                add: Vec::new(),
            })
            .remove
            .push(id.index);
    }
    for item in items {
        if item.line >= doc.line_count() {
            return Err(invalid("Destination judge line does not exist"));
        }
        let track = if let Some((layer, kind)) = item.event {
            if layer > 3 {
                return Err(invalid("Only event layers 0–3 are editable"));
            }
            LineEvent::read(kind, &item.value)?.validate(kind)?;
            Track::Events(item.line, layer, kind)
        } else {
            serde_json::from_value::<Note>(item.value.clone())?.validate()?;
            Track::Notes(item.line)
        };
        tracks
            .entry(track)
            .or_insert_with(|| TrackEdit {
                remove: Vec::new(),
                add: Vec::new(),
            })
            .add
            .push(item.value);
    }
    Ok(ToolEdit { label, tracks })
}

pub(super) fn replacement(
    doc: &ChartDocument,
    selection: &Selection,
    items: Vec<Object>,
    label: &'static str,
) -> Result<ToolEdit, ChartError> {
    let old = objects(doc, selection)?;
    if old.len() == items.len()
        && old
            .iter()
            .zip(&items)
            .all(|(a, b)| a.line == b.line && a.event == b.event && a.value == b.value)
    {
        return plan(doc, &Selection::default(), vec![], label);
    }
    plan(doc, selection, items, label)
}

pub fn delete(doc: &ChartDocument, selection: &Selection) -> Result<ToolEdit, ChartError> {
    plan(doc, selection, vec![], "Delete selection")
}
