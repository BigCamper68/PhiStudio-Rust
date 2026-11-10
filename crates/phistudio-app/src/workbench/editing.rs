use super::*;

impl Workbench {
    pub fn notes(&mut self) -> Arc<Vec<(usize, Note)>> {
        self.view.notes(&self.editor, self.session, self.line)
    }
    pub(crate) fn note_candidates(&mut self, first: f64, last: f64) -> Vec<usize> {
        self.view
            .candidates(&self.editor, self.session, self.line, first, last)
    }

    pub fn execute(&mut self, command: Command) {
        let anchor = self.anchor();
        let line_remap = self.pending_line_remap(&command);
        let sound = crate::editor_sounds::command_event(&command);
        match self.editor.execute(command) {
            Ok(changed) => {
                self.error = None;
                if changed {
                    self.sounds.emit(sound);
                    if let Some(remap) = line_remap {
                        self.apply_line_remap(remap);
                    }
                    self.remember_anchor(anchor);
                    self.transport.playing = false;
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
        self.windows.scroll_to_properties |= id.is_some();
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
        self.request_editor_action(if redo {
            phistudio_sdk::EditorAction::Redo
        } else {
            phistudio_sdk::EditorAction::Undo
        });
    }

    pub(crate) fn history_step_now(&mut self, redo: bool) {
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
            self.sounds.emit(if redo {
                phistudio_core::EditorSoundEvent::Redo
            } else {
                phistudio_core::EditorSoundEvent::Undo
            });
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
            self.transport.playing = false;
        }
        self.select(None);
        if changed
            && self.options.history_jump
            && let Some(anchor) = anchor
        {
            if !structural {
                self.line = anchor.line.min(self.editor.document().line_count() - 1);
            }
            self.transport.beat = (anchor.beat - 1.).max(0.);
            self.timeline.camera_x = if anchor.x.abs() <= 675. { 0. } else { anchor.x };
        }
        self.unlink_invalid_xy();
        self.timeline.cancel_gesture();
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
        self.request_editor_action(phistudio_sdk::EditorAction::SetTool { tool: tool.into() });
    }

    pub(crate) fn set_tool_now(&mut self, tool: Tool) {
        self.stop_plugin_tool();
        if self.timeline.tool != tool {
            self.timeline.tool = tool;
            self.timeline.drag = None;
        }
    }

    pub fn delete_selected(&mut self) {
        self.request_editor_action(phistudio_sdk::EditorAction::DeleteSelection);
    }

    pub(crate) fn delete_selected_now(&mut self) {
        let revision = self.editor.revision();
        self.delete_selected_objects();
        if revision != self.editor.revision() {
            self.sounds.emit(phistudio_core::EditorSoundEvent::Delete);
        }
    }

    fn delete_selected_objects(&mut self) {
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
