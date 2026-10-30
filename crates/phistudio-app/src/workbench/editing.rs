use super::*;

impl Workbench {
    pub fn notes(&mut self) -> Arc<Vec<(usize, Note)>> {
        let revision = self.editor.revision();
        if self
            .note_cache
            .get(&self.line)
            .is_none_or(|cache| cache.revision != revision)
        {
            let notes = self.editor.document().notes(self.line);
            self.note_cache
                .retain(|_, cache| cache.revision == revision);
            self.note_cache.insert(
                self.line,
                NoteCache {
                    revision,
                    index: crate::note_index::NoteIndex::new(&notes),
                    notes: Arc::new(notes),
                },
            );
        }
        Arc::clone(&self.note_cache.get(&self.line).unwrap().notes)
    }

    pub(crate) fn note_candidates(&mut self, first: f64, last: f64) -> Vec<usize> {
        self.notes();
        self.note_cache
            .get(&self.line)
            .unwrap()
            .index
            .query(first, last)
    }

    pub fn execute(&mut self, command: Command) {
        let anchor = self.anchor();
        let line_remap = self.pending_line_remap(&command);
        match self.editor.execute(command) {
            Ok(changed) => {
                self.error = None;
                if changed {
                    if let Some(remap) = line_remap {
                        self.apply_line_remap(remap);
                    }
                    self.remember_anchor(anchor);
                    self.playing = false;
                }
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn select(&mut self, id: Option<NoteId>) {
        if let Some(id) = id {
            self.line = id.line;
        }
        self.advanced.story_selection.clear();
        self.properties_scroll |= id.is_some();
        self.advanced.story_selected = None;
        self.advanced.story = None;
        self.tools.selection = phistudio_core::tools::Selection {
            notes: id.into_iter().collect(),
            events: Vec::new(),
        };
        self.selected = id;
        self.draft = None;
        self.clear_event_selection();
    }

    pub(crate) fn history_step(&mut self, redo: bool) {
        let anchor = self
            .editor
            .action_id(redo)
            .and_then(|id| self.advanced.anchors.get(&id))
            .copied();
        let multi_action = self.editor.action_id(redo);
        let structural =
            multi_action.is_some_and(|id| self.multi_line.line_history.contains_key(&id));
        let changed = if redo {
            self.editor.redo()
        } else {
            self.editor.undo()
        };
        if changed {
            if let Some(change) = multi_action.and_then(|id| self.multi_line.line_history.get(&id))
            {
                self.multi_line.lines = if redo {
                    change.after.clone()
                } else {
                    change.before.clone()
                };
                self.line = if redo {
                    change.after_line
                } else {
                    change.before_line
                };
            }
            self.playing = false;
        }
        self.select(None);
        if changed
            && self.options.history_jump
            && let Some(anchor) = anchor
        {
            if !structural {
                self.line = anchor.line.min(self.editor.document().line_count() - 1);
            }
            self.beat = (anchor.beat - 1.).max(0.);
            self.camera_x = if anchor.x.abs() <= 675. { 0. } else { anchor.x };
        }
        self.unlink_invalid_xy();
        self.drag = None;
        self.grid_input.cancel();
    }

    pub(crate) fn unlink_invalid_xy(&mut self) {
        if self.options.xy_link
            && self
                .editor
                .document()
                .validate_xy_layer(self.line, self.events.layer)
                .is_err()
        {
            self.options.xy_link = false;
        }
    }

    pub(crate) fn set_tool(&mut self, tool: Tool) {
        if self.tool != tool {
            self.tool = tool;
            self.drag = None;
        }
    }

    pub fn delete_selected(&mut self) {
        if !self.story_selection().is_empty() {
            let edit = phistudio_core::objects::delete(
                self.editor.document(),
                &self.selection(),
                &self.story_selection(),
            );
            self.apply_object_tool(edit);
            return;
        }
        if let Some(id) = self.advanced.story_selected {
            self.apply_story(Some(id), id.kind, None);
            return;
        }

        let selection = self.selection();
        if !selection.is_empty() {
            let edit = self
                .link_selection(&selection)
                .and_then(|s| phistudio_core::tools::delete(self.editor.document(), &s));
            self.apply_tool(edit);
            return;
        }
        if let Some(id) = self.events.selected {
            self.commit_authoring(Command::DeleteEvent(id));
            return;
        }
        if let Some(id) = self.selected {
            self.execute(Command::DeleteNote(id));
            self.select(None);
        }
    }
}
