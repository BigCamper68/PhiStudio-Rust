use super::*;

impl Editor {
    /// Apply an extension's edits as one validated, plugin-independent Undo entry.
    pub fn apply_patch(
        &mut self,
        expected_revision: u64,
        label: String,
        operations: Vec<phistudio_sdk::PatchOperation>,
    ) -> Result<bool, ChartError> {
        if expected_revision != self.revision {
            return Err(ChartError::Invalid(
                "Document changed; run the plugin action again".into(),
            ));
        }
        if label.trim().is_empty() || label.len() > 256 {
            return Err(ChartError::Invalid(
                "Transaction labels need 1–256 bytes".into(),
            ));
        }
        let Some(delta) = crate::patch::prepare(&self.document, operations)? else {
            return Ok(false);
        };
        delta.apply(&mut self.document, true);
        self.record(Delta::Patch(delta), label);
        Ok(true)
    }

    pub fn execute(&mut self, command: Command) -> Result<bool, ChartError> {
        let Some((delta, label)) = Delta::prepare(&self.document, command)? else {
            return Ok(false);
        };
        delta.apply(&mut self.document, true);
        self.record(delta, label);
        Ok(true)
    }

    /// Defaults affect newly authored lines; all other commands retain their
    /// explicit values, including duplication, imports and pasted objects.
    pub fn execute_with_line_defaults(
        &mut self,
        command: Command,
        defaults: &crate::LineCreationDefaults,
    ) -> Result<bool, ChartError> {
        if !matches!(command, Command::AddLine { .. } | Command::AddLines { .. }) {
            return self.execute(command);
        }
        let Some((delta, label)) =
            crate::line_management::prepare_with_defaults(&self.document, command, defaults)?
        else {
            return Ok(false);
        };
        let delta = Delta::Lines(delta);
        delta.apply(&mut self.document, true);
        self.record(delta, label);
        Ok(true)
    }

    /// A transaction exposes either all edits or none. Validation failure rolls
    /// back the prepared deltas without changing history, revision or dirty state.
    pub fn execute_batch(
        &mut self,
        label: &'static str,
        commands: Vec<Command>,
    ) -> Result<bool, ChartError> {
        if commands.len() > 40_000 {
            return Err(ChartError::Invalid(
                "Transaction exceeds 40,000 commands".into(),
            ));
        }
        let mut deltas: Vec<Delta> = Vec::new();
        for command in commands {
            match Delta::prepare(&self.document, command) {
                Ok(Some((delta, _))) => {
                    delta.apply(&mut self.document, true);
                    deltas.push(delta);
                }
                Ok(None) => {}
                Err(error) => {
                    for delta in deltas.iter().rev() {
                        delta.apply(&mut self.document, false);
                    }
                    return Err(error);
                }
            }
        }
        if deltas.is_empty() {
            return Ok(false);
        }
        self.record(Delta::Group(deltas), label);
        Ok(true)
    }

    pub(crate) fn execute_note_trace(
        &mut self,
        edit: crate::note_tracing::Edit,
    ) -> Result<crate::tools::Selection, ChartError> {
        if self.revision != edit.revision || self.document.line_count() != edit.line_count {
            return Err(ChartError::Invalid(
                "The chart changed. Preview tracing again before applying it.".into(),
            ));
        }
        let mut deltas = Vec::new();
        if let Some(defaults) = edit.new_line
            && let Some((delta, _)) = crate::line_management::prepare_with_defaults(
                &self.document,
                Command::AddLine {
                    name: "Note trace".into(),
                },
                &defaults,
            )?
        {
            let delta = Delta::Lines(delta);
            delta.apply(&mut self.document, true);
            deltas.push(delta);
        }
        let result = (|| {
            let plan = crate::tools::generated_event_edit(
                &self.document,
                &edit.remove,
                &edit.additions,
                "Note Tracing",
            )?;
            if let Some((delta, mut selected, _)) = plan.prepare_apply(&mut self.document, false)? {
                deltas.push(Delta::Tools(delta));
                selected.events.retain(|id| {
                    self.document
                        .event(*id)
                        .is_ok_and(|e| e.start_time >= edit.range[0] && e.end_time <= edit.range[1])
                });
                Ok(selected)
            } else {
                Ok(crate::tools::Selection::default())
            }
        })();
        let selected = match result {
            Ok(selected) => selected,
            Err(error) => {
                for delta in deltas.iter().rev() {
                    delta.apply(&mut self.document, false);
                }
                return Err(error);
            }
        };
        if let Some((before, after)) = edit.recipe_change {
            let delta = Delta::LineField {
                line: edit.line,
                key: crate::trajectories::RECIPE_FIELD,
                before: Some(before),
                after,
            };
            delta.apply(&mut self.document, true);
            deltas.push(delta);
        }
        if !deltas.is_empty() {
            self.record(Delta::Group(deltas), "Note Tracing");
        }
        Ok(selected)
    }

