use super::*;

impl Workbench {
    pub(crate) fn shader_source_view(&mut self, ui: &mut egui::Ui) -> bool {
        if !self.shaders.code_view {
            return false;
        }
        self.grid_area = Rect::NOTHING;
        self.grid_hover = None;
        self.drag = None;
        self.grid_input.cancel();
        ui.horizontal(|ui| {
            self.workspace_selector(ui);
            if ui.button(crate::i18n::tr("Back to timeline")).clicked() {
                self.shaders.code_view = false;
            }
        });
        self.shader_code_ui(ui);
        true
    }

    pub(super) fn new_shader_code(&mut self, source: String) {
        if !self.flush_shader_draft() {
            return;
        }
        self.shaders.code = source;
        self.shaders.code_original.clear();
        self.shaders.code_target.clear();
        let sources = shader_sources(self.editor.document()).unwrap_or_default();
        let mut n = 1;
        loop {
            let path = format!("shaders/custom-{n}.glsl");
            if !sources.contains_key(&path)
                && self.asset_root().is_none_or(|r| !r.join(&path).exists())
            {
                self.shaders.code_path = path;
                break;
            }
            n += 1;
        }
        self.shaders.code_view = true;
    }

    pub(crate) fn accept_shader_file(&mut self, file: MediaFile) {
        match std::fs::read_to_string(file.path()) {
            Ok(source) => {
                self.new_shader_code(source);
                self.status = format!("Imported {} · Apply source to use it", file.name());
            }
            Err(e) => self.error = Some(format!("Could not read shader: {e}")),
        }
    }

    pub(super) fn save_shader_code(&mut self) -> bool {
        let path = self.shaders.code_path.trim().to_owned();
        let source = self.shaders.code.clone();
        if let Err(e) = validate_shader_source(&path, &source) {
            self.error = Some(e.to_string());
            return false;
        }
        // Source and reference change are a single project history transaction.
        let mut commands = vec![Command::SetShaderSource {
            path: path.clone(),
            source: source.clone(),
        }];
        if let Some(index) = self.shaders.selected {
            let mut effect = self
                .shaders
                .draft
                .as_ref()
                .map(|d| d.raw.clone())
                .unwrap_or_else(|| self.shaders.source["effects"][index].clone());
            effect["shader"] = json!(format!("/{path}"));
            commands.push(Command::EditShader {
                baseline: self.shaders.source.clone(),
                action: ShaderAction::Update { index, effect },
            });
        } else {
            let start = Beat::from_grid(
                self.shaders.tempo.as_ref().map_or(self.beat, |t| {
                    t.beat_at(self.editor.document().tempo().seconds_at(self.beat))
                }),
                self.division,
            )
            .unwrap_or(Beat::ZERO);
            let mut effect = ShaderEffect::new(
                start,
                start
                    .checked_add(Beat::new(4, 0, 1).unwrap())
                    .unwrap_or(Beat::ONE),
            );
            effect.shader = format!("/{path}");
            commands.push(Command::EditShader {
                baseline: self.shaders.source.clone(),
                action: ShaderAction::Insert {
                    index: self.shaders.effects.len(),
                    effect: json!(effect),
                },
            });
        }
        match self.editor.execute_batch("Edit shader source", commands) {
            Ok(_) => {
                self.shaders.selected =
                    Some(self.shaders.selected.unwrap_or(self.shaders.effects.len()));
                self.shaders.code_original = source;
                self.shaders.code_target = format!("/{path}");
                self.playing = false;
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(e.to_string());
                false
            }
        }
    }

    pub(super) fn shader_code_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(crate::i18n::tr("Project path"));
            ui.add(egui::TextEdit::singleline(&mut self.shaders.code_path).desired_width(220.));
            if ui.button(crate::i18n::tr("Apply source")).clicked()
                && self.flush_shader_draft()
                && (self.shaders.code_target == format!("/{}", self.shaders.code_path.trim())
                    || self.save_shader_code())
            {
                self.refresh_shaders();
            }
            if ui.button(crate::i18n::tr("Revert source")).clicked() {
                self.shaders.code = self.shaders.code_original.clone();
            }
        });
        ui.small(crate::i18n::tr("GLSL ES 100 · screenTexture / uv · time / screenSize / UVScale. Parameters: float, vec2, vec4; // %default% annotations."));
        if !self.shaders.code_target.starts_with('/') && !self.shaders.code_target.is_empty() {
            ui.colored_label(
                muted(),
                crate::i18n::tr(
                    "Preset source: choose a .glsl project path to save a custom copy.",
                ),
            );
        }
        if let Some(warning) = &self.resource_warning {
            egui::ScrollArea::vertical()
                .id_source("shader-compile-output")
                .max_height(90.)
                .touch_show(ui, |ui| {
                    ui.colored_label(flick(), crate::i18n::tr(warning));
                });
        }
        let mut layouter = |ui: &egui::Ui, text: &str, width: f32| {
            let mut job = glsl_layout(text);
            job.wrap.max_width = width;
            ui.fonts(|fonts| fonts.layout_job(job))
        };
        egui::ScrollArea::both()
            .id_source("shader-code")
            .auto_shrink([false, false])
            .touch_show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.shaders.code)
                        .code_editor()
                        .layouter(&mut layouter)
                        .desired_width(f32::INFINITY)
                        .desired_rows(25),
                );
            });
    }
}
