use super::*;

impl Workbench {
    pub(crate) fn selection(&self) -> Selection {
        let mut selection = self.tools.selection.clone();
        if selection.is_empty() {
            selection.notes.extend(self.selected);
            selection.events.extend(self.events.selected);
        }
        selection.normalized()
    }

    pub(crate) fn link_selection(&self, s: &Selection) -> Result<Selection, ChartError> {
        if self.options.xy_link {
            phistudio_core::linked_selection(self.editor.document(), s)
        } else {
            Ok(s.clone())
        }
    }

    pub(crate) fn timing_selection(&self) -> Result<Selection, ChartError> {
        self.link_selection(&self.selection())
    }

    pub(crate) fn set_selection(&mut self, selection: Selection) {
        self.advanced.story_selection.clear();
        self.advanced.story_selected = None;
        self.advanced.story = None;
        let selection = selection.normalized();
        self.windows.scroll_to_properties |= !selection.is_empty();
        self.selected = selection.notes.first().copied();
        self.events.selected = if self.selected.is_none() {
            selection.events.first().copied()
        } else {
            None
        };
        self.draft = None;
        self.clear_event_draft();
        if self.options.auto_copy_selection
            && selection.len() > 1
            && let Ok(clipboard) = phistudio_core::objects::ObjectClipboard::copy(
                self.editor.document(),
                &selection,
                &[],
            )
        {
            self.tools.clipboard = Some(Arc::new(clipboard));
        }
        self.tools.selection = selection;
    }

    pub(crate) fn toggle_note(&mut self, id: Option<NoteId>, additive: bool) {
        self.tools.scope = Scope::Notes;
        if !additive {
            self.select(id);
            return;
        }
        if let Some(id) = id {
            let mut selection = self.selection();
            if let Some(at) = selection.notes.iter().position(|n| *n == id) {
                selection.notes.remove(at);
            } else {
                selection.notes.push(id);
            }
            self.set_object_selection(selection, self.story_selection());
        }
    }

    pub(crate) fn toggle_event(&mut self, id: Option<EventId>, additive: bool) {
        self.tools.scope = Scope::Events;
        if !additive {
            self.select_event(id);
            return;
        }
        if let Some(id) = id {
            let mut selection = self.selection();
            if let Some(at) = selection.events.iter().position(|e| *e == id) {
                selection.events.remove(at);
            } else {
                selection.events.push(id);
            }
            self.set_object_selection(selection, self.story_selection());
        }
    }

    pub(crate) fn select_scope(&mut self, scope: Scope, invert: bool) {
        self.request_editor_action(phistudio_sdk::EditorAction::SelectScope {
            scope: scope.into(),
            invert,
        });
    }

