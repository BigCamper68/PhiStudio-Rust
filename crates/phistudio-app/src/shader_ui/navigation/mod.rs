//! Paired timelines select effects and uniforms; the inspector exposes properties,
//! animation and the independent FX tempo with explicit return navigation.
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
    pub(super) fn shader_left_navigation(&mut self, ui: &mut egui::Ui) {
        ui.set_enabled(!self.modal_open());
        ui.spacing_mut().interact_size.y = 24.;
        crate::timeline::navigation_row(ui, "shader-effect-navigation", |ui| {
            let mut x = self.shaders.offsets[0] + 1.;
            let mut changed = false;
            if ui.add_sized([24., 24.], egui::Button::new("<")).clicked() {
                x -= 1.;
                changed = true;
            }
            changed |= ui
                .add_sized(
                    [64., 24.],
                    crate::numeric::Value::new(&mut x)
                        .speed(0.05)
                        .prefix("X ")
                        .max_decimals(2),
                )
                .changed();
            if ui.add_sized([24., 24.], egui::Button::new(">")).clicked() {
                x += 1.;
                changed = true;
            }
            if changed {
                self.shaders.offsets[0] = x - 1.;
                self.drag = None;
                self.grid_input.cancel();
            }
        });
        crate::timeline::navigation_row(ui, "shader-preset-navigation", |ui| {
            egui::ComboBox::from_id_source("shader-placement-preset")
                .width(110.)
                .selected_text(crate::i18n::format(
                    "New: {0}",
                    &[PRESETS[self.shaders.preset].to_owned()],
                ))
                .touch_show_ui(ui, |ui| {
                    for (i, preset) in PRESETS.iter().enumerate() {
                        ui.selectable_value(&mut self.shaders.preset, i, *preset);
                    }
                });
        });
    }
    pub(super) fn shader_page_selector(&mut self, ui: &mut egui::Ui) {
        let mut page = self.shaders.page;
        let label = |page| match page {
            ShaderPage::Effect => "Effect",
            ShaderPage::Parameters => "Parameters",
            ShaderPage::Animation => "Animation",
            ShaderPage::Tempo => "FX BPM",
        };
        egui::ComboBox::from_id_source("shader-inspector-page")
            .width(90.)
            .selected_text(crate::i18n::tr(label(page)))
            .touch_show_ui(ui, |ui| {
                for value in [
                    ShaderPage::Effect,
                    ShaderPage::Parameters,
                    ShaderPage::Animation,
                    ShaderPage::Tempo,
                ] {
                    ui.add_enabled_ui(
                        self.shaders.selected.is_some()
                            || matches!(value, ShaderPage::Effect | ShaderPage::Tempo),
                        |ui| {
                            ui.selectable_value(&mut page, value, crate::i18n::tr(label(value)));
                        },
                    );
                }
            });
        if page != self.shaders.page {
            self.shader_navigate(page, None);
        }
    }
    pub(crate) fn shader_navigation(&mut self, ui: &mut egui::Ui) {
        self.refresh_shaders();
        ui.set_enabled(!self.modal_open());
        ui.spacing_mut().interact_size.y = 24.;
        ui.spacing_mut().button_padding.y = 3.;
        ui.spacing_mut().item_spacing.y = 4.;
        crate::timeline::navigation_row(ui, "shader-main-navigation-scroll", |ui| {
            self.workspace_selector(ui);
            if self.advanced.right_mode != RightMode::Shaders {
                return;
            }
            let mut selected = self.shaders.selected;
            let label = selected
                .and_then(|i| {
                    self.shaders
                        .effects
                        .get(i)
                        .map(|e| format!("#{} · {}", i + 1, e.shader))
                })
                .unwrap_or_else(|| crate::i18n::tr("Select effect"));
            let font = egui::TextStyle::Button.resolve(ui.style());
            let mut job =
                egui::text::LayoutJob::simple(label, font, ui.visuals().text_color(), 74.);
            job.wrap.max_rows = 1;
            let label = ui.fonts(|fonts| fonts.layout_job(job));
            let mut jump = false;
            egui::ComboBox::from_id_source("shader-effect-list")
                .width(104.)
                .selected_text(label)
                .touch_show_ui(ui, |ui| {
                    for (i, effect) in self.shaders.effects.iter().enumerate() {
                        jump |= ui
                            .selectable_value(
                                &mut selected,
                                Some(i),
                                format!(
                                    "#{} · {} · {}–{}",
                                    i + 1,
                                    effect.shader,
                                    effect.start,
                                    effect.end
                                ),
                            )
                            .clicked();
                    }
                });
            if jump && let Some(index) = selected {
                self.jump_shader(index, crate::grid_input::geometry(self.grid_area, self));
            }
        });
        crate::timeline::navigation_row(ui, "shader-parameter-navigation", |ui| {
            if !self.shaders.parameters.is_empty() {
                let mut name = self
                    .shaders
                    .track
                    .clone()
                    .unwrap_or_else(|| self.shaders.parameters[0].name.clone());
                let mut chosen = false;
                egui::ComboBox::from_id_source("shader-parameter-choice")
                    .width(88.)
                    .selected_text(&name)
                    .touch_show_ui(ui, |ui| {
                        for parameter in &self.shaders.parameters {
                            chosen |= ui
                                .selectable_value(
                                    &mut name,
                                    parameter.name.clone(),
                                    &parameter.name,
                                )
                                .clicked();
                        }
                    });
                if chosen {
                    let page = if self.shaders.page == ShaderPage::Animation {
                        ShaderPage::Animation
                    } else {
                        ShaderPage::Parameters
                    };
                    self.shader_navigate(page, Some(name.clone()));
                    let grid = crate::grid_input::geometry(self.grid_area, self).events();
                    if let Some(index) = self.shaders.parameters.iter().position(|p| p.name == name)
                    {
                        let visible =
                            f64::from(grid.rect.width() / layout::width(self, grid, true));
                        self.shaders.offsets[1] = (index as f64 + 0.5 - visible / 2.)
                            .clamp(0., layout::max_offset(self, grid));
                    }
                }
            }
        });
    }
}
