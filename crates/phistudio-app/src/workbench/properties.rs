use super::*;

impl Workbench {
    pub(super) fn preview_panel(&mut self, ui: &mut egui::Ui) {
        self.preview_panel_impl(ui);
    }

    pub(super) fn properties(&mut self, ui: &mut egui::Ui) {
        if self.selected.is_some() {
            self.plugin_context_menu(ui, phistudio_sdk::ContextTarget::Note);
        }
        let Some(id) = self.selected else {
            ui.label(RichText::new(crate::i18n::tr("Note properties")).strong());
            ui.label(
                RichText::new(crate::i18n::tr(
                    "Select a note on the grid to edit it. Drag upward to place a Hold.",
                ))
                .color(muted()),
            );
            ui.add_space(8.);
            ui.small(RichText::new(crate::i18n::tr("Line events share the timeline on the right. Use the right grid selector for Storyboard.")).color(muted()));
            return;
        };
        let revision = self.editor.revision();
        if self
            .draft
            .as_ref()
            .is_none_or(|d| d.id != id || d.revision != revision)
        {
            match self.editor.document().note(id.line, id.index) {
                Ok(note) => {
                    self.draft = Some(NoteDraft {
                        id,
                        start: note.start_time.to_string(),
                        end: note.end_time.to_string(),
                        volume: self
                            .editor
                            .document()
                            .note_sound_gain(id)
                            .map_or(100., |g| g.volume * 100.),
                        note,
                        revision,
                    })
                }
                Err(_) => {
                    self.select(None);
                    return;
                }
            }
        }
        let draft = self.draft.as_mut().unwrap();
        let mut commit = false;
        let mut volume_commit = false;
        let mut choose_color = false;
        let mut error = None;
        ui.label(
            RichText::new(crate::i18n::format(
                "Note {0}",
                &[(id.index + 1).to_string()],
            ))
            .strong(),
        );
        ui.add(
            egui::Label::new(
                RichText::new(crate::i18n::tr("Changes apply when editing ends"))
                    .small()
                    .color(muted()),
            )
            .wrap(true),
        );
        ui.horizontal_wrapped(|ui| {
            for (kind, label) in [
                (NoteKind::Tap, "Tap"),
                (NoteKind::Hold, "Hold"),
                (NoteKind::Flick, "Flick"),
                (NoteKind::Drag, "Drag"),
            ] {
                if ui
                    .selectable_value(&mut draft.note.kind, kind, crate::i18n::tr(label))
                    .changed()
                {
                    commit = true;
                }
            }
        });
        ui.push_id("note-fields", |ui| {
            let form = crate::form::Form::new(
                ui,
                &[
                    "Start",
                    "End",
                    "Position X",
                    "Size",
                    "Speed",
                    "Visible time",
                    "Y offset",
                    "Alpha",
                    "Judge area",
                    "Volume (%)",
                ],
            );
            let response = form.field(ui, "Start", |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut draft.start)
                        .desired_width(ui.available_width())
                        .hint_text("0:1/4"),
                )
            });
            if response.lost_focus()
                || (response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                match draft.start.parse::<Beat>() {
                    Ok(beat) => {
                        draft.note.start_time = beat;
                        commit = true;
                    }
                    Err(e) => error = Some(e.to_string()),
                }
            }
            if draft.note.kind == NoteKind::Hold {
                let response = form.field(ui, "End", |ui| ui.text_edit_singleline(&mut draft.end));
                if response.lost_focus()
                    || (response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    match draft.end.parse::<Beat>() {
                        Ok(beat) => {
                            draft.note.end_time = beat;
                            commit = true;
                        }
                        Err(e) => error = Some(e.to_string()),
                    }
                }
            }
            for (label, value, speed) in [
                ("Position X", &mut draft.note.x, 5.),
                ("Size", &mut draft.note.size, 0.01),
                ("Speed", &mut draft.note.speed, 0.01),
                ("Visible time", &mut draft.note.visible_time, 0.1),
                ("Y offset", &mut draft.note.y_offset, 1.),
            ] {
                let response = form.field(ui, label, |ui| {
                    ui.add(crate::numeric::Value::new(value).speed(speed))
                });
                commit |= response.drag_released() || response.lost_focus();
            }
            let response = form.field(ui, "Alpha", |ui| {
                ui.add(crate::numeric::Value::new(&mut draft.note.alpha).clamp_range(0..=u16::MAX))
            });
            commit |= response.drag_released() || response.lost_focus();
            let mut area = draft.note.judge_area.unwrap_or(1.);
            let response = form.field(ui, "Judge area", |ui| {
                ui.add(crate::numeric::Value::new(&mut area).speed(0.01).clamp_range(0. ..=f32::MAX as f64))
                    .on_hover_text(crate::i18n::tr("Hit area multiplier. 1 is the normal Phira judge area; note size is independent."))
            });
            if response.changed() { draft.note.judge_area = Some(area); }
            commit |= response.drag_released() || response.lost_focus();
            let response = form.field(ui, "Volume (%)", |ui| {
                ui.add(crate::numeric::Value::new(&mut draft.volume).speed(0.5).clamp_range(0. ..=100.))
            });
            volume_commit |= response.drag_released() || response.lost_focus();
        });
        ui.small(crate::i18n::tr("Volume uses a custom WAV from the current sound/skin, included in the package for Phira. 100% restores the original sound."));
        let mut custom_color = draft.note.tint_hit_effects.is_some();
        if ui
            .checkbox(
                &mut custom_color,
                crate::i18n::tr("Custom hit effect color"),
            )
            .changed()
        {
            draft.note.tint_hit_effects = custom_color.then_some([255, 236, 160]);
            commit = true;
        }
        if let Some(rgb) = draft.note.tint_hit_effects {
            choose_color = crate::color_ui::swatch(ui, rgb, "Hit effect color").clicked();
        }
        ui.horizontal_wrapped(|ui| {
            let mut below = draft.note.above != 1;
            if ui
                .checkbox(&mut below, crate::i18n::tr("Below the line"))
                .changed()
            {
                draft.note.above = u8::from(!below);
                commit = true;
            }
            let mut fake = draft.note.is_fake != 0;
            if ui.checkbox(&mut fake, crate::i18n::tr("Fake")).changed() {
                draft.note.is_fake = u8::from(fake);
                commit = true;
            }
        });
        if draft.note.kind != NoteKind::Hold {
            draft.note.end_time = draft.note.start_time;
        }
        let updated = draft.note.clone();
        let volume = draft.volume / 100.;
        let color = draft.note.tint_hit_effects;
        if let Some(error) = error {
            self.error = Some(error);
        } else if commit {
            self.execute(Command::UpdateNote { id, note: updated });
        }
        if choose_color && let Some(rgb) = color {
            self.open_note_color(id, rgb);
        }
        if volume_commit {
            self.set_note_volume(id, volume);
        }
        if ui
            .add_enabled(
                self.selection().notes.len() == 1,
                egui::Button::new(crate::i18n::tr("Note speed curve…")).wrap(true),
            )
            .clicked()
        {
            self.open_speed_curve();
        }
        if ui.button(crate::i18n::tr("Hitsounds…")).clicked() {
            self.open_sounds();
        }
        if ui
            .button(crate::i18n::tr("Use selected note as defaults"))
            .clicked()
        {
            self.use_selected_note_defaults();
        }
        if ui
            .button(RichText::new(crate::i18n::tr("Delete note")).color(flick()))
            .clicked()
        {
            self.delete_selected();
        }
    }
}
