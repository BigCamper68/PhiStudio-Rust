use super::*;

impl Workbench {
    pub(super) fn shader_animation_ui(&mut self, ui: &mut egui::Ui) {
        let Some(mut a) = self.shaders.animation.take() else {
            return;
        };
        let Some(d) = &mut self.shaders.draft else {
            return;
        };
        let Some(rows) = d.raw["vars"][&a.name].as_array_mut() else {
            return;
        };
        if a.index >= rows.len() {
            return;
        }

        let mut next = a.index;
        egui::ComboBox::from_id_source("shader-keyframe")
            .selected_text({
                let __i18n_arg_0 = &(a.index + 1);
                let __i18n_arg_1 = &(rows.len());
                crate::i18n::format(
                    "Keyframe {0} of {1}",
                    &[__i18n_arg_0.to_string(), __i18n_arg_1.to_string()],
                )
            })
            .touch_show_ui(ui, |ui| {
                for i in 0..rows.len() {
                    ui.selectable_value(&mut next, i, {
                        let __i18n_arg_0 = &(i + 1);
                        crate::i18n::format("Keyframe {0}", &[__i18n_arg_0.to_string()])
                    });
                }
            });
        ui.push_id("shader-keyframe-fields", |ui| {
            let form = crate::form::Form::new(
                ui,
                &[
                    "Start FX beat",
                    "End FX beat",
                    "From",
                    "From color",
                    "To",
                    "To color",
                ],
            );
            form.field(ui, "Start FX beat", |ui| {
                ui.text_edit_singleline(&mut a.start);
            });
            form.field(ui, "End FX beat", |ui| {
                ui.text_edit_singleline(&mut a.end);
            });
            form.field(ui, "From", |ui| uniform_widget(ui, &mut a.raw["start"]));
            if let Some(rgba) = rgba_value(&a.raw["start"]) {
                form.field(ui, "From color", |ui| {
                    if crate::color_ui::swatch(ui, [rgba[0], rgba[1], rgba[2]], "Color palette…")
                        .clicked()
                    {
                        self.open_shader_color(a.name.clone(), Some((a.index, false)), rgba);
                    }
                });
            }
            form.field(ui, "To", |ui| uniform_widget(ui, &mut a.raw["end"]));
            if let Some(rgba) = rgba_value(&a.raw["end"]) {
                form.field(ui, "To color", |ui| {
                    if crate::color_ui::swatch(ui, [rgba[0], rgba[1], rgba[2]], "Color palette…")
                        .clicked()
                    {
                        self.open_shader_color(a.name.clone(), Some((a.index, true)), rgba);
                    }
                });
            }
            shader_easing_fields(ui, &mut a.curve);
        });
        draw_curve(ui, EventType::MoveX, &a.curve);
        let mut add = false;
        let mut remove = false;
        let mut apply = false;
        ui.horizontal_wrapped(|ui| {
            add = ui.button(crate::i18n::tr("Append keyframe")).clicked();
            remove = ui.button(crate::i18n::tr("Delete keyframe")).clicked();
            apply = ui.button(crate::i18n::tr("Apply keyframe")).clicked();
        });
        let name = a.name.clone();
        let index = a.index;
        self.shaders.animation = Some(a);
        if (next != index || add || apply) && !self.apply_shader_animation() {
            return;
        }
        if let Some(d) = &mut self.shaders.draft {
            if add {
                let rows = d.raw["vars"][&name].as_array_mut().unwrap();
                let last = &rows[index];
                let mut value = last.clone();
                let start: Beat = serde_json::from_value(last["endTime"].clone()).unwrap();
                value["startTime"] = json!(start);
                value["endTime"] = json!(start.checked_add(Beat::ONE).unwrap_or(start));
                value["start"] = last["end"].clone();
                rows.insert(index + 1, value);
                next = index + 1;
            }
            if remove {
                let rows = d.raw["vars"][&name].as_array_mut().unwrap();
                let value = rows.remove(index)["start"].clone();
                if rows.is_empty() {
                    d.raw["vars"][&name] = value;
                    self.shaders.animation = None;
                    if self.flush_shader_draft() {
                        self.refresh_shaders();
                    }
                    return;
                }
                next = index.min(rows.len() - 1);
            }
            if next != index || add || remove {
                self.shaders.animation = animation_draft(&d.raw, &name, next);
            }
        }
        if (apply || add || remove) && self.flush_shader_draft() {
            self.refresh_shaders();
        }
    }

    pub(super) fn apply_shader_animation(&mut self) -> bool {
        let Some(a) = &mut self.shaders.animation else {
            return true;
        };
        let result = a
            .start
            .parse::<Beat>()
            .and_then(|s| a.end.parse::<Beat>().map(|e| (s, e)));
        let (start, end) = match result {
            Ok(v) => v,
            Err(e) => {
                self.error = Some(e.to_string());
                return false;
            }
        };
        if end <= start || a.curve.easing_left >= a.curve.easing_right {
            self.error = Some("Use a positive keyframe interval and easing window".into());
            return false;
        }
        for (key, beat) in [("startTime", start), ("endTime", end)] {
            if serde_json::from_value::<Beat>(a.raw[key].clone()).ok() != Some(beat) {
                a.raw[key] = json!(beat);
            }
        }
        if a.raw["easingType"].as_i64().unwrap_or(1) != a.curve.easing_type as i64 {
            a.raw["easingType"] = json!(a.curve.easing_type);
        }
        for (key, value, default) in [
            ("easingLeft", a.curve.easing_left, 0.),
            ("easingRight", a.curve.easing_right, 1.),
        ] {
            if a.raw[key].as_f64().unwrap_or(default) != value {
                a.raw[key] = json!(value);
            }
        }
        if let Some(d) = &mut self.shaders.draft
            && let Some(rows) = d.raw["vars"][&a.name].as_array_mut()
            && let Some(row) = rows.get_mut(a.index)
        {
            *row = a.raw.clone();
        }
        true
    }
}
