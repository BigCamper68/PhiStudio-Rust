//! Effect → parameter → animation navigation follows the Java shader editor.
//! Navigation stays outside the scrolling inspector and always has a return path.
mod parameters;
mod tempo;
use super::*;

impl Workbench {
    pub(crate) fn grid_beat(&self) -> f64 {
        if self.advanced.right_mode == RightMode::Shaders {
            self.shaders.tempo.as_ref().map_or(self.beat, |tempo| {
                tempo.beat_at(self.editor.document().tempo().seconds_at(self.beat))
            })
        } else {
            self.beat
        }
    }
    pub(crate) fn grid_bpm(&self) -> f64 {
        if self.advanced.right_mode == RightMode::Shaders {
            self.shaders.tempo.as_ref().map_or_else(
                || self.editor.document().tempo().bpm_at(self.beat),
                |t| t.bpm_at(self.grid_beat()),
            )
        } else {
            self.editor.document().tempo().bpm_at(self.beat)
        }
    }
    pub(crate) fn set_grid_beat(&mut self, beat: f64) {
        self.beat = if self.advanced.right_mode == RightMode::Shaders {
            self.shaders.tempo.as_ref().map_or(beat, |tempo| {
                self.editor
                    .document()
                    .tempo()
                    .beat_at(tempo.seconds_at(beat))
            })
        } else {
            beat
        }
        .max(0.);
    }
    pub(crate) fn shader_placement_label(&self) -> &'static str {
        if self.shaders.animation_track().is_some() {
            "+ Keyframe"
        } else {
            "+ Shader"
        }
    }
    fn shader_parameter(&self) -> Option<ShaderParameter> {
        self.shaders
            .parameters
            .iter()
            .find(|p| Some(&p.name) == self.shaders.track.as_ref())
            .cloned()
    }
    pub(super) fn shader_navigate(&mut self, page: ShaderPage, parameter: Option<String>) {
        // Back must remain usable even with an unfinished numeric field.
        // Discard only invalid, unapplied form data; history is never rewound.
        if !self.flush_shader_draft() {
            self.shaders.key = None;
            self.shaders.animation = None;
            self.error = None;
            self.status = "Discarded invalid, unapplied shader edits".into();
        }
        self.refresh_shaders();
        self.shaders.page = page;
        if let Some(parameter) = parameter {
            self.shaders.track = Some(parameter);
        }
        if matches!(page, ShaderPage::Parameters | ShaderPage::Animation)
            && self.shader_parameter().is_none()
        {
            self.shaders.track = self.shaders.parameters.first().map(|p| p.name.clone());
        }
        self.shaders.animation = if page == ShaderPage::Animation {
            self.shaders.track.as_ref().and_then(|name| {
                self.shaders
                    .draft
                    .as_ref()
                    .and_then(|d| animation_draft(&d.raw, name, 0))
            })
        } else {
            None
        };
        if page == ShaderPage::Tempo {
            self.shaders.tempo_rows = None;
        }
        self.shaders.code_view = false;
        self.drag = None;
        self.grid_input.cancel();
        self.set_tool(crate::workbench::Tool::Select);
        self.show_context = true;
        self.properties_scroll = true;
    }
    pub(crate) fn shader_back(&mut self) -> bool {
        if self.advanced.right_mode != RightMode::Shaders {
            return false;
        }
        if self.shaders.code_view {
            self.shaders.code_view = false;
            return true;
        }
        match self.shaders.page {
            ShaderPage::Animation => self.shader_navigate(ShaderPage::Parameters, None),
            ShaderPage::Parameters | ShaderPage::Tempo => {
                self.shader_navigate(ShaderPage::Effect, None)
            }
            ShaderPage::Effect => {
                self.shader_navigate(ShaderPage::Effect, None);
                self.advanced.right_mode = RightMode::Events;
                self.shaders.track = None;
            }
        }
        true
    }
    pub(crate) fn shader_navigation(&mut self, ui: &mut egui::Ui) {
        self.refresh_shaders();
        ui.set_enabled(!self.modal_open());
        ui.spacing_mut().interact_size.y = 24.;
        ui.spacing_mut().button_padding.y = 3.;
        ui.spacing_mut().item_spacing.y = 4.;
        ui.spacing_mut().scroll_bar_width = 3.;
        ui.spacing_mut().scroll_bar_inner_margin = 1.;
        crate::timeline::navigation_row(ui, "shader-main-navigation-scroll", |ui| {
            self.workspace_selector(ui);
            if self.advanced.right_mode != RightMode::Shaders {
                return;
            }
            if self.shaders.page != ShaderPage::Effect
                && ui.button(crate::i18n::tr("Back to effect")).clicked()
            {
                self.shader_navigate(ShaderPage::Effect, None);
            }
            let mut selected = self.shaders.selected;
            let label = selected
                .and_then(|i| {
                    self.shaders
                        .effects
                        .get(i)
                        .map(|e| format!("#{} · {}", i + 1, e.shader))
                })
                .unwrap_or_else(|| "Select effect".into());
            let placing = self.tool == crate::workbench::Tool::Event
                && self.shaders.animation_track().is_none();
            egui::ComboBox::from_id_source("shader-effect-list")
                .width((ui.available_width() - if placing { 120. } else { 0. }).clamp(90., 260.))
                .selected_text(crate::i18n::tr(label))
                .touch_show_ui(ui, |ui| {
                    for (i, effect) in self.shaders.effects.iter().enumerate() {
                        ui.selectable_value(
                            &mut selected,
                            Some(i),
                            format!(
                                "#{} · {} · {}–{}",
                                i + 1,
                                effect.shader,
                                effect.start,
                                effect.end
                            ),
                        );
                    }
                });
            if selected != self.shaders.selected
                && let Some(index) = selected
            {
                self.select_shader(index);
            }
            if placing {
                egui::ComboBox::from_id_source("shader-placement-preset")
                    .width(100.)
                    .selected_text({
                        let __i18n_arg_0 = &(PRESETS[self.shaders.preset]);
                        crate::i18n::format("New: {0}", &[__i18n_arg_0.to_string()])
                    })
                    .touch_show_ui(ui, |ui| {
                        for (i, preset) in PRESETS.iter().enumerate() {
                            ui.selectable_value(&mut self.shaders.preset, i, *preset);
                        }
                    });
            }
        });
        egui::ScrollArea::horizontal()
            .id_source("shader-pages-scroll")
            .touch_show_x(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut page = self.shaders.page;
                    let mut activated = false;
                    for (value, label) in [
                        (ShaderPage::Effect, "Effect"),
                        (ShaderPage::Parameters, "Parameters"),
                        (ShaderPage::Animation, "Animation"),
                        (ShaderPage::Tempo, "FX BPM"),
                    ] {
                        if ui
                            .add_enabled(
                                self.shaders.selected.is_some()
                                    || matches!(value, ShaderPage::Effect | ShaderPage::Tempo),
                                egui::SelectableLabel::new(page == value, crate::i18n::tr(label)),
                            )
                            .clicked()
                        {
                            page = value;
                            activated = true;
                        }
                    }
                    if page != self.shaders.page {
                        self.shader_navigate(page, None);
                    } else if activated {
                        self.show_context = true;
                        self.properties_scroll = true;
                    }
                    if matches!(
                        self.shaders.page,
                        ShaderPage::Parameters | ShaderPage::Animation
                    ) && !self.shaders.parameters.is_empty()
                    {
                        let mut name = self
                            .shaders
                            .track
                            .clone()
                            .unwrap_or_else(|| self.shaders.parameters[0].name.clone());
                        egui::ComboBox::from_id_source("shader-parameter-choice")
                            .width(ui.available_width().clamp(60., 150.))
                            .selected_text(&name)
                            .touch_show_ui(ui, |ui| {
                                for parameter in &self.shaders.parameters {
                                    ui.selectable_value(
                                        &mut name,
                                        parameter.name.clone(),
                                        &parameter.name,
                                    );
                                }
                            });
                        if self.shaders.track.as_ref() != Some(&name) {
                            self.shader_navigate(self.shaders.page, Some(name));
                        }
                    }
                });
            });
    }
}
