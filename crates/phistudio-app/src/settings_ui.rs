use crate::{scroll::TouchCombo as _, workbench::Workbench};
use phistudio_core::PreviewAspect;
impl Workbench {
    pub(crate) fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Language"));
            egui::ComboBox::from_id_source("interface-language")
                .selected_text(self.options.language.native_name())
                .touch_show_ui(ui, |ui| {
                    for language in phistudio_core::Language::ALL {
                        if ui
                            .selectable_value(
                                &mut self.options.language,
                                language,
                                language.native_name(),
                            )
                            .changed()
                        {
                            ui.ctx().request_repaint();
                        }
                    }
                });
        });
        ui.separator();
        ui.strong(crate::i18n::tr("Preview"));
        ui.checkbox(&mut self.options.preview_behind_grid, crate::i18n::tr("Preview behind grids"))
            .on_hover_text(crate::i18n::tr("Show the chart behind the note and event grids. Replace the sidebar preview with a fullscreen button."));
        ui.label(crate::i18n::tr("Grid preview brightness"));
        ui.add_enabled(
            self.options.preview_behind_grid,
            egui::Slider::new(&mut self.options.preview_grid_brightness, 0. ..=1.)
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
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Screen ratio"));
            egui::ComboBox::from_id_source("preview-ratio")
                .selected_text(crate::i18n::tr(self.options.preview_aspect.label()))
                .touch_show_ui(ui, |ui| {
                    for ratio in PreviewAspect::ALL {
                        ui.selectable_value(
                            &mut self.options.preview_aspect,
                            ratio,
                            crate::i18n::tr(ratio.label()),
                        );
                    }
                });
        });
        ui.add(
            egui::Slider::new(&mut self.options.preview_note_scale, 0.800..=1.200)
                .step_by(0.001)
                .fixed_decimals(3)
                .text(crate::i18n::tr("Note scale")),
        )
        .on_hover_text(crate::i18n::tr(
            "Phira note_scale: scales notes and hit effects. Default: 1.000.",
        ));
        ui.checkbox(
            &mut self.options.mark_active_line_id,
            crate::i18n::tr("Mark active line ID"),
        )
        .on_hover_text(crate::i18n::tr(
            "Show line IDs in preview and the active ID in the editor header.",
        ));
        ui.checkbox(
            &mut self.options.highlight_current_line,
            crate::i18n::tr("Highlight current line preview"),
        );
        ui.checkbox(&mut self.options.preview_hud, crate::i18n::tr("Show HUD"));
        ui.checkbox(
            &mut self.options.preview_shaders,
            crate::i18n::tr("Enable chart shaders"),
        );
        ui.checkbox(
            &mut self.options.preview_particles,
            crate::i18n::tr("Hit effects"),
        );
        ui.checkbox(
            &mut self.options.preview_double_hint,
            crate::i18n::tr("Simultaneous note highlight"),
        );
        ui.checkbox(
            &mut self.options.preview_accuracy,
            crate::i18n::tr("Show accuracy"),
        );
        let mut override_dim = self.options.preview_dim.is_some();
        if ui
            .checkbox(
                &mut override_dim,
                crate::i18n::tr("Override background dim"),
            )
            .changed()
        {
            self.options.preview_dim = override_dim.then_some(0.6);
        }
        if let Some(dim) = &mut self.options.preview_dim {
            ui.add(egui::Slider::new(dim, 0. ..=1.).text(crate::i18n::tr("Background dim")));
        }
        ui.separator();
        self.skins_ui(ui);
        ui.separator();
        self.theme_settings(ui);
        ui.separator();
        ui.strong(crate::i18n::tr("Grid and gestures"));
        ui.checkbox(
            &mut self.options.numeric_drag.enabled,
            crate::i18n::tr("Swipe numeric fields"),
        );
        ui.label(crate::i18n::tr("Numeric swipe sensitivity"));
        ui.add(
            crate::numeric::Value::new(&mut self.options.numeric_drag.sensitivity)
                .speed(0.01)
                .clamp_range(0.01..=100.)
                .unconfigured(),
        );
        ui.checkbox(
            &mut self.options.numeric_drag.snap,
            crate::i18n::tr("Snap numeric swipes to a step"),
        );
        ui.label(crate::i18n::tr("Numeric swipe step"));
        ui.add(
            crate::numeric::Value::new(&mut self.options.numeric_drag.step)
                .speed(0.05)
                .clamp_range(0.000001..=1e9)
                .unconfigured(),
        );
        ui.small(crate::i18n::tr("Step 5 gives values ending in 0 or 5. Use any positive step, such as 0.05. Typed numbers and formulas keep their precision."));
        ui.separator();
        ui.checkbox(
            &mut self.options.snap_time,
            crate::i18n::tr("Snap time to grid"),
        );
        ui.checkbox(
            &mut self.options.snap_x,
            crate::i18n::tr("Snap note X to lanes"),
        );
        ui.add(
            egui::Slider::new(&mut self.options.scroll_speed, 0.25..=3.)
                .text(crate::i18n::tr("Scroll speed")),
        );
        ui.add(
            egui::Slider::new(&mut self.options.grid_note_width, 24. ..=90.)
                .text(crate::i18n::tr("Grid note width")),
        );
        ui.checkbox(
            &mut self.options.minor_grid,
            crate::i18n::tr("Subdivision lines"),
        );
        ui.checkbox(
            &mut self.options.beat_labels,
            crate::i18n::tr("Beat labels"),
        );
        ui.separator();
        ui.strong(crate::i18n::tr("Audio"));
        self.editor_sounds_ui(ui);
        ui.collapsing(crate::i18n::tr("Audio guide"), |ui| {
            if self.audio_guide.controls(ui, &mut self.options.audio_guide) {
                self.open_audio_notes();
            }
        });
        for (label, value) in [
            ("Music volume", &mut self.transport.music_volume),
            ("Hitsound volume", &mut self.transport.hitsound_volume),
        ] {
            ui.add(
                egui::Slider::new(value, 0. ..=1.)
                    .text(crate::i18n::tr(label))
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
        ui.add(
            egui::Slider::new(&mut self.options.hitsound_delay_ms, -200. ..=200.)
                .text(crate::i18n::tr("Tap / Flick delay (ms)")),
        );
        ui.add(
            egui::Slider::new(&mut self.options.drag_delay_ms, -200. ..=200.)
                .text(crate::i18n::tr("Drag delay (ms)")),
        );
        ui.separator();
        ui.strong(crate::i18n::tr("Editor"));
        if ui.button(crate::i18n::tr("Keyboard shortcuts…")).clicked() {
            self.advanced.shortcuts_window = true;
        }
        ui.checkbox(
            &mut self.options.history_jump,
            crate::i18n::tr("Jump to changed objects on Undo / Redo"),
        );
        ui.checkbox(
            &mut self.options.auto_connect_events,
            crate::i18n::tr("Connect following event after value edits"),
        );
        ui.checkbox(
            &mut self.options.auto_copy_selection,
            crate::i18n::tr("Copy multi-selection automatically"),
        );
        egui::ComboBox::from_id_source("editor-ui-scale")
            .selected_text({
                let __i18n_arg_0 = &(self.options.ui_scale * 100.);
                crate::i18n::format("Interface size: {0}%", &[format!("{:.0}", __i18n_arg_0)])
            })
            .touch_show_ui(ui, |ui| {
                for scale in [0.75, 0.9, 1., 1.1, 1.25, 1.5] {
                    ui.selectable_value(
                        &mut self.options.ui_scale,
                        scale,
                        format!("{:.0}%", scale * 100.),
                    );
                }
            });
        ui.add(
            egui::Slider::new(&mut self.options.recovery_seconds, 1..=60)
                .text(crate::i18n::tr("Recovery interval (s)")),
        );
        ui.small(crate::i18n::tr(
            "Preferences are saved automatically, separately from chart data.",
        ));
        if ui
            .button(crate::i18n::tr("Reset editor preferences"))
            .clicked()
        {
            self.apply_preferences(&Default::default());
        }
        ui.separator();
        self.about_ui(ui);
    }
}
