use super::*;

impl Workbench {
    pub(crate) fn shader_properties(&mut self, ui: &mut egui::Ui) {
        self.refresh_shaders();
        if let Some(e) = &self.shaders.error {
            ui.colored_label(flick(), crate::i18n::tr(e));
            return;
        }
        match self.shaders.page {
            ShaderPage::Parameters => {
                self.shader_parameter_ui(ui);
                return;
            }
            ShaderPage::Animation => {
                self.shader_animation_page(ui);
                return;
            }
            ShaderPage::Tempo => {
                self.shader_tempo_ui(ui);
                return;
            }
            ShaderPage::Effect => {}
        }
        ui.strong(crate::i18n::tr("Shader properties"));
        if self.shaders.selected.is_none() {
            ui.label(crate::i18n::tr(
                "Select + Shader, choose a preset above the grid, then draw its interval upwards.",
            ));
            return;
        }
        self.apply_shader_animation();
        let Some(mut d) = self.shaders.draft.take() else {
            ui.label(crate::i18n::tr(
                "Select an effect on the timeline or in the effect list above.",
            ));
            return;
        };
        let mut apply = false;
        ui.label({
            let __i18n_arg_0 = &(d.index + 1);
            let __i18n_arg_1 = &(d.raw["shader"].as_str().unwrap_or(""));
            crate::i18n::format(
                "Effect {0} · {1}",
                &[__i18n_arg_0.to_string(), __i18n_arg_1.to_string()],
            )
        });
        ui.horizontal_wrapped(|ui| {
            for (label, target) in [
                ("Earlier", d.index.checked_sub(1)),
                (
                    "Later",
                    (d.index + 1 < self.shaders.effects.len()).then_some(d.index + 1),
                ),
            ] {
                if ui
                    .add_enabled(target.is_some(), egui::Button::new(crate::i18n::tr(label)))
                    .clicked()
                {
                    // Staged values are applied before changing pass order.
                    self.shaders.draft = Some(EffectDraft {
                        index: d.index,
                        raw: d.raw.clone(),
                        start: d.start.clone(),
                        end: d.end.clone(),
                    });
                    if self.flush_shader_draft()
                        && self.shader_command(ShaderAction::Move {
                            from: d.index,
                            to: target.unwrap(),
                        })
                    {
                        self.shaders.selected = target;
                        self.refresh_shaders();
                        if let Some(next) = self.shaders.draft.take() {
                            d = next;
                        }
                    }
                }
            }
            if ui.button(crate::i18n::tr("Copy")).clicked() {
                self.shaders.clipboard = Some(d.raw.clone());
            }
            if ui
                .add_enabled(
                    self.shaders.clipboard.is_some(),
                    egui::Button::new(crate::i18n::tr("Paste")),
                )
                .clicked()
            {
                let index = self.shaders.effects.len();
                if self.shader_command(ShaderAction::Insert {
                    index,
                    effect: self.shaders.clipboard.clone().unwrap(),
                }) {
                    self.status = "Pasted shader effect".into();
                }
            }
        });
        ui.push_id("shader-effect-fields", |ui| {
            let form = crate::form::Form::new(ui, &["Start FX beat", "End FX beat", "Pass"]);
            form.field(ui, "Start FX beat", |ui| {
                ui.text_edit_singleline(&mut d.start);
            });
            form.field(ui, "End FX beat", |ui| {
                ui.text_edit_singleline(&mut d.end);
            });
            form.field(ui, "Pass", |ui| {
                let mut global = d.raw["global"].as_bool().unwrap_or(false);
                if ui
                    .checkbox(&mut global, crate::i18n::tr("Global"))
                    .changed()
                {
                    d.raw["global"] = json!(global);
                }
            });
        });
        ui.small(crate::i18n::tr("Local effects run before the HUD; global effects include it. Order is significant within each pass."));
        let mut shader = d.raw["shader"].as_str().unwrap_or("grayscale").to_owned();
        egui::ComboBox::from_id_source("effect-shader")
            .selected_text(&shader)
            .touch_show_ui(ui, |ui| {
                for preset in PRESETS {
                    ui.selectable_value(&mut shader, preset.into(), preset);
                }
                if let Ok(sources) = shader_sources(self.editor.document()) {
                    for name in sources.keys() {
                        ui.selectable_value(&mut shader, format!("/{name}"), name);
                    }
                }
                for e in &self.shaders.effects {
                    if e.shader.starts_with('/') {
                        ui.selectable_value(&mut shader, e.shader.clone(), &e.shader);
                    }
                }
            });
        if d.raw["shader"].as_str() != Some(&shader) {
            d.raw["shader"] = json!(shader);
            apply = true;
        }
        ui.separator();
        let mut open_parameters = false;
        if ui.button(crate::i18n::tr("Edit parameters")).clicked() {
            open_parameters = true;
        }
        if ui.button(crate::i18n::tr("Jump to start")).clicked()
            && let Ok(start) = d.start.parse::<Beat>()
        {
            self.set_grid_beat(start.as_f64());
        }
        self.shaders.draft = Some(d);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(crate::i18n::tr("Apply effect")).clicked() {
                apply = true;
            }
            if ui.button(crate::i18n::tr("Reset edits")).clicked() {
                self.shaders.key = None;
                self.refresh_shaders();
            }
            if ui.button(crate::i18n::tr("Source ↗")).clicked() {
                self.shaders.code_view = true;
            }
        });
        if apply && self.flush_shader_draft() {
            self.refresh_shaders();
        }
        if open_parameters {
            self.shader_navigate(ShaderPage::Parameters, None);
        }
        ui.small(crate::i18n::tr(
            "Apply commits one Undo step. Switching effect or workspace also applies valid edits.",
        ));
    }

    pub(crate) fn apply_shader_color(
        &mut self,
        name: &str,
        keyframe: Option<(usize, bool)>,
        rgba: [u8; 4],
    ) {
        if let Some((index, end)) = keyframe {
            if let Some(a) = &mut self.shaders.animation
                && a.name == name
                && a.index == index
            {
                a.raw[if end { "end" } else { "start" }] = json!(rgba);
            }
        } else if let Some(d) = &mut self.shaders.draft {
            d.raw["vars"][name] = json!(rgba);
        }
        if self.flush_shader_draft() {
            self.refresh_shaders();
        }
    }
}
