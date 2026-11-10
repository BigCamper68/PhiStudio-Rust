use super::*;
use crate::{
    i18n::{format, tr},
    media::{MediaFile, MediaKind},
    scroll::{TouchCombo as _, TouchScroll as _},
};
use phistudio_core::{EditorSoundCategory as Category, EditorSoundPack, EditorSoundTone};

impl Workbench {
    pub(crate) fn editor_sounds_ui(&mut self, ui: &mut egui::Ui) {
        ui.collapsing(tr("Editor sounds"), |ui| {
            let width = ui
                .available_width()
                .min(ui.clip_rect().right() - ui.next_widget_position().x)
                .max(1.);
            ui.set_width(width);
            ui.style_mut().wrap = Some(true);
            ui.checkbox(
                &mut self.options.editor_sounds.enabled,
                tr("Enable editor sounds"),
            );
            volume(
                ui,
                &mut self.options.editor_sounds.volume,
                "Editor sound volume",
            );
            ui.push_id("sound-settings", |ui| {
                pack_selector(ui, &mut self.options.editor_sounds)
            });
            if ui.button(tr("Editor Sounds Manager")).clicked() {
                self.sounds.open = true;
            }
        });
    }
    pub(crate) fn accept_editor_sound(&mut self, event: Event, file: MediaFile) {
        let target = self.sounds.import_target.take();
        let Some(pack) = self
            .options
            .editor_sounds
            .packs
            .iter_mut()
            .find(|p| Some(&p.id) == target.as_ref())
        else {
            self.error = Some("The target sound pack no longer exists".into());
            return;
        };
        let mut rule = pack.rule(event);
        if rule.files.len() >= 4 {
            self.error = Some("An action supports up to four sound variants".into());
            return;
        }
        rule.files.push(file.path().to_string_lossy().into_owned());
        pack.events.insert(event, rule);
        self.sounds.warning = None;
    }
    pub(crate) fn editor_sounds_window(&mut self, ctx: &egui::Context) {
        self.poll_sound_pack_export();
        if !self.sounds.open {
            return;
        }
        let enabled = !self.modal_open_except_editor_sounds() && !self.files.busy;
        let mut open = !enabled || !ctx.input(|i| i.key_pressed(egui::Key::Escape));
        crate::i18n::window("Editor Sounds Manager")
            .open(&mut open)
            .default_width(720.)
            .default_height((ctx.screen_rect().height() - 120.).clamp(180., 640.))
            .default_pos([24., 40.])
            .resize(|r| {
                r.max_size([
                    (ctx.screen_rect().width() - 32.).max(180.),
                    (ctx.screen_rect().height() - 72.).max(120.),
                ])
            })
            .show(ctx, |ui| {
                ui.set_enabled(enabled);
                ui.style_mut().wrap = Some(true);
                egui::ScrollArea::vertical()
                    .id_source("sound-manager-scroll")
                    .max_height((ctx.screen_rect().height() - 120.).max(100.))
                    .touch_show_y(ui, |ui| self.sound_manager_contents(ui));
            });
        self.sounds.open &= open;
    }
    fn sound_manager_contents(&mut self, ui: &mut egui::Ui) {
        let old_pack = self.options.editor_sounds.selected_pack.clone();
        pack_selector(ui, &mut self.options.editor_sounds);
        if old_pack != self.options.editor_sounds.selected_pack {
            self.sounds.delete_confirm = false;
        }
        ui.collapsing(tr("Manage sound packs"), |ui| self.sound_pack_controls(ui));
        let mut pack = self.options.editor_sounds.pack().into_owned();
        let builtin = EditorSoundPack::is_builtin_id(&pack.id);
        if builtin {
            ui.small(tr("Built-in pack. Duplicate it to customize sounds."));
        }
        ui.checkbox(
            &mut self.options.editor_sounds.mute_during_playback,
            tr("Mute editor sounds during playback"),
        );
        ui.separator();
        let ui_height = ui.ctx().screen_rect().height();
        if ui.available_width() >= 600. {
            ui.columns(2, |columns| {
                let height = (ui_height - 250.).clamp(140., 440.);
                self.sound_action_list(&mut columns[0], &mut pack, builtin, height);
                egui::ScrollArea::vertical()
                    .id_source("sound-action-details")
                    .max_height(height)
                    .touch_show_y(&mut columns[1], |ui| {
                        self.sound_action_details(ui, &mut pack, builtin)
                    });
            });
        } else {
            self.sound_action_list(ui, &mut pack, builtin, 160.);
            ui.separator();
            self.sound_action_details(ui, &mut pack, builtin);
        }
        ui.separator();
        ui.collapsing(tr("Sound pack playback settings"), |ui| {
            ui.set_enabled(!builtin);
            ui.label(tr("Built-in sound palette"));
            egui::ComboBox::from_id_source("sound-pack-palette")
                .selected_text(tr(pack.style.label()))
                .touch_show_ui(ui, |ui| {
                    for style in EditorSoundStyle::ALL {
                        ui.selectable_value(&mut pack.style, style, tr(style.label()));
                    }
                });
            ui.label(tr("Sound pitch"));
            ui.add(egui::Slider::new(&mut pack.pitch, 0.5..=2.));
            ui.label(tr("Repeat interval (ms)"));
            ui.add(egui::Slider::new(&mut pack.cooldown_ms, 20..=500));
            for category in Category::ALL {
                ui.label(tr(category.label()));
                let mut custom = pack.category_intervals.contains_key(&category);
                if ui
                    .checkbox(&mut custom, tr("Custom category repeat interval"))
                    .changed()
                {
                    if custom {
                        pack.category_intervals.insert(category, pack.cooldown_ms);
                    } else {
                        pack.category_intervals.remove(&category);
                    }
                }
                if let Some(interval) = pack.category_intervals.get_mut(&category) {
                    ui.add(egui::Slider::new(interval, 20..=500).suffix(" ms"));
                }
            }
        });
        if let Some(warning) = &self.sounds.warning {
            ui.colored_label(
                crate::theme::drag(),
                tr("Some custom sounds could not be loaded; using built-in sounds."),
            );
            ui.small(warning);
        }
        if !builtin
            && let Some(saved) = self
                .options
                .editor_sounds
                .packs
                .iter_mut()
                .find(|p| p.id == pack.id)
        {
            *saved = pack;
        }
    }
    fn sound_pack_controls(&mut self, ui: &mut egui::Ui) {
        let builtin = EditorSoundPack::is_builtin_id(&self.options.editor_sounds.selected_pack);
        let room = self.options.editor_sounds.packs.len() < EditorSoundOptions::MAX_PACKS;
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(room, egui::Button::new(tr("New sound pack")))
                .clicked()
            {
                self.options
                    .editor_sounds
                    .add_pack(EditorSoundPack::default());
            }
            if ui
                .add_enabled(room, egui::Button::new(tr("Duplicate sound pack")))
                .clicked()
            {
                let mut pack = self.options.editor_sounds.pack().into_owned();
                pack.name = format("{0} copy", &[pack.name]);
                self.options.editor_sounds.add_pack(pack);
            }
            if ui
                .add_enabled(
                    room && self.sounds.export.is_none(),
                    egui::Button::new(tr("Import sound pack…")),
                )
                .clicked()
            {
                self.open_media_picker(MediaKind::SoundPack);
            }
            if ui
                .add_enabled(
                    self.sounds.export.is_none(),
                    egui::Button::new(tr("Export sound pack…")),
                )
                .clicked()
            {
                let pack = self.options.editor_sounds.pack().into_owned();
                let (sender, receiver) = mpsc::channel();
                self.sounds.export = Some(receiver);
                std::thread::spawn(move || {
                    let _ = sender.send(archive::export(pack).map_err(|e| format!("{e:#}")));
                });
            }
            if ui
                .add_enabled(!builtin, egui::Button::new(tr("Delete sound pack")))
                .clicked()
            {
                self.sounds.delete_confirm = true;
            }
        });
        if self.sounds.export.is_some() {
            ui.spinner();
        }
        if self.sounds.delete_confirm {
            ui.label(tr(
                "Delete this sound pack? Imported audio stays in the library.",
            ));
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr("Delete")).clicked() {
                    let selected = self.options.editor_sounds.selected_pack.clone();
                    self.options
                        .editor_sounds
                        .packs
                        .retain(|p| p.id != selected);
                    self.options.editor_sounds.selected_pack =
                        EditorSoundPack::builtin(EditorSoundStyle::Soft).id;
                    self.sounds.delete_confirm = false;
                }
                if ui.button(tr("Cancel")).clicked() {
                    self.sounds.delete_confirm = false;
                }
            });
        }
        if let Some(pack) = self.options.editor_sounds.selected_pack_mut() {
            ui.label(tr("Sound pack name"));
            ui.add(egui::TextEdit::singleline(&mut pack.name).desired_width(ui.available_width()));
            ui.small(tr("Changes are saved automatically in this sound pack."));
        }
    }
    fn sound_action_list(
        &mut self,
        ui: &mut egui::Ui,
        pack: &mut EditorSoundPack,
        builtin: bool,
        height: f32,
    ) {
        ui.strong(tr("Actions"));
        egui::ScrollArea::vertical()
            .id_source("sound-action-list")
            .max_height(height)
            .touch_show_y(ui, |ui| {
                for category in Category::ALL {
                    ui.strong(tr(category.label()));
                    for event in Event::ALL.into_iter().filter(|e| e.category() == category) {
                        ui.push_id(event as usize, |ui| {
                            ui.horizontal(|ui| {
                                let mut enabled = pack.rule(event).enabled;
                                if ui
                                    .add_enabled(
                                        !builtin,
                                        egui::Checkbox::without_text(&mut enabled),
                                    )
                                    .changed()
                                {
                                    let mut rule = pack.rule(event);
                                    rule.enabled = enabled;
                                    pack.events.insert(event, rule);
                                }
                                if ui
                                    .selectable_label(
                                        self.sounds.selected == event,
                                        tr(event.label()),
                                    )
                                    .clicked()
                                {
                                    self.sounds.selected = event;
                                }
                            });
                        });
                    }
                    ui.add_space(8.);
                }
            });
    }
    fn sound_action_details(
        &mut self,
        ui: &mut egui::Ui,
        pack: &mut EditorSoundPack,
        builtin: bool,
    ) {
        let event = self.sounds.selected;
        ui.strong(tr(event.label()));
        let original = pack.rule(event);
        let mut rule = original.clone();
        let mut reset = false;
        ui.add_enabled_ui(!builtin, |ui| {
            ui.checkbox(&mut rule.enabled, tr("Enable this action sound"));
            volume(ui, &mut rule.volume, "Action sound volume");
            ui.label(tr("Action sound pitch")); ui.add(egui::Slider::new(&mut rule.pitch,0.5..=2.));
            egui::ComboBox::from_id_source("editor-sound-tone").selected_text(tr(rule.tone.unwrap_or(event.default_tone()).label())).touch_show_ui(ui, |ui| {
                ui.selectable_value(&mut rule.tone, None, tr("Default sound"));
                for tone in EditorSoundTone::ALL { ui.selectable_value(&mut rule.tone, Some(tone), tr(tone.label())); }
            });
            ui.small(tr("The built-in tone is used when there are no custom variants or a file is unavailable."));
        });
        if ui.button(tr("Test sound")).clicked() {
            self.sounds.preview = Some(event);
            self.sounds.preview_slot = None;
        }
        ui.strong(tr("Sound variants"));
        let mut remove = None;
        for (index, path) in rule.files.iter().enumerate() {
            ui.push_id(index, |ui| {
                let name = std::path::Path::new(path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                ui.label(name.as_ref());
                if let Some(info) = self.sounds.files.iter().find(|info| &info.path == path) {
                    ui.small(format!(
                        "{:.0} ms · {:.1} KiB",
                        info.duration * 1000.,
                        info.bytes as f32 / 1024.
                    ));
                    waveform(ui, &info.peaks);
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button(tr("Test sound")).clicked() {
                        self.sounds.preview = Some(event);
                        self.sounds.preview_slot = Some(index);
                    }
                    if ui
                        .add_enabled(!builtin, egui::Button::new(tr("Remove sound variant")))
                        .clicked()
                    {
                        remove = Some(index);
                    }
                });
            });
        }
        if let Some(index) = remove {
            rule.files.remove(index);
        }
        ui.add_enabled_ui(!builtin, |ui| {
            if rule.files.len() > 1 {
                ui.checkbox(
                    &mut rule.random,
                    tr("Random variants without immediate repeats"),
                );
            }
            if ui
                .add_enabled(
                    rule.files.len() < 4 && !self.files.busy && self.sounds.export.is_none(),
                    egui::Button::new(tr("Add sound variant…")),
                )
                .clicked()
            {
                self.sounds.import_target = Some(pack.id.clone());
                self.open_media_picker(MediaKind::EditorSound(event));
            }
            if ui.button(tr("Reset action sound")).clicked() {
                reset = true;
            }
        });
        ui.small(tr("WAV, OGG, MP3 or FLAC; up to 2 seconds and 2 MiB. Imported sounds are copied to the app library."));
        ui.small(tr(
            "Up to four variants per action. Disable random selection to play them in sequence.",
        ));
        if !builtin {
            if reset {
                pack.events.remove(&event);
            } else if rule != original {
                pack.events.insert(event, rule);
            }
        }
    }
    fn poll_sound_pack_export(&mut self) {
        if self.files.busy || self.files.modal_open() || self.files.request.is_some() {
            return;
        }
        let Some(receiver) = &self.sounds.export else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("Sound pack export interrupted".into()),
        };
        self.sounds.export = None;
        match result {
            Err(error) => self.error = Some(error),
            Ok(package) => {
                #[cfg(any(target_os = "android", target_os = "ios"))]
                {
                    self.files.request = Some(crate::files::FileRequest::ReadyExport(package));
                }
                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                {
                    self.files.export_path = package.name.clone();
                    self.files.export_package = Some(package);
                }
            }
        }
    }
}
fn pack_selector(ui: &mut egui::Ui, options: &mut EditorSoundOptions) {
    let pack = options.pack();
    let name = if EditorSoundPack::is_builtin_id(&pack.id) {
        tr(&pack.name)
    } else {
        pack.name.clone()
    };
    ui.label(tr("Sound pack"));
    egui::ComboBox::from_id_source("editor-sound-pack")
        .width(ui.available_width().min(300.))
        .wrap(true)
        .selected_text(name)
        .touch_show_ui(ui, |ui| {
            for style in EditorSoundStyle::ALL {
                let pack = EditorSoundPack::builtin(style);
                ui.selectable_value(&mut options.selected_pack, pack.id, tr(&pack.name));
            }
            for pack in &options.packs {
                ui.selectable_value(&mut options.selected_pack, pack.id.clone(), &pack.name);
            }
        });
}
fn waveform(ui: &mut egui::Ui, peaks: &[f32]) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(400.), 40.),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 4., crate::theme::raised());
    for (index, peak) in peaks.iter().enumerate() {
        let x = rect.left() + (index as f32 + 0.5) / peaks.len() as f32 * rect.width();
        let h = peak * (rect.height() - 4.) * 0.5;
        ui.painter().line_segment(
            [
                egui::pos2(x, rect.center().y - h),
                egui::pos2(x, rect.center().y + h),
            ],
            egui::Stroke::new(1_f32, crate::theme::accent()),
        );
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
