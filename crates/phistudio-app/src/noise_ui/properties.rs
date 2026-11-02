use super::*;
const NATIVE_EASINGS: [&str; 15] = [
    "Linear",
    "In Quad",
    "Out Quad",
    "In Out Quad",
    "In Cubic",
    "Out Cubic",
    "In Out Cubic",
    "In Quart",
    "Out Quart",
    "In Out Quart",
    "In Quint",
    "Out Quint",
    "In Out Quint",
    "Hold value",
    "Jump to value",
];
fn number(
    ui: &mut egui::Ui,
    form: &crate::form::Form,
    raw: &mut Value,
    path: &[&str],
    label: &str,
    speed: f64,
) {
    let mut value = &*raw;
    for key in path {
        value = &value[*key];
    }
    let mut value = value.as_f64().unwrap_or(0.);
    let changed = form
        .field(ui, label, |ui| {
            ui.add(
                crate::numeric::Value::new(&mut value)
                    .speed(speed)
                    .max_decimals(6),
            )
        })
        .changed();
    if changed {
        let mut target = raw;
        for key in &path[..path.len() - 1] {
            target = &mut target[*key];
        }
        target[path[path.len() - 1]] = json!(value);
    }
}
impl Workbench {
    pub(crate) fn noise_properties(&mut self, ui: &mut egui::Ui) {
        self.refresh_noise();
        ui.strong(crate::i18n::tr(if self.noise.animation {
            "Noise Field animation"
        } else {
            "Noise Field properties"
        }));
        if let Some(error) = &self.noise.error {
            ui.colored_label(flick(), crate::i18n::tr(error));
        }
        let Some(index) = self.noise.selected else {
            ui.label(crate::i18n::tr(
                "Choose + Field and draw its lifetime upwards, or select an existing field.",
            ));
            return;
        };
        if self.noise.animation {
            self.noise_key_properties(ui);
            return;
        }
        let Some(mut d) = self.noise.draft.take() else {
            return;
        };
        let format = noise::format(&d.raw).unwrap_or_default();
        ui.label(format!(
            "#{} · {}",
            index + 1,
            if format == Format::Native {
                "Phigros"
            } else {
                "rpe-next"
            }
        ));
        let names = if format == Format::Native {
            ["bottomLeftPercentage", "topRightPercentage"]
        } else {
            ["bottomLeft", "topRight"]
        };
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Center field")).clicked()
                && let Ok([[a, b], [c, e]]) = noise::bounds(&d.raw)
            {
                let (cx, cy) = if format == Format::Native {
                    (0.5, 0.5)
                } else {
                    (0., 0.)
                };
                for (key, x, y) in [
                    (names[0], cx - (c - a) * 0.5, cy - (e - b) * 0.5),
                    (names[1], cx + (c - a) * 0.5, cy + (e - b) * 0.5),
                ] {
                    d.raw[key]["x"] = json!(x);
                    d.raw[key]["y"] = json!(y);
                }
            }
            if ui.button(crate::i18n::tr("Full screen")).clicked() {
                let (lo, hi) = if format == Format::Native {
                    ([0., 0.], [1., 1.])
                } else {
                    ([-675., -450.], [675., 450.])
                };
                for (key, p) in names.into_iter().zip([lo, hi]) {
                    d.raw[key]["x"] = json!(p[0]);
                    d.raw[key]["y"] = json!(p[1]);
                }
            }
        });
        let rgb = noise::color(&d.raw);
        let color = crate::color_ui::swatch(ui, rgb, "Field color…").clicked();
        if ui.button(crate::i18n::tr("Reset field color")).clicked() {
            d.raw.as_object_mut().unwrap().shift_remove(noise::COLOR);
        }
        let edit_preview = ui
            .button(crate::i18n::tr("Edit field in Preview"))
            .clicked();
        self.noise_bounds_preview(ui, &mut d.raw, format);
        if format == Format::Native
            && let Ok(a) = d.times[0].parse::<Beat>()
        {
            ui.small(crate::i18n::format(
                "Chart time: {0} s",
                &[format!(
                    "{:.6}",
                    self.editor.document().tempo().seconds_at(a.as_f64())
                )],
            ));
        }
        ui.separator();
        let form = crate::form::Form::new(
            ui,
            &[
                "Name",
                "Appear beat",
                "Enable beat",
                "Disable beat",
                "Disappear beat",
                "Bottom left X",
                "Bottom left Y",
                "Top right X",
                "Top right Y",
            ],
        );
        let name_key = if format == Format::Native {
            "phiStudioName"
        } else {
            "Name"
        };
        let mut name = noise::name(&d.raw).to_owned();
        if form
            .field(ui, "Name", |ui| ui.text_edit_singleline(&mut name))
            .changed()
        {
            d.raw[name_key] = json!(name);
        }
        let invert = if format == Format::Native {
            "isSubtract"
        } else {
            "isInvert"
        };
        let mut subtract = d.raw[invert].as_bool().unwrap_or(false);
        if ui
            .checkbox(&mut subtract, crate::i18n::tr("Subtract"))
            .changed()
        {
            d.raw[invert] = json!(subtract);
        }
        for (i, label) in [
            "Appear beat",
            "Enable beat",
            "Disable beat",
            "Disappear beat",
        ]
        .iter()
        .enumerate()
        {
            if format == Format::Next && (i == 1 || i == 2) {
                continue;
            }
            form.field(ui, label, |ui| ui.text_edit_singleline(&mut d.times[i]));
        }
        for (point, labels) in names.into_iter().zip([
            ["Bottom left X", "Bottom left Y"],
            ["Top right X", "Top right Y"],
        ]) {
            for (axis, label) in ["x", "y"].into_iter().zip(labels) {
                number(
                    ui,
                    &form,
                    &mut d.raw,
                    &[point, axis],
                    label,
                    if format == Format::Native { 0.005 } else { 1. },
                );
            }
        }
        self.noise.draft = Some(d);
        if edit_preview {
            self.open_noise_preview_editor();
        }
        if color {
            self.open_noise_color(None, rgb);
        }
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Apply field")).clicked() && self.flush_noise_draft() {
                self.refresh_noise();
            }
            if ui.button(crate::i18n::tr("Reset edits")).clicked() {
                self.noise.cache = None;
                self.refresh_noise();
            }
            if ui.button(crate::i18n::tr("Edit animation")).clicked() && self.flush_noise_draft() {
                self.noise.animation = true;
                self.noise.selection.clear();
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(index > 0, egui::Button::new(crate::i18n::tr("Earlier")))
                .clicked()
                && self.flush_noise_draft()
                && self.noise_actions(
                    "Reorder Noise Fields",
                    vec![Action::Move {
                        from: index,
                        to: index - 1,
                    }],
                )
            {
                self.remap_noise_order(index, index - 1);
            }
            if ui
                .add_enabled(
                    index + 1 < self.noise.rows.len(),
                    egui::Button::new(crate::i18n::tr("Later")),
                )
                .clicked()
                && self.flush_noise_draft()
                && self.noise_actions(
                    "Reorder Noise Fields",
                    vec![Action::Move {
                        from: index,
                        to: index + 1,
                    }],
                )
            {
                self.remap_noise_order(index, index + 1);
            }
            if ui.button(crate::i18n::tr("Jump to start")).clicked()
                && let Some(d) = &self.noise.draft
                && let Ok(b) = d.times[0].parse::<Beat>()
            {
                self.set_grid_beat(b.as_f64());
            }
        });
    }
    fn noise_key_properties(&mut self, ui: &mut egui::Ui) {
        ui.label(crate::i18n::tr(self.noise.track.label()));
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Add key at cursor")).clicked() {
                self.add_noise();
            }
            if ui.button(crate::i18n::tr("Back to field")).clicked() {
                self.noise_back();
            }
        });
        let Some(mut d) = self.noise.key_draft.take() else {
            ui.label(crate::i18n::tr(
                "Select a keyframe or draw one on the animation track.",
            ));
            return;
        };
        let native = self.noise.track.native(self.noise_format());
        let track = self.noise.track;
        let form = crate::form::Form::new(
            ui,
            &[
                "Start beat",
                "End beat",
                "Value X",
                "Value Y",
                "Start value",
                "End value",
                "Anchor X",
                "Anchor Y",
            ],
        );
        form.field(ui, if native { "Beat" } else { "Start beat" }, |ui| {
            ui.text_edit_singleline(&mut d.times[0])
        });
        if !native {
            form.field(ui, "End beat", |ui| {
                ui.text_edit_singleline(&mut d.times[1])
            });
        }
        let mut palette = None;
        let mut sample = None;
        if native {
            match track {
                Track::Move | Track::Scale => {
                    let key = if track == Track::Move {
                        "endPosition"
                    } else {
                        "scale"
                    };
                    number(ui, &form, &mut d.raw, &[key, "x"], "Value X", 0.01);
                    number(ui, &form, &mut d.raw, &[key, "y"], "Value Y", 0.01);
                }
                _ => number(ui, &form, &mut d.raw, &["rotation"], "Value", 1.),
            }
            for key in if track == Track::Rotate {
                &["easeType"][..]
            } else {
                &["easeTypeX", "easeTypeY"][..]
            } {
                let mut ease = d.raw[*key].as_u64().unwrap_or(0) as usize;
                let old = ease;
                egui::ComboBox::from_id_source(key)
                    .width(ui.available_width())
                    .selected_text(format!(
                        "{}: {}",
                        crate::i18n::tr(match *key {
                            "easeTypeX" => "Easing X",
                            "easeTypeY" => "Easing Y",
                            _ => "Easing",
                        }),
                        crate::i18n::tr(NATIVE_EASINGS.get(ease).copied().unwrap_or("Unknown"))
                    ))
                    .touch_show_ui(ui, |ui| {
                        for (i, label) in NATIVE_EASINGS.iter().enumerate() {
                            ui.selectable_value(
                                &mut ease,
                                i,
                                format!("{i} · {}", crate::i18n::tr(*label)),
                            );
                        }
                    });
                if ease != old {
                    d.raw[*key] = json!(ease);
                }
            }
        } else if track == Track::Active {
            let mut ready = match &d.raw["readyFlash"] {
                Value::Bool(false) => 0.,
                Value::Number(n) => n.as_f64().unwrap_or(0.5),
                _ => 0.5,
            };
            if form
                .field(ui, "Ready duration (s)", |ui| {
                    ui.add(crate::numeric::Value::new(&mut ready).speed(0.01))
                })
                .changed()
            {
                d.raw["readyFlash"] = json!(ready.max(0.));
            }
        } else {
            if track == Track::Color {
                for (end, key, label) in [
                    (false, "start", "Start color…"),
                    (true, "end", "End color…"),
                ] {
                    if end && noise::flag(&d.raw["inst"]) {
                        continue;
                    }
                    let rgb = noise::rgb(&d.raw[key]).unwrap_or(noise::DEFAULT_COLOR);
                    if crate::color_ui::swatch(ui, rgb, label).clicked() {
                        palette = Some((end, rgb));
                    }
                }
            } else {
                number(ui, &form, &mut d.raw, &["start"], "Start value", 0.1);
                if noise::flag(&d.raw["inst"]) {
                    d.raw["end"] = d.raw["start"].clone();
                } else {
                    number(ui, &form, &mut d.raw, &["end"], "End value", 0.1);
                }
            }
            let mut instant = noise::flag(&d.raw["inst"]);
            if ui
                .checkbox(&mut instant, crate::i18n::tr("Instant"))
                .changed()
            {
                d.raw["inst"] = json!(u8::from(instant));
                if instant {
                    d.raw["end"] = d.raw["start"].clone();
                }
            }
            let mut curve_json = d.raw.clone();
            curve_json["start"] = json!(0.);
            curve_json["end"] = json!(1.);
            curve_json["bezier"] = json!(u8::from(noise::flag(&d.raw["bezier"])));
            if let Ok(mut curve) = serde_json::from_value::<phistudio_core::LineEvent>(curve_json) {
                let old = curve.clone();
                crate::authoring_ui::easing_fields(ui, &mut curve);
                if curve != old {
                    let value = json!(curve);
                    for key in [
                        "easingType",
                        "easingLeft",
                        "easingRight",
                        "bezier",
                        "bezierPoints",
                    ] {
                        d.raw[key] = value[key].clone();
                    }
                }
                crate::authoring_ui::draw_sampled_curve(ui, |t| noise::curve(&curve, t) as f32);
            }
            number(ui, &form, &mut d.raw, &["linkgroup"], "Link group", 1.);
        }
        if track.anchored() {
            let original_anchor = d.raw.get("anchor").cloned();
            if !d.raw["anchor"].is_object() {
                let center = self
                    .noise
                    .draft
                    .as_ref()
                    .and_then(|f| noise::center(&f.raw).ok())
                    .unwrap_or([0., 0.]);
                d.raw["anchor"] = json!({"x":center[0],"y":center[1]});
            }
            let fallback_anchor = d.raw["anchor"].clone();
            number(
                ui,
                &form,
                &mut d.raw,
                &["anchor", "x"],
                "Anchor X",
                if native { 0.005 } else { 1. },
            );
            number(
                ui,
                &form,
                &mut d.raw,
                &["anchor", "y"],
                "Anchor Y",
                if native { 0.005 } else { 1. },
            );
            if ui.button(crate::i18n::tr("Pick anchor")).clicked() {
                sample = Some(SampleTarget::Anchor);
            }
            if original_anchor.is_none() && d.raw["anchor"] == fallback_anchor {
                d.raw.as_object_mut().unwrap().shift_remove("anchor");
            }
        }
        if track == Track::Move && ui.button(crate::i18n::tr("Pick position")).clicked() {
            sample = Some(SampleTarget::Position);
        }
        if !native && track.anchored() {
            egui::CollapsingHeader::new(crate::i18n::tr("Transform reference"))
                .id_source("noise-transform-reference")
                .show(ui, |ui| {
                    let mut custom = d.raw.get("transformStart").is_some()
                        || d.raw.get("transformEnd").is_some();
                    if ui
                        .checkbox(&mut custom, crate::i18n::tr("Custom reference"))
                        .changed()
                    {
                        if custom {
                            d.raw["transformStart"] = d.raw["start"].clone();
                            d.raw["transformEnd"] = d.raw["end"].clone();
                        } else {
                            for k in ["transformStart", "transformEnd", "transformStep"] {
                                d.raw.as_object_mut().unwrap().shift_remove(k);
                            }
                        }
                    }
                    if custom {
                        number(
                            ui,
                            &form,
                            &mut d.raw,
                            &["transformStart"],
                            "Reference start",
                            0.01,
                        );
                        number(
                            ui,
                            &form,
                            &mut d.raw,
                            &["transformEnd"],
                            "Reference end",
                            0.01,
                        );
                        let mut step = noise::flag(&d.raw["transformStep"]);
                        if ui
                            .checkbox(&mut step, crate::i18n::tr("Transform step"))
                            .changed()
                        {
                            d.raw["transformStep"] = json!(step);
                            if step {
                                d.times[1] = d.times[0].clone();
                            }
                        }
                    }
                });
        }
        self.noise.key_draft = Some(d);
        if let Some(target) = sample {
            self.open_noise_sample(target);
        }
        if let Some((end, rgb)) = palette
            && let Some(index) = self.noise.key_selected
        {
            self.open_noise_color(Some((index, end)), rgb);
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Apply keyframe")).clicked() && self.flush_noise_draft() {
                self.refresh_noise();
            }
            if ui.button(crate::i18n::tr("Reset edits")).clicked() {
                self.noise.cache = None;
                self.refresh_noise();
            }
            if ui.button(crate::i18n::tr("Delete keyframe")).clicked() {
                self.delete_noise();
            }
        });
    }
    pub(crate) fn apply_noise_color(&mut self, keyframe: Option<(usize, bool)>, rgb: [u8; 3]) {
        if let Some((index, end)) = keyframe {
            if self.noise.key_selected == Some(index)
                && let Some(d) = &mut self.noise.key_draft
            {
                d.raw[if end { "end" } else { "start" }] = json!(rgb);
                if noise::flag(&d.raw["inst"]) {
                    d.raw["start"] = json!(rgb);
                    d.raw["end"] = json!(rgb);
                }
            }
        } else if let Some(d) = &mut self.noise.draft {
            d.raw[noise::COLOR] = json!(rgb);
        }
        if self.flush_noise_draft() {
            self.refresh_noise();
        }
    }
}