    pub(crate) fn execute_tool(
        &mut self,
        plan: crate::tools::ToolEdit,
        linked: bool,
    ) -> Result<Option<crate::tools::Selection>, ChartError> {
        let Some((delta, selection, label)) = plan.prepare_apply(&mut self.document, linked)?
        else {
            return Ok(None);
        };
        self.record(Delta::Tools(delta), label);
        Ok(Some(selection))
    }

    pub(crate) fn execute_story_tool(
        &mut self,
        plan: crate::story_tools::StoryEdit,
    ) -> Result<Option<Vec<crate::story_tools::StoryId>>, ChartError> {
        let Some((delta, selection, label)) = plan.prepare_apply(&mut self.document)? else {
            return Ok(None);
        };
        self.record(Delta::Advanced(delta), label);
        Ok(Some(selection))
    }

    pub(crate) fn execute_trajectory(
        &mut self,
        edit: crate::trajectories::Edit,
    ) -> Result<crate::tools::Selection, ChartError> {
        let key = crate::trajectories::RECIPE_FIELD;
        let line = self.document.root["judgeLineList"]
            .get(edit.line)
            .and_then(Value::as_object)
            .ok_or_else(|| ChartError::Invalid("Judge line no longer exists".into()))?;
        if line.get(key) != edit.before.as_ref() {
            return Err(ChartError::Invalid(
                "Trajectory changed; reopen it before applying".into(),
            ));
        }
        let mut deltas = Vec::new();
        let mut selection = crate::tools::Selection::default();
        if let Some(tool) = edit.tool
            && let Some((delta, selected, _)) = tool.prepare_apply(&mut self.document, false)?
        {
            selection = selected;
            deltas.push(Delta::Tools(delta));
        }
        if edit.before != edit.after {
            let delta = Delta::LineField {
                line: edit.line,
                key,
                before: edit.before,
                after: edit.after,
            };
            delta.apply(&mut self.document, true);
            deltas.push(delta);
        }
        if !deltas.is_empty() {
            self.record(Delta::Group(deltas), "Edit trajectory");
        }
        Ok(selection)
    }

    pub(crate) fn execute_objects(
        &mut self,
        plan: crate::objects::ObjectEdit,
    ) -> Result<Option<crate::objects::ObjectSelection>, ChartError> {
        let label = plan.label;
        let (deltas, selected) = self.apply_objects(plan)?;
        if deltas.is_empty() {
            return Ok(None);
        }
        self.record(Delta::Group(deltas), label);
        Ok(Some(selected))
    }

    pub(super) fn apply_objects(
        &mut self,
        plan: crate::objects::ObjectEdit,
    ) -> Result<(Vec<Delta>, crate::objects::ObjectSelection), ChartError> {
        let mut deltas: Vec<Delta> = vec![];
        let mut selected = crate::objects::ObjectSelection::default();
        if let Some(normal) = plan.normal
            && let Some((delta, selection, _)) = normal.prepare_apply(&mut self.document, false)?
        {
            selected.normal = selection;
            deltas.push(Delta::Tools(delta));
        }
        if let Some(story) = plan.story {
            match story.prepare_apply(&mut self.document) {
                Ok(Some((delta, selection, _))) => {
                    selected.story = selection;
                    deltas.push(Delta::Advanced(delta));
                }
                Ok(None) => {}
                Err(error) => {
                    for delta in deltas.iter().rev() {
                        delta.apply(&mut self.document, false);
                    }
                    return Err(error);
                }
            }
        }
        Ok((deltas, selected))
    }

