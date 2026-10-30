use super::*;

impl Workbench {
    pub fn event_rows(&mut self) -> Arc<Vec<(EventId, LineEvent)>> {
        let revision = self.editor.revision();
        if self
            .events
            .cache
            .get(&(self.line, self.events.layer))
            .is_none_or(|v| v.revision != revision)
        {
            let mut rows = Vec::new();
            for kind in EventType::ALL {
                match self
                    .editor
                    .document()
                    .events(self.line, self.events.layer, kind)
                {
                    Ok(events) => rows.extend(events.into_iter().map(|(index, e)| {
                        (
                            EventId {
                                line: self.line,
                                layer: self.events.layer,
                                kind,
                                index,
                            },
                            e,
                        )
                    })),
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
            self.events.cache.retain(|_, v| v.revision == revision);
            self.events.cache.insert(
                (self.line, self.events.layer),
                EventCache {
                    revision,
                    rows: Arc::new(rows),
                },
            );
        }
        Arc::clone(
            &self
                .events
                .cache
                .get(&(self.line, self.events.layer))
                .unwrap()
                .rows,
        )
    }

    pub fn select_event(&mut self, id: Option<EventId>) {
        if let Some(id) = id {
            self.line = id.line;
        }
        self.advanced.story_selection.clear();
        self.properties_scroll |= id.is_some();
        self.advanced.story_selected = None;
        self.advanced.story = None;
        self.tools.selection = phistudio_core::tools::Selection {
            notes: Vec::new(),
            events: id.into_iter().collect(),
        };
        self.selected = None;
        self.draft = None;
        self.events.selected = id;
        self.events.draft = None;
    }

    pub fn clear_event_selection(&mut self) {
        self.tools.selection.events.clear();
        self.events.selected = None;
        self.events.draft = None;
    }

    pub(crate) fn clear_event_draft(&mut self) {
        self.events.draft = None;
    }

    pub fn commit_authoring(&mut self, command: Command) -> bool {
        let anchor = self.anchor();
        if self.options.xy_link || self.options.auto_connect_events {
            let doc = self.editor.document();
            let plan = match &command {
                Command::AddEvent {
                    line,
                    layer,
                    kind,
                    event,
                } => Some(phistudio_core::tools::create_event(
                    doc,
                    *line,
                    *layer,
                    *kind,
                    event.clone(),
                    self.options.xy_link,
                )),
                Command::UpdateEvent { id, event } => {
                    Some(phistudio_core::tools::update_event_auto(
                        doc,
                        *id,
                        event.clone(),
                        self.options.xy_link,
                        self.options.auto_connect_events,
                    ))
                }
                Command::DeleteEvent(id) => Some(
                    self.link_selection(&phistudio_core::tools::Selection {
                        notes: vec![],
                        events: vec![*id],
                    })
                    .and_then(|s| phistudio_core::tools::delete(doc, &s)),
                ),
                _ => None,
            };
            if let Some(plan) = plan {
                let kind = match command {
                    Command::AddEvent { kind, .. } => Some(kind),
                    Command::UpdateEvent { id, .. } => Some(id.kind),
                    _ => None,
                };
                let changed = self.apply_tool(plan);
                if changed
                    && let Some(kind) = kind
                    && let Some(id) = self
                        .selection()
                        .events
                        .into_iter()
                        .find(|id| id.kind == kind)
                {
                    self.select_event(Some(id));
                }
                return changed;
            }
        }
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
                    self.select(None);
                    self.line_draft = None;
                    self.bpm_draft = None;
                    if let Some(EditTarget::Event(id)) = self.editor.last_target() {
                        self.select_event(Some(id));
                    }
                }
                changed
            }
            Err(e) => {
                self.error = Some(e.to_string());
                false
            }
        }
    }

    pub fn event_layer_picker(&mut self, ui: &mut egui::Ui) {
        let before = self.events.layer;
        egui::ComboBox::from_id_source("event-layer")
            .width(85.)
            .selected_text({
                let __i18n_arg_0 = &(self.events.layer);
                crate::i18n::format("Layer {0}", &[__i18n_arg_0.to_string()])
            })
            .touch_show_ui(ui, |ui| {
                for layer in 0..self.editor.document().event_layer_count(self.line).max(4) {
                    ui.selectable_value(&mut self.events.layer, layer, {
                        let __i18n_arg_0 =
                            &(crate::i18n::tr(if layer > 3 { " · read only" } else { "" }));
                        crate::i18n::format(
                            "Layer {0}{1}",
                            &[format!("{}", layer), __i18n_arg_0.to_string()],
                        )
                    });
                }
            });
        if before != self.events.layer {
            self.clear_event_selection();
            self.drag = None;
            self.grid_input.cancel();
            self.unlink_invalid_xy();
        }
        if self.events.layer > 3 {
            ui.small(RichText::new(crate::i18n::tr("Read only")).color(muted()));
        }
    }
}