    pub(crate) fn select_scope_now(&mut self, scope: Scope, invert: bool) {
        if crate::story_grid::active(self) && scope != Scope::Notes {
            let result = self.visible_story_ids().and_then(|mut stories| {
                let mut normal = Selection::default();
                if scope == Scope::Both {
                    for line in self.target_lines(false) {
                        normal.notes.extend(
                            Selection::all(
                                self.editor.document(),
                                line,
                                self.events.layer,
                                true,
                                false,
                            )?
                            .notes,
                        );
                    }
                }
                if invert {
                    let old = self.selection();
                    normal.notes.retain(|id| !old.notes.contains(id));
                    let old = self.story_selection();
                    stories.retain(|id| !old.contains(id));
                }
                Ok((normal, stories))
            });
            match result {
                Ok((normal, stories)) => {
                    self.set_object_selection(normal, stories);
                    self.tools.scope = scope;
                    self.error = None;
                }
                Err(error) => self.error = Some(error.to_string()),
            }
            return;
        }
        let result = (|| {
            let mut selected = Selection::default();
            if scope != Scope::Events {
                for line in self.target_lines(false) {
                    selected.notes.extend(
                        Selection::all(
                            self.editor.document(),
                            line,
                            self.events.layer,
                            true,
                            false,
                        )?
                        .notes,
                    );
                }
            }
            if scope != Scope::Notes && !crate::story_grid::active(self) {
                for line in self.target_lines(true) {
                    selected.events.extend(
                        Selection::all(
                            self.editor.document(),
                            line,
                            self.events.layer,
                            false,
                            true,
                        )?
                        .events,
                    );
                }
            }
            selected.checked(self.editor.document())
        })();
        match result {
            Ok(mut selected) => {
                if invert {
                    let old = self.selection();
                    selected.notes.retain(|n| !old.notes.contains(n));
                    selected.events.retain(|e| !old.events.contains(e));
                }
                self.tools.scope = scope;
                self.set_selection(selected);
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub(super) fn select_range(&mut self) {
        let result = (|| {
            let start = self.tools.start.parse::<Beat>()?;
            let end = self.tools.end.parse::<Beat>()?;
            if start > end || self.tools.x_range[0] > self.tools.x_range[1] {
                return Err(ChartError::Invalid(
                    "Selection range must be ordered".into(),
                ));
            }
            let scope = self.tools.scope;
            let mut selected = Selection::default();
            if scope != Scope::Events {
                for line in self.target_lines(false) {
                    selected.notes.extend(
                        self.editor
                            .document()
                            .notes(line)
                            .into_iter()
                            .map(|(index, _)| NoteId { line, index }),
                    );
                }
            }
            if scope != Scope::Notes {
                for line in self.target_lines(true) {
                    for kind in EventType::ALL {
                        selected.events.extend(
                            self.editor
                                .document()
                                .events(line, self.events.layer, kind)?
                                .into_iter()
                                .map(|(index, _)| EventId {
                                    line,
                                    layer: self.events.layer,
                                    kind,
                                    index,
                                }),
                        );
                    }
                }
            }
            selected.notes.retain(|id| {
                let n = self.editor.document().note(id.line, id.index).unwrap();
                n.start_time >= start
                    && n.start_time <= end
                    && (self.tools.x_range[0]..=self.tools.x_range[1]).contains(&n.x)
                    && self.tools.filter_kind.is_none_or(|v| v == n.kind)
                    && self.tools.filter_side.is_none_or(|v| v == n.above)
                    && self.tools.filter_fake.is_none_or(|v| v == (n.is_fake != 0))
            });
            selected.events.retain(|id| {
                let e = self.editor.document().event(*id).unwrap();
                e.start_time <= end
                    && e.end_time >= start
                    && self.tools.filter_track.is_none_or(|v| v == id.kind)
            });
            if self.tools.additive {
                let old = self.selection();
                selected.notes.extend(old.notes);
                selected.events.extend(old.events);
            }
            selected.checked(self.editor.document())
        })();
        match result {
            Ok(s) => {
                self.set_selection(s);
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub(super) fn select_xy_pairs(&mut self) {
        let result = (|| {
            let mut selected = self.selection();
            let mut marks = std::collections::BTreeSet::new();
            let mut groups = std::collections::BTreeSet::new();
            for id in &selected.events {
                if matches!(id.kind, EventType::MoveX | EventType::MoveY) {
                    let event = self.editor.document().event(*id)?;
                    marks.insert((id.line, id.layer, event.start_time, event.end_time));
                    groups.insert((id.line, id.layer));
                }
            }
            if marks.is_empty() {
                return Err(ChartError::Invalid(
                    "Select Move X or Move Y events first".into(),
                ));
            }
            for (line, layer) in groups {
                for kind in [EventType::MoveX, EventType::MoveY] {
                    for (index, event) in self.editor.document().events(line, layer, kind)? {
                        if marks.contains(&(line, layer, event.start_time, event.end_time)) {
                            selected.events.push(EventId {
                                line,
                                layer,
                                kind,
                                index,
                            });
                        }
                    }
                }
            }
            selected.checked(self.editor.document())
        })();
        match result {
            Ok(selected) => {
                self.set_selection(selected);
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub(super) fn selection_bounds(&mut self) {
        match self
            .timing_selection()
            .and_then(|s| ChartClipboard::copy(self.editor.document(), &s))
        {
            Ok(clipboard) => {
                self.tools.start = clipboard.first.to_string();
                self.tools.end = clipboard.last.to_string();
                let mut notes: Vec<_> = self
                    .selection()
                    .notes
                    .iter()
                    .filter_map(|id| self.editor.document().note(id.line, id.index).ok())
                    .collect();
                notes.sort_by_key(|n| n.start_time);
                if let (Some(a), Some(b)) = (notes.first(), notes.last()) {
                    self.tools.pattern_x = [a.x, b.x];
                }
                self.tools.include_ends = false;
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
}
