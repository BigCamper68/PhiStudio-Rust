use super::*;

impl Workbench {
    pub(crate) fn open_storyboard(&mut self) {
        self.advanced.right_mode = RightMode::Storyboard;
        self.select(None);
        self.tools.scope = crate::toolkit::Scope::Events;
    }

    pub(crate) fn select_story(&mut self, id: Option<StoryId>) {
        self.select(None);
        self.advanced.story_selected = id;
        self.advanced.story_selection = id.into_iter().collect();
        if let Some(id) = id {
            self.show_context = true;
            self.properties_scroll = true;
            self.load_story(id.line, id.kind, Some(id.index));
        }
    }

    pub(crate) fn apply_story(
        &mut self,
        id: Option<StoryId>,
        kind: StoryKind,
        event: Option<StoryEvent>,
    ) {
        let line = id.map_or(self.line, |i| i.line);
        if self.commit_authoring(Command::EditStoryboard {
            line,
            kind,
            index: id.map(|i| i.index),
            event: event.clone(),
        }) && let Some(event) = event
        {
            let index = self
                .editor
                .document()
                .story_events(line, kind)
                .ok()
                .and_then(|v| {
                    v.iter().position(|e| {
                        e.start_time == event.start_time && e.end_time == event.end_time
                    })
                });
            self.select_story(index.map(|index| StoryId { line, kind, index }));
        }
    }

    pub(super) fn load_story(&mut self, line: usize, kind: StoryKind, index: Option<usize>) {
        let event = index
            .and_then(|i| {
                self.editor
                    .document()
                    .story_events(line, kind)
                    .ok()?
                    .get(i)
                    .cloned()
            })
            .unwrap_or_else(|| {
                let start = Beat::from_grid(self.beat.max(0.), self.division).unwrap_or(Beat::ZERO);
                StoryEvent::new(kind, start, start.checked_add(Beat::ONE).unwrap_or(start))
            });
        self.advanced.story = Some(StoryDraft {
            line,
            kind,
            index,
            start: event.start_time.to_string(),
            end: event.end_time.to_string(),
            event,
            revision: self.editor.revision(),
        });
    }

