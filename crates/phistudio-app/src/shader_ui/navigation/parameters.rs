use super::*;

impl Workbench {
    pub(in super::super) fn shader_parameter_ui(&mut self, ui: &mut egui::Ui) {
        let Some(parameter) = self.shader_parameter() else {
            ui.label(crate::i18n::tr("This shader has no editable parameters."));
            return;
        };
        let Some(d) = &self.shaders.draft else {
            return;
        };
        let name = parameter.name.clone();
        let mut value = d.raw["vars"]
            .get(&name)
            .cloned()
            .unwrap_or(parameter.default.clone());
        ui.strong(format!(
            "{} · {}",
            name,
            crate::i18n::tr(
                match uniform_type(&parameter.default).unwrap_or(UniformKind::Float) {
                    UniformKind::Float => "Float",
                    UniformKind::Vec2 => "X / Y",
                    UniformKind::Color => "RGBA",
                }
            )
        ));
        if is_animated(&value) {
            ui.label({
                let __i18n_arg_0 = &(value.as_array().unwrap().len());
                crate::i18n::format("Animated · {0} keyframes", &[__i18n_arg_0.to_string()])
            });
            if ui.button(crate::i18n::tr("Edit animation")).clicked() {
                self.shader_navigate(ShaderPage::Animation, None);
                return;
            }
            if ui.button(crate::i18n::tr("Use constant")).clicked() {
                if let Some(d) = &mut self.shaders.draft {
                    d.raw["vars"][&name] = value[0]["start"].clone();
                }
                self.shaders.animation = None;
                if self.flush_shader_draft() {
                    self.refresh_shaders();
                }
            }
        } else {
            ui.label(crate::i18n::tr("Constant value"));
            if uniform_type(&value).is_none() {
                value = parameter.default.clone();
            }
            if uniform_widget(ui, &mut value)
                && let Some(d) = &mut self.shaders.draft
            {
                d.raw["vars"][&name] = value.clone();
            }
            if let Some(rgba) = rgba_value(&value)
                && crate::color_ui::swatch(ui, [rgba[0], rgba[1], rgba[2]], "Choose color…")
                    .clicked()
            {
                self.open_shader_color(name.clone(), None, rgba);
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button(crate::i18n::tr("Apply value")).clicked() && self.flush_shader_draft()
                {
                    self.refresh_shaders();
                }
                if ui.button(crate::i18n::tr("Animate")).clicked() {
                    self.shader_create_animation();
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Reset default")).clicked() {
                self.shaders.animation = None;
                if let Some(d) = &mut self.shaders.draft
                    && let Some(vars) = d.raw["vars"].as_object_mut()
                {
                    vars.shift_remove(&name);
                }
                if self.flush_shader_draft() {
                    self.refresh_shaders();
                }
            }
            if ui.button(crate::i18n::tr("Discard edits")).clicked() {
                self.shaders.key = None;
                self.shaders.animation = None;
                self.refresh_shaders();
                self.error = None;
            }
        });
        ui.small(crate::i18n::tr(
            "Apply changes the preview. Animate adds time-based From / To values.",
        ));
        self.shader_add_uniform_ui(ui);
    }

    pub(in super::super) fn shader_create_animation(&mut self) {
        let Some(parameter) = self.shader_parameter() else {
            return;
        };
        if let Some(d) = &mut self.shaders.draft {
            let name = &parameter.name;
            if !is_animated(&d.raw["vars"][name]) {
                let value = d.raw["vars"]
                    .get(name)
                    .filter(|v| uniform_type(v).is_some())
                    .cloned()
                    .unwrap_or(parameter.default);
                d.raw["vars"][name] = json!([{"startTime":d.raw["start"],"endTime":d.raw["end"],"start":value,"end":value,"easingType":1}]);
            }
        }
        self.shader_navigate(ShaderPage::Animation, None);
        if let Some(a) = &self.shaders.animation
            && let Ok(start) = a.start.parse::<Beat>()
        {
            self.set_grid_beat(start.as_f64());
        }
    }

    pub(in super::super) fn shader_animation_page(&mut self, ui: &mut egui::Ui) {
        let Some(parameter) = self.shader_parameter() else {
            ui.label(crate::i18n::tr("This shader has no editable parameters."));
            return;
        };
        ui.strong({
            let __i18n_arg_0 = &(parameter.name);
            crate::i18n::format("{0} · Animation", std::slice::from_ref(__i18n_arg_0))
        });
        if self.shaders.animation.is_none() {
            let animated = self
                .shaders
                .draft
                .as_ref()
                .is_some_and(|d| is_animated(&d.raw["vars"][&parameter.name]));
            if animated {
                ui.label(crate::i18n::tr("Select a keyframe on the timeline."));
                if ui
                    .button(crate::i18n::tr("Select first keyframe"))
                    .clicked()
                {
                    self.shaders.animation = self
                        .shaders
                        .draft
                        .as_ref()
                        .and_then(|d| animation_draft(&d.raw, &parameter.name, 0));
                }
            } else {
                ui.label(crate::i18n::tr("No animation yet. Draw with + Keyframe, or create an interval for the whole effect."));
                if ui.button(crate::i18n::tr("Create animation")).clicked() {
                    self.shader_create_animation();
                }
            }
        } else {
            self.shader_animation_ui(ui);
            ui.horizontal_wrapped(|ui| {
                if ui.button(crate::i18n::tr("Use constant")).clicked() {
                    if let Some(a) = self.shaders.animation.take()
                        && let Some(d) = &mut self.shaders.draft
                    {
                        d.raw["vars"][&a.name] = a.raw["start"].clone();
                    }
                    self.shader_navigate(ShaderPage::Parameters, None);
                }
                if ui.button(crate::i18n::tr("Discard edits")).clicked() {
                    self.shaders.key = None;
                    self.shaders.animation = None;
                    self.refresh_shaders();
                    self.error = None;
                }
            });
        }
    }

    pub(in super::super) fn shader_add_uniform_ui(&mut self, ui: &mut egui::Ui) {
        ui.collapsing(crate::i18n::tr("Add uniform"), |ui| {
            ui.text_edit_singleline(&mut self.shaders.new_uniform);
            egui::ComboBox::from_id_source("uniform-kind")
                .selected_text(crate::i18n::tr(
                    ["Float", "Vec2", "RGBA"][self.shaders.uniform_kind],
                ))
                .touch_show_ui(ui, |ui| {
                    for (i, label) in ["Float", "Vec2", "RGBA"].iter().enumerate() {
                        ui.selectable_value(
                            &mut self.shaders.uniform_kind,
                            i,
                            crate::i18n::tr(*label),
                        );
                    }
                });
            if ui.button(crate::i18n::tr("Add parameter")).clicked() {
                let name = self.shaders.new_uniform.trim().to_owned();
                let value = match self.shaders.uniform_kind {
                    0 => json!(0.),
                    1 => json!([0., 0.]),
                    _ => json!([255, 255, 255, 255]),
                };
                if let Some(d) = &mut self.shaders.draft {
                    d.raw["vars"][&name] = value;
                }
                if self.flush_shader_draft() {
                    self.refresh_shaders();
                    self.shaders.track = Some(name);
                }
            }
        });
    }
}
