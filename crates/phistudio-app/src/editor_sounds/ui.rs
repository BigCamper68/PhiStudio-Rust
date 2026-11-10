use super::*;
use crate::{i18n::tr, media::MediaKind, scroll::TouchCombo as _};
use phistudio_core::{EditorSoundStyle, EditorSoundTone};

impl Workbench {
    pub(crate) fn editor_sounds_ui(&mut self, ui: &mut egui::Ui) {
        ui.collapsing(tr("Editor sounds"), |ui| self.editor_sound_controls(ui));
    }

    fn editor_sound_controls(&mut self, ui: &mut egui::Ui) {
        let options = &mut self.options.editor_sounds;
        ui.checkbox(&mut options.enabled, tr("Enable editor sounds"));
        ui.small(tr(
            "Independent of music and hitsounds. Preferences apply to every project.",
        ));
        volume(ui, &mut options.volume, "Editor sound volume");
        ui.label(tr("Sound pitch"));
        ui.add(egui::Slider::new(&mut options.pitch, 0.5..=2.));
        egui::ComboBox::from_id_source("editor-sound-style")
            .selected_text(tr(options.style.label()))
            .touch_show_ui(ui, |ui| {
                for style in EditorSoundStyle::ALL {
                    ui.selectable_value(&mut options.style, style, tr(style.label()));
                }
            });
        ui.checkbox(
            &mut options.mute_during_playback,
            tr("Mute editor sounds during playback"),
        );
        ui.label(tr("Repeat interval (ms)"));
        ui.add(egui::Slider::new(&mut options.cooldown_ms, 20..=500));
        ui.separator();
        ui.strong(tr("Customize each action"));
        egui::ComboBox::from_id_source("editor-sound-event")
            .selected_text(tr(self.sounds.selected.label()))
            .touch_show_ui(ui, |ui| {
                for event in Event::ALL {
                    ui.selectable_value(&mut self.sounds.selected, event, tr(event.label()));
                }
            });
        let event = self.sounds.selected;
        let rule = options.events.entry(event).or_default();
        ui.checkbox(&mut rule.enabled, tr("Enable this action sound"));
        volume(ui, &mut rule.volume, "Action sound volume");
        ui.label(tr("Action sound pitch"));
        ui.add(egui::Slider::new(&mut rule.pitch, 0.5..=2.));
        egui::ComboBox::from_id_source("editor-sound-tone")
            .selected_text(if rule.file.is_some() {
                tr("Imported sound")
            } else {
                tr(rule.tone.unwrap_or(event.default_tone()).label())
            })
            .touch_show_ui(ui, |ui| {
                if ui
                    .selectable_value(&mut rule.tone, None, tr("Default sound"))
                    .clicked()
                {
                    rule.file = None;
                }
                for tone in EditorSoundTone::ALL {
                    if ui
                        .selectable_value(&mut rule.tone, Some(tone), tr(tone.label()))
                        .clicked()
                    {
                        rule.file = None;
                    }
                }
            });
        if let Some(path) = &rule.file {
            let name = std::path::Path::new(path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            ui.label(crate::i18n::format("Custom sound: {0}", &[name]));
        }
        let mut import = false;
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr("Test sound")).clicked() {
                self.sounds.preview = Some(event);
            }
            import = ui
                .add_enabled(!self.files.busy, egui::Button::new(tr("Import sound…")))
                .clicked();
            if ui.button(tr("Reset action sound")).clicked() {
                options.events.remove(&event);
            }
        });
        ui.small(tr("WAV, OGG, MP3 or FLAC; up to 2 seconds and 2 MiB. Imported sounds are copied to the app library."));
        ui.small(tr(
            "Test sound ignores enable switches. Volume and playback muting still apply.",
        ));
        if let Some(warning) = &self.sounds.warning {
            ui.colored_label(
                crate::theme::drag(),
                tr("Some custom sounds could not be loaded; using built-in sounds."),
            );
            ui.small(warning);
        }
        if ui.button(tr("Reset all editor sounds")).clicked() {
            self.options.editor_sounds = EditorSoundOptions::default();
        }
        if import {
            self.open_media_picker(MediaKind::EditorSound(event));
        }
    }
}

fn volume(ui: &mut egui::Ui, value: &mut f32, label: &str) {
    ui.label(tr(label));
    ui.add(
        egui::Slider::new(value, 0. ..=1.)
            .custom_formatter(|v, _| format!("{:.0}%", v * 100.))
            .custom_parser(|s| {
                s.trim()
                    .trim_end_matches('%')
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .map(|v| v / 100.)
            }),
    );
}
