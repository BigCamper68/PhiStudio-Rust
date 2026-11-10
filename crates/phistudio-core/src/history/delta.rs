use super::*;

impl Delta {
    /// Resolve and validate every fallible part before touching the document.
    pub(super) fn prepare(
        doc: &ChartDocument,
        command: Command,
    ) -> Result<Option<(Self, &'static str)>, ChartError> {
        let line_object = |index: usize| {
            doc.root["judgeLineList"]
                .get(index)
                .and_then(Value::as_object)
                .ok_or_else(|| ChartError::Invalid("Judge line no longer exists".into()))
        };
        let note_delta =
            |id: NoteId, before: Option<Value>, after: Option<Value>| -> Result<Self, ChartError> {
                let line = line_object(id.line)?;
                let count = doc.note_values(id.line).map_or(0, Vec::len)
                    + usize::from(after.is_some())
                    - usize::from(before.is_some());
                let notes_shape = match line.get("notes") {
                    None => NotesShape::Missing,
                    Some(Value::Null) => NotesShape::Null,
                    _ => NotesShape::Array,
                };
                Ok(Self::Note {
                    id,
                    before,
                    after,
                    notes_shape,
                    count_before: line.get("numOfNotes").cloned(),
                    count_after: json!(count),
                })
            };
        Ok(Some(match command {
            Command::EditNoise(action) => {
                return crate::noise_fields::prepare(doc, action)
                    .map(|v| v.map(|(delta, label)| (Self::Noise(delta), label)));
            }
            command @ (Command::AddLine { .. }
            | Command::AddLines { .. }
            | Command::DuplicateLine(_)
            | Command::MoveLine { .. }
            | Command::DeleteLine(_)) => {
                return crate::line_management::prepare(doc, command)
                    .map(|v| v.map(|(d, label)| (Self::Lines(d), label)));
            }
            Command::EditShader { baseline, action } => {
                return crate::shaders::prepare(doc, baseline, action)
                    .map(|v| v.map(|(d, label)| (Self::Shader(d), label)));
            }
            Command::SetShaderSource { path, source } => {
                return crate::shaders::prepare_source(doc, path, source)
                    .map(|v| v.map(|d| (Self::Shader(d), "Edit shader source")));
            }
            Command::EraseStoryboard(ids) => {
                return crate::advanced::erase_story(doc, ids)
                    .map(|d| d.map(|d| (Self::Advanced(d), "Erase storyboard events")));
            }
            Command::EditMetadata(meta) => {
                return crate::advanced::metadata(doc, meta)
                    .map(|d| d.map(|d| (Self::Advanced(d), "Edit chart properties")));
            }
            Command::EditStoryboard {
                line,
                kind,
                index,
                event,
            } => {
                return crate::advanced::story(doc, line, kind, index, event)
                    .map(|d| d.map(|d| (Self::Advanced(d), "Edit storyboard event")));
            }
            Command::SetNoteHitsound { ids, reference } => {
                return crate::note_extras::sound_delta(doc, ids, reference)
                    .map(|d| d.map(|d| (Self::Extras(d), "Edit note hitsound")));
            }
            Command::SetNoteVolume { id, gain } => {
                return crate::note_extras::volume_delta(doc, id, gain)
                    .map(|d| d.map(|d| (Self::Extras(d), "Edit note sound volume")));
            }
            Command::EditSpeedCurve(edit) => {
                return crate::speed_curve::prepare(doc, edit)
                    .map(|d| d.map(|d| (Self::SpeedCurve(d), "Edit note speed curve")));
            }
            Command::SetControlCurve { line, kind, points } => {
                return crate::note_extras::control_delta(doc, line, kind, points)
                    .map(|d| d.map(|d| (Self::Extras(d), "Edit note control curve")));
            }
            Command::EditGroups(action) => {
                return crate::groups::prepare(doc, action)
                    .map(|d| d.map(|(d, label)| (Self::Groups(d), label)));
            }
            command @ (Command::InsertEvent { .. }
            | Command::AddEvent { .. }
            | Command::UpdateEvent { .. }
            | Command::DeleteEvent(_)
            | Command::AddBpm(_)
            | Command::UpdateBpm { .. }
            | Command::DeleteBpm(_)
            | Command::EditLine { .. }) => {
                return crate::authoring::prepare(doc, command)
                    .map(|v| v.map(|(d, label)| (Self::Authoring(d), label)));
            }
            Command::InsertNote { line, value } => {
                line_object(line)?;
                if !value.is_object() {
                    return Err(ChartError::Invalid("Note must be an object".into()));
                }
                serde_json::from_value::<Note>(value.clone())?.validate()?;
                let id = NoteId {
                    line,
                    index: doc.note_values(line).map_or(0, Vec::len),
                };
                (note_delta(id, None, Some(value))?, "Paste note")
            }
            Command::AddNote { line, note } => {
                line_object(line)?;
                let id = NoteId {
                    line,
                    index: doc.note_values(line).map_or(0, Vec::len),
                };
                (
                    note_delta(id, None, Some(note.merge_into(None)?))?,
                    "Add note",
                )
            }
            Command::UpdateNote { id, note } => {
                let previous = doc.note(id.line, id.index)?;
                note.validate()?;
                if previous == note {
                    return Ok(None);
                }
                let before = doc.note_values(id.line).unwrap()[id.index].clone();
                let after = note.merge_into(Some(&before))?;
                (note_delta(id, Some(before), Some(after))?, "Edit note")
            }
            Command::DeleteNote(id) => {
                doc.note(id.line, id.index)?;
                let before = doc.note_values(id.line).unwrap()[id.index].clone();
                (note_delta(id, Some(before), None)?, "Delete note")
            }
            Command::RenameLine { line, name } => {
                let before = line_object(line)?.get("Name").cloned();
                let after = Value::String(name);
                if before.as_ref() == Some(&after) {
                    return Ok(None);
                }
                (
                    Self::LineName {
                        index: line,
                        before,
                        after,
                    },
                    "Rename line",
                )
            }
            Command::RenameChart(name) => {
                let before = doc.root["META"].get("name").cloned();
                let after = Value::String(name);
                if before.as_ref() == Some(&after) {
                    return Ok(None);
                }
                (Self::Title { before, after }, "Rename chart")
            }
        }))
    }

