use crate::{i18n::tr, scroll::TouchCombo as _, workbench::Workbench};
use phistudio_core::{DefaultNoteSound, NoteCreationDefaults};

impl Workbench {
    pub(crate) fn note_defaults_changed(&mut self, before: &NoteCreationDefaults) {
        let next = &self.options.new_note_defaults;
        let audio = &mut self.options.audio_notes;
        if before.size != next.size
            || before.speed != next.speed
            || before.alpha != next.alpha
            || before.y_offset != next.y_offset
            || before.is_fake != next.is_fake
            || before.above != next.above
        {
            audio.use_creation_defaults = true;
        }
        if before.size != next.size {
            audio.size = next.size;
        }
        if before.speed != next.speed {
            audio.speed = next.speed;
        }
        if before.alpha != next.alpha {
            audio.alpha = next.alpha;
        }
        if before.y_offset != next.y_offset {
            audio.y_offset = next.y_offset;
        }
        if before.is_fake != next.is_fake {
            audio.fake = next.is_fake != 0;
        }
        if before.above != next.above {
            audio.side = if next.above == 1 {
                phistudio_core::tools::AudioSide::Above
            } else {
                phistudio_core::tools::AudioSide::Below
            };
        }
    }
    pub(crate) fn use_selected_note_defaults(&mut self) {
        let ids = self.selection().notes;
        if ids.len() != 1 {
            return;
        }
        let id = ids[0];
        let result = (|| -> Result<NoteCreationDefaults, String> {
            let note = self
                .editor
                .document()
                .note(id.line, id.index)
                .map_err(|e| e.to_string())?;
            let gain = self.editor.document().note_sound_gain(id);
            let reference = match &gain {
                Some(gain) => gain.source.clone(),
                None => self
                    .editor
                    .document()
                    .note_hitsound(id)
                    .map_err(|e| e.to_string())?
                    .map(str::to_owned),
            };
            let sound = reference.map(|reference| DefaultNoteSound {
                project: (!phistudio_core::builtin_hitsound(&reference))
                    .then(|| self.note_default_scope())
                    .flatten(),
                reference,
            });
            let defaults = NoteCreationDefaults {
                above: u8::from(note.above == 1),
                is_fake: u8::from(note.is_fake != 0),
                alpha: note.alpha,
                size: note.size,
                speed: note.speed,
                visible_time: note.visible_time,
                y_offset: note.y_offset,
                judge_area: note.judge_area,
                tint_hit_effects: note.tint_hit_effects,
                volume: gain.map_or(1., |g| g.volume),
                sound,
            };
            defaults.validate().map_err(|e| e.to_string())?;
            Ok(defaults)
        })();
        match result {
            Ok(defaults) => {
                let before = std::mem::replace(&mut self.options.new_note_defaults, defaults);
                self.note_defaults_changed(&before);
                self.status = "New-note defaults saved".into();
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
    pub(crate) fn new_note_defaults_ui(&mut self, ui: &mut egui::Ui) {
        let before = self.options.new_note_defaults.clone();
        let selected = self.selection().notes.len() == 1;
        let scope = self.note_default_scope();
        let mut choose_color = None;
        ui.push_id("new-note-defaults", |ui| {
            let content = egui::CollapsingHeader::new(tr("New note defaults"))
                .id_source("new-note-defaults-body")
                .show(ui, |ui| {
                    ui.small(tr("Newly placed notes use these properties. Time, position and note type come from the placement tool."));
                    if ui.add_enabled(selected, egui::Button::new(tr("Use selected note as defaults")).wrap(true)).clicked() {
                        self.use_selected_note_defaults();
                    }
                    choose_color = note_defaults_controls(ui, &mut self.options.new_note_defaults, scope.as_deref());
                });
            ui.ctx().data_mut(|data| {
                data.insert_temp(egui::Id::new("new-note-defaults-header"), content.header_response.rect);
                data.insert_temp(egui::Id::new("new-note-defaults-open"), content.body_returned.is_some());
            });
        });
        if self.options.new_note_defaults != before {
            self.note_defaults_changed(&before);
        }
        if let Some(rgb) = choose_color {
            self.open_note_default_color(rgb);
        }
    }
}

fn note_defaults_controls(
    ui: &mut egui::Ui,
    defaults: &mut NoteCreationDefaults,
    scope: Option<&std::path::Path>,
) -> Option<[u8; 3]> {
    let form = crate::form::Form::new(
        ui,
        &[
            "Size",
            "Speed",
            "Visible time",
            "Y offset",
            "Alpha",
            "Judge area",
            "Volume (%)",
        ],
    );
    let mut fields = Vec::new();
    for (label, value, speed) in [
        ("Size", &mut defaults.size, 0.01),
        ("Speed", &mut defaults.speed, 0.01),
        ("Visible time", &mut defaults.visible_time, 0.1),
        ("Y offset", &mut defaults.y_offset, 1.),
    ] {
        let response = form.field(ui, label, |ui| {
            ui.add(
                crate::numeric::Value::new(value)
                    .speed(speed)
                    .clamp_range(-1e9..=1e9),
            )
        });
        fields.push(response.rect);
    }
    form.field(ui, "Alpha", |ui| {
        ui.add(crate::numeric::Value::new(&mut defaults.alpha).clamp_range(0..=u16::MAX))
    });
    let mut below = defaults.above != 1;
    let mut fake = defaults.is_fake != 0;
    ui.horizontal_wrapped(|ui| {
        if ui.checkbox(&mut below, tr("Below the line")).changed() {
            defaults.above = u8::from(!below);
        }
        if ui.checkbox(&mut fake, tr("Fake")).changed() {
            defaults.is_fake = u8::from(fake);
        }
    });
    let mut area = defaults.judge_area.is_some();
    if ui
        .checkbox(&mut area, tr("Custom default judge area"))
        .changed()
    {
        defaults.judge_area = area.then_some(1.);
    }
    if let Some(value) = &mut defaults.judge_area {
        form.field(ui, "Judge area", |ui| {
            ui.add(
                crate::numeric::Value::new(value)
                    .speed(0.01)
                    .clamp_range(0. ..=f32::MAX as f64),
            )
        });
    }
    let mut tinted = defaults.tint_hit_effects.is_some();
    if ui
        .checkbox(&mut tinted, tr("Custom hit effect color"))
        .changed()
    {
        defaults.tint_hit_effects = tinted.then_some([255, 236, 160]);
    }
    let choose_color = defaults
        .tint_hit_effects
        .filter(|rgb| crate::color_ui::swatch(ui, *rgb, "Hit effect color").clicked());
    let mut volume = defaults.volume * 100.;
    if form
        .field(ui, "Volume (%)", |ui| {
            ui.add(
                crate::numeric::Value::new(&mut volume)
                    .speed(0.5)
                    .clamp_range(0. ..=100.),
            )
        })
        .changed()
    {
        defaults.volume = volume / 100.;
    }
    let custom = defaults.sound.clone().filter(|s| s.project.is_some());
    egui::ComboBox::from_id_source("new-note-sound")
        .width(ui.available_width().min(320.))
        .wrap(true)
        .selected_text(
            defaults
                .sound
                .as_ref()
                .map_or_else(|| tr("Use note type sound"), |s| s.reference.clone()),
        )
        .touch_show_ui(ui, |ui| {
            ui.selectable_value(&mut defaults.sound, None, tr("Use note type sound"));
            for (reference, label) in [
                ("tap.mp3", "Tap"),
                ("flick.mp3", "Flick"),
                ("drag.mp3", "Drag"),
            ] {
                ui.selectable_value(
                    &mut defaults.sound,
                    Some(DefaultNoteSound {
                        reference: reference.into(),
                        project: None,
                    }),
                    tr(label),
                );
            }
            if let Some(custom) = custom {
                ui.selectable_value(&mut defaults.sound, Some(custom.clone()), custom.reference);
            }
        });
    if defaults
        .sound
        .as_ref()
        .is_some_and(|s| s.project.is_some() && s.project.as_deref() != scope)
    {
        ui.small(tr("The custom default hitsound belongs to another project. Notes here use their note-type sound."));
    }
    ui.small(tr(
        "For a custom sound, set it on a note and use that note as defaults.",
    ));
    let reset = ui.button(tr("Reset note defaults"));
    ui.ctx().data_mut(|data| {
        data.insert_temp(egui::Id::new("new-note-default-fields"), fields);
        data.insert_temp(egui::Id::new("reset-new-note-defaults"), reset.rect);
    });
    if reset.clicked() {
        *defaults = Default::default();
        return None;
    }
    choose_color
}