    pub(crate) fn story_properties(&mut self, ui: &mut egui::Ui) {
        ui.strong(crate::i18n::tr("Storyboard properties"));
        let Some(id) = self.advanced.story_selected else {
            ui.label(crate::i18n::tr("Use + Event and drag a track to create a storyboard event. Select an event to edit it."));
            return;
        };
        if self.advanced.story.as_ref().is_none_or(|d| {
            d.revision != self.editor.revision()
                || d.line != id.line
                || d.kind != id.kind
                || d.index != Some(id.index)
        }) {
            self.load_story(id.line, id.kind, Some(id.index));
        }
        let Some(mut d) = self.advanced.story.take() else {
            return;
        };
        ui.label({
            let __i18n_arg_0 = &(d.line);
            let __i18n_arg_1 = &(crate::i18n::tr(d.kind.label()));
            crate::i18n::format(
                "L{0} · {1}",
                &[__i18n_arg_0.to_string(), __i18n_arg_1.to_string()],
            )
        });
        let count = self.story_selection().len();
        if count > 1 {
            ui.label(crate::i18n::format(
                "{0} Storyboard events selected",
                &[format!("{}", count)],
            ));
            if ui.button(crate::i18n::tr("Storyboard tools…")).clicked() {
                self.tools.show = true;
            }
        }
        let mut apply = false;
        let mut delete = false;
        let mut apply_selection = false;
        let mut apply_easing = false;
        ui.push_id("story-fields", |ui| {
            let form = crate::form::Form::new(
                ui,
                &[
                    "Start beat",
                    "End beat",
                    "Start value",
                    "End value",
                    "Link group",
                ],
            );
            for (label, text) in [("Start beat", &mut d.start), ("End beat", &mut d.end)] {
                form.field(ui, label, |ui| {
                    ui.text_edit_singleline(text);
                });
            }
            for (end, label, value) in [
                (false, "Start value", &mut d.event.start),
                (true, "End value", &mut d.event.end),
            ] {
                form.field(ui, label, |ui| match d.kind {
                    StoryKind::Color => {
                        let rgb = std::array::from_fn(|i| {
                            value.get(i).and_then(|v| v.as_u64()).unwrap_or(255) as u8
                        });
                        if crate::color_ui::swatch(ui, rgb, "Choose color…").clicked() {
                            self.open_story_color(id, end, rgb);
                        }
                    }
                    StoryKind::Text => {
                        let mut text = value.as_str().unwrap_or_default().to_owned();
                        if ui.text_edit_singleline(&mut text).changed() {
                            *value = serde_json::json!(text);
                        }
                    }
                    _ => {
                        let mut n = value.as_f64().unwrap_or(0.);
                        if ui
                            .add(
                                crate::numeric::Value::new(&mut n)
                                    .speed(0.02)
                                    .max_decimals(5),
                            )
                            .changed()
                        {
                            *value = serde_json::json!(n);
                        }
                    }
                });
            }
            let mut curve = story_curve(&d.event);
            crate::authoring_ui::easing_fields(ui, &mut curve);
            d.event.easing_type = curve.easing_type;
            d.event.easing_left = curve.easing_left;
            d.event.easing_right = curve.easing_right;
            d.event.bezier = curve.bezier;
            d.event.bezier_points = curve.bezier_points;
            form.field(ui, "Link group", |ui| {
                ui.add(crate::numeric::Value::new(&mut d.event.linkgroup).clamp_range(0..=i32::MAX))
            });
        });
        crate::authoring_ui::draw_curve(
            ui,
            phistudio_core::EventType::MoveX,
            &story_curve(&d.event),
        );
        ui.horizontal_wrapped(|ui| {
            apply = ui
                .button(crate::i18n::tr("Apply storyboard event"))
                .clicked();
            delete = ui
                .add_enabled(
                    d.index.is_some(),
                    egui::Button::new(crate::i18n::tr("Delete storyboard event")),
                )
                .clicked();
        });
        if count > 1 {
            apply_selection = ui
                .button({
                    let __i18n_arg_0 = &(crate::i18n::tr(d.kind.label()));
                    crate::i18n::format(
                        "Apply values and easing to selected {0} events",
                        std::slice::from_ref(__i18n_arg_0),
                    )
                })
                .clicked();
            apply_easing = ui
                .button(crate::i18n::tr("Apply easing to all selected events"))
                .clicked();
        }

        if apply_selection || apply_easing {
            let edit = if apply_selection {
                phistudio_core::story_tools::properties(
                    self.editor.document(),
                    &self.story_selection(),
                    d.kind,
                    &d.event,
                )
            } else {
                phistudio_core::story_tools::transform(
                    self.editor.document(),
                    &self.story_selection(),
                    phistudio_core::tools::Transform::Curve {
                        easing: d.event.easing_type,
                        window: [d.event.easing_left, d.event.easing_right],
                        bezier: (d.event.bezier != 0).then_some(d.event.bezier_points),
                    },
                )
            };
            if !self.apply_story_tool(edit) {
                self.advanced.story = Some(d);
            }
            return;
        }

        if apply || delete {
            let result = (|| -> Result<_, phistudio_core::ChartError> {
                if delete {
                    return Ok(None);
                }
                d.event.start_time = d.start.parse()?;
                d.event.end_time = d.end.parse()?;
                d.event.validate(d.kind)?;
                Ok(Some(d.event.clone()))
            })();
            match result {
                Ok(event) => self.apply_story(Some(id), id.kind, event),
                Err(e) => {
                    self.error = Some(e.to_string());
                    self.advanced.story = Some(d);
                }
            }
        } else {
            self.advanced.story = Some(d);
        }
    }
}