    pub(super) fn apply(&self, doc: &mut ChartDocument, forward: bool) {
        fn put(object: &mut serde_json::Map<String, Value>, key: &str, value: Option<Value>) {
            if let Some(value) = value {
                object.insert(key.into(), value);
            } else {
                object.shift_remove(key);
            }
        }
        match self {
            Self::Patch(delta) => delta.apply(doc, forward),
            Self::Lines(delta) => delta.apply(doc, forward),
            Self::LineField {
                line,
                key,
                before,
                after,
            } => {
                let target = doc.root["judgeLineList"][*line].as_object_mut().unwrap();
                if let Some(value) = if forward { after } else { before } {
                    target.insert((*key).into(), value.clone());
                } else {
                    target.shift_remove(*key);
                }
            }
            Self::Shader(delta) => delta.apply(doc, forward),
            Self::Noise(delta) => delta.apply(doc, forward),
            Self::Advanced(delta) => delta.apply(doc, forward),
            Self::Extras(delta) => delta.apply(doc, forward),
            Self::SpeedCurve(delta) => delta.apply(doc, forward),
            Self::Groups(delta) => delta.apply(doc, forward),
            Self::Tools(delta) => delta.apply(doc, forward),
            Self::Group(deltas) => {
                if forward {
                    for delta in deltas {
                        delta.apply(doc, true);
                    }
                } else {
                    for delta in deltas.iter().rev() {
                        delta.apply(doc, false);
                    }
                }
            }
            Self::Authoring(delta) => delta.apply(doc, forward),
            Self::Note {
                id,
                before,
                after,
                notes_shape,
                count_before,
                count_after,
            } => {
                let line = doc.root["judgeLineList"][id.line].as_object_mut().unwrap();
                if forward {
                    if !line.get("notes").is_some_and(Value::is_array) {
                        line.insert("notes".into(), json!([]));
                    }
                    let notes = line.get_mut("notes").unwrap().as_array_mut().unwrap();
                    match (before, after) {
                        (None, Some(value)) => notes.insert(id.index, value.clone()),
                        (Some(_), Some(value)) => notes[id.index] = value.clone(),
                        (Some(_), None) => {
                            notes.remove(id.index);
                        }
                        _ => unreachable!(),
                    }
                    put(line, "numOfNotes", Some(count_after.clone()));
                } else {
                    // A missing/null notes container is restored exactly, as is
                    // an imported numOfNotes which may not match the array.
                    if matches!(notes_shape, NotesShape::Array) {
                        let notes = line.get_mut("notes").unwrap().as_array_mut().unwrap();
                        match (before, after) {
                            (None, Some(_)) => {
                                notes.remove(id.index);
                            }
                            (Some(value), Some(_)) => notes[id.index] = value.clone(),
                            (Some(value), None) => notes.insert(id.index, value.clone()),
                            _ => unreachable!(),
                        }
                    } else {
                        put(
                            line,
                            "notes",
                            if matches!(notes_shape, NotesShape::Null) {
                                Some(Value::Null)
                            } else {
                                None
                            },
                        );
                    }
                    put(line, "numOfNotes", count_before.clone());
                }
            }
            Self::Line { index, value } => {
                let lines = doc.root["judgeLineList"].as_array_mut().unwrap();
                if forward {
                    lines.insert(*index, value.clone());
                } else {
                    lines.remove(*index);
                }
            }
            Self::LineName {
                index,
                before,
                after,
            } => put(
                doc.root["judgeLineList"][*index].as_object_mut().unwrap(),
                "Name",
                if forward {
                    Some(after.clone())
                } else {
                    before.clone()
                },
            ),
            Self::Title { before, after } => put(
                doc.root["META"].as_object_mut().unwrap(),
                "name",
                if forward {
                    Some(after.clone())
                } else {
                    before.clone()
                },
            ),
        }
    }
}
