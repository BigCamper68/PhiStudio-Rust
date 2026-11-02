use super::*;
impl Workbench {
    pub(crate) fn noise_navigation(
        &mut self,
        ui: &mut egui::Ui,
        keys: bool,
        grid: crate::grid_input::Grid,
    ) {
        self.refresh_noise();
        ui.set_enabled(!self.modal_open());
        ui.spacing_mut().interact_size.y = 24.;
        crate::timeline::navigation_row(
            ui,
            if keys {
                "noise-animation-navigation"
            } else {
                "noise-field-navigation"
            },
            |ui| {
                if !keys {
                    self.noise_x_navigation(ui);
                    egui::ComboBox::from_id_source("noise-format")
                        .width(90.)
                        .selected_text(match self.noise.format {
                            Format::Native => "Phigros",
                            Format::Next => "rpe-next",
                        })
                        .touch_show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.noise.format, Format::Next, "rpe-next");
                            ui.selectable_value(&mut self.noise.format, Format::Native, "Phigros");
                        });
                    ui.checkbox(&mut self.noise.subtract, crate::i18n::tr("Subtract"));
                } else {
                    self.workspace_selector(ui);
                    if self.advanced.right_mode != RightMode::NoiseFields {
                        return;
                    }
                    let mut jump = false;
                    let mut selected = self.noise.selected;
                    let label = selected
                        .and_then(|i| {
                            self.noise
                                .rows
                                .get(i)
                                .map(|v| format!("#{} · {}", i + 1, noise::name(v)))
                        })
                        .unwrap_or_else(|| crate::i18n::tr("Select field"));
                    let font = egui::TextStyle::Button.resolve(ui.style());
                    let mut job =
                        egui::text::LayoutJob::simple(label, font, ui.visuals().text_color(), 74.);
                    job.wrap.max_rows = 1;
                    let label = ui.fonts(|fonts| fonts.layout_job(job));
                    egui::ComboBox::from_id_source("noise-field-list")
                        .width(104.)
                        .selected_text(label)
                        .touch_show_ui(ui, |ui| {
                            for (i, field) in self.noise.rows.iter().enumerate() {
                                jump |= ui
                                    .selectable_value(
                                        &mut selected,
                                        Some(i),
                                        format!("#{} · {}", i + 1, noise::name(field)),
                                    )
                                    .clicked();
                            }
                        });
                    if jump && let Some(index) = selected {
                        self.jump_noise_field(index, grid);
                    }
                }
            },
        );
    }
    fn noise_x_navigation(&mut self, ui: &mut egui::Ui) {
        let mut x = self.noise.offset + 1.;
        let mut changed = false;
        if ui.add_sized([24., 24.], egui::Button::new("<")).clicked() {
            x -= 1.;
            changed = true;
        }
        changed |= ui
            .add_sized(
                [80., 24.],
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
            self.noise.offset = x - 1.;
            self.timeline.cancel_gesture();
        }
    }
    pub(crate) fn jump_noise_field(&mut self, index: usize, grid: crate::grid_input::Grid) -> bool {
        self.refresh_noise();
        if index >= self.noise.rows.len() || !self.flush_noise_draft() {
            return false;
        }
        self.noise.animation = false;
        self.noise.selected = Some(index);
        self.noise.key_selected = None;
        self.noise.key_draft = None;
        self.noise.selection = [index].into();
        self.noise.cycle = None;
        self.noise.cache = None;
        self.refresh_noise();
        let row = &self.noise.rows[index];
        self.noise.offset = layout::centered_offset(grid, layout::lane(row, index));
        if let Ok(start) = noise::beat(
            row,
            "appearTime",
            self.noise_format(),
            self.editor.document().tempo(),
        ) {
            self.set_grid_beat((start.as_f64() - 1.).max(0.));
        }
        self.timeline.cancel_gesture();
        self.transport.playing = false;
        self.windows.scroll_to_properties = true;
        true
    }
    pub(crate) fn noise_toolbar(&mut self, ui: &mut egui::Ui) {
        if ui
            .button(crate::i18n::tr(if self.noise.animation {
                "Add key at cursor"
            } else {
                "Add field at cursor"
            }))
            .clicked()
        {
            self.add_noise();
        }
        if ui.button(crate::i18n::tr("Duplicate")).clicked() {
            self.noise_key(egui::Key::D, egui::Modifiers::COMMAND);
        }
        if ui.button(crate::i18n::tr("Copy")).clicked() {
            self.noise_key(egui::Key::C, egui::Modifiers::COMMAND);
        }
        if ui
            .add_enabled(
                self.noise.clipboard.is_some(),
                egui::Button::new(crate::i18n::tr("Paste")),
            )
            .clicked()
        {
            self.paste_noise(true);
        }
        if ui.button(crate::i18n::tr("Delete")).clicked() {
            self.delete_noise();
        }
        if !self.noise.animation
            && self
                .noise
                .rows
                .iter()
                .any(|v| noise::format(v).ok() == Some(Format::Next))
        {
            let mut options = self.editor.document().as_json()["noiseAreaOptions"].clone();
            if !options.is_object() {
                options = json!({});
            }
            let mut single = options["ignoreTripleInversion"].as_bool().unwrap_or(false);
            if ui
                .checkbox(&mut single, crate::i18n::tr("Single inversion only"))
                .changed()
                && self.flush_noise_draft()
            {
                options["ignoreTripleInversion"] = json!(single);
                self.noise_actions("Edit Noise Field options", vec![Action::Options(options)]);
            }
        }
    }
}
