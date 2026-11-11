use super::*;

impl Workbench {
    pub fn event_properties(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.events.selected else {
            ui.label(crate::i18n::tr(
                "Select an event to edit its values and curve.",
            ));
            return;
        };
        let revision = self.editor.revision();
        if self
            .events
            .draft
            .as_ref()
            .is_none_or(|v| v.id != id || v.revision != revision)
        {
            match self.editor.document().event(id) {
                Ok(event) => {
                    self.events.draft = Some(EventDraft {
                        id,
                        revision,
                        start: event.start_time.to_string(),
                        end: event.end_time.to_string(),
                        event,
                    })
                }
                Err(_) => {
                    self.select_event(None);
                    return;
                }
            }
        }
        let draft = self.events.draft.as_mut().unwrap();
        ui.label(
            RichText::new({
                let __i18n_arg_0 = &(crate::i18n::tr(id.kind.label()));
                let __i18n_arg_1 = &(id.layer);
                let __i18n_arg_2 = &(id.index + 1);
                crate::i18n::format(
                    "{0} · Layer {1} · #{2}",
                    &[
                        __i18n_arg_0.to_string(),
                        __i18n_arg_1.to_string(),
                        __i18n_arg_2.to_string(),
                    ],
                )
            })
            .strong()
            .color(accent()),
        );
        let editable = id.layer < 4;
        ui.add_enabled_ui(editable, |ui| {
            ui.push_id("event-fields", |ui| {
                let form = crate::form::Form::new(
                    ui,
                    &[
                        "Start beat",
                        "End beat",
                        "Start value",
                        "End value",
                        "Constant value",
                        "Link group",
                    ],
                );
                form.field(ui, "Start beat", |ui| {
                    ui.text_edit_singleline(&mut draft.start);
                });
                form.field(ui, "End beat", |ui| {
                    ui.text_edit_singleline(&mut draft.end);
                });
                let mut edited_end = false;
                for (label, value) in [
                    ("Start value", &mut draft.event.start),
                    ("End value", &mut draft.event.end),
                ] {
                    let changed = form.field(ui, label, |ui| {
                        ui.add(crate::numeric::Value::new(value).speed(0.2).max_decimals(5))
                            .changed()
                    });
                    if changed && label == "End value" {
                        edited_end = true;
                    }
                }
                form.field(ui, "Constant value", |ui| {
                    ui.checkbox(&mut draft.event.constant, crate::i18n::tr("Link endpoints"));
                });
                if draft.event.constant {
                    if edited_end {
                        draft.event.start = draft.event.end;
                    } else {
                        draft.event.end = draft.event.start;
                    }
                }
                if id.kind != EventType::Speed || draft.event.custom_easing.is_some() {
                    easing_fields(ui, &mut draft.event);
                }
                form.field(ui, "Link group", |ui| {
                    ui.add(
                        crate::numeric::Value::new(&mut draft.event.link_group)
                            .clamp_range(0..=i32::MAX),
                    )
                });
            });
        });
        if id.kind == EventType::Speed && draft.event.custom_easing.is_none() {
            ui.small(
                RichText::new(crate::i18n::tr(
                    "Speed edits use Linear. Imported easing metadata is preserved.",
                ))
                .color(muted()),
            );
        }
        draw_curve(ui, id.kind, &draft.event);
        let apply = ui
            .add_enabled(editable, egui::Button::new(crate::i18n::tr("Apply event")))
            .clicked();
        let delete = ui
            .add_enabled(
                editable,
                egui::Button::new(RichText::new(crate::i18n::tr("Delete event")).color(flick())),
            )
            .clicked();
        if apply {
            match draft
                .start
                .parse::<Beat>()
                .and_then(|start| draft.end.parse::<Beat>().map(|end| (start, end)))
            {
                Ok((start, end)) => {
                    let mut event = draft.event.clone();
                    event.start_time = start;
                    event.end_time = end;
                    self.commit_authoring(Command::UpdateEvent { id, event });
                }
                Err(e) => self.error = Some(e.to_string()),
            }
        } else if delete {
            self.commit_authoring(Command::DeleteEvent(id));
        }
    }
}