    pub(crate) fn execute_text_composition(
        &mut self,
        mut lines: Vec<Value>,
        parented: bool,
    ) -> Result<crate::tools::Selection, ChartError> {
        let start = self.document.line_count();
        if lines.is_empty() || lines.len() > 1024 || start + lines.len() > 4096 {
            return Err(ChartError::Invalid(
                "Text target exceeds 4096 judge lines".into(),
            ));
        }
        if parented {
            for value in lines.iter_mut().skip(1) {
                value["father"] = json!(start);
            }
        }
        let mut selection = crate::tools::Selection::default();
        for (offset, value) in lines.iter().enumerate() {
            for (index, raw) in value["notes"].as_array().into_iter().flatten().enumerate() {
                let note: Note = serde_json::from_value(raw.clone())?;
                note.validate()?;
                selection.notes.push(NoteId {
                    line: start + offset,
                    index,
                });
            }
        }
        let deltas: Vec<_> = lines
            .into_iter()
            .enumerate()
            .map(|(i, value)| Delta::Line {
                index: start + i,
                value,
            })
            .collect();
        let delta = Delta::Group(deltas);
        delta.apply(&mut self.document, true);
        self.record(delta, "Compose text");
        Ok(selection)
    }

    /// New target lines and all pattern objects share one sparse Undo entry.
    pub fn insert_pattern(
        &mut self,
        clipboard: &crate::objects::ObjectClipboard,
        line: usize,
        layer: usize,
        beat: crate::Beat,
        mirrored: bool,
    ) -> Result<crate::objects::ObjectSelection, ChartError> {
        let end = line
            .checked_add(clipboard.required_lines())
            .filter(|n| *n <= 4096)
            .ok_or_else(|| ChartError::Invalid("Pattern target exceeds 4096 lines".into()))?;
        let count = self.document.line_count();
        if end.saturating_sub(count) > 256 {
            return Err(ChartError::Invalid(
                "Insert at most 256 new lines at once".into(),
            ));
        }
        let mut deltas = vec![];
        let result = (|| {
            for index in count..end {
                if let Some((delta, _)) = Delta::prepare(
                    &self.document,
                    Command::AddLine {
                        name: format!("Line {index}"),
                    },
                )? {
                    delta.apply(&mut self.document, true);
                    deltas.push(delta);
                }
            }
            let plan = clipboard.paste(&self.document, line, layer, beat, mirrored)?;
            let (edits, selected) = self.apply_objects(plan)?;
            deltas.extend(edits);
            Ok(selected)
        })();
        match result {
            Ok(selected) => {
                if !deltas.is_empty() {
                    self.record(Delta::Group(deltas), "Insert pattern");
                }
                Ok(selected)
            }
            Err(error) => {
                for delta in deltas.iter().rev() {
                    delta.apply(&mut self.document, false);
                }
                Err(error)
            }
        }
    }

    pub fn erase_objects(
        &mut self,
        selection: &crate::tools::Selection,
        stories: Vec<(usize, crate::StoryKind, usize)>,
    ) -> Result<bool, ChartError> {
        let plan = crate::tools::delete(&self.document, selection)?;
        if stories.is_empty() {
            return plan.apply(self).map(|result| result.is_some());
        }
        let tools = plan
            .prepare_apply(&mut self.document, false)?
            .map(|(delta, _, _)| delta);
        let story = match crate::advanced::erase_story(&self.document, stories) {
            Ok(story) => story,
            Err(error) => {
                if let Some(delta) = &tools {
                    delta.apply(&mut self.document, false);
                }
                return Err(error);
            }
        };
        let mut deltas = vec![];
        if let Some(delta) = tools {
            deltas.push(Delta::Tools(delta));
        }
        if let Some(delta) = story {
            delta.apply(&mut self.document, true);
            deltas.push(Delta::Advanced(delta));
        }
        if deltas.is_empty() {
            return Ok(false);
        }
        self.record(Delta::Group(deltas), "Erase objects");
        Ok(true)
    }
}
