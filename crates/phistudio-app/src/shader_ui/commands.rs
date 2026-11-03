use super::*;

impl Workbench {
    pub(super) fn shader_command(&mut self, action: ShaderAction) -> bool {
        let result = self.editor.execute(Command::EditShader {
            baseline: self.shaders.source.clone(),
            action,
        });
        match result {
            Ok(changed) => {
                if changed {
                    self.transport.playing = false;
                    self.remember_anchor(self.anchor());
                }
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(e.to_string());
                false
            }
        }
    }

    pub(crate) fn flush_shader_draft(&mut self) -> bool {
        if self.advanced.right_mode == RightMode::NoiseFields {
            return self.flush_noise_draft();
        }
        if self.advanced.right_mode != RightMode::Shaders {
            return true;
        }
        self.refresh_shaders();
        if !self.apply_shader_animation() {
            return false;
        }
        if let Some(d) = &mut self.shaders.draft {
            let range = d
                .start
                .parse::<Beat>()
                .and_then(|start| d.end.parse::<Beat>().map(|end| (start, end)));
            match range {
                Ok((start, end)) => {
                    for (key, value) in [("start", start), ("end", end)] {
                        if serde_json::from_value::<Beat>(d.raw[key].clone()).ok() != Some(value) {
                            d.raw[key] = json!(value);
                        }
                    }
                }
                Err(e) => {
                    self.error = Some(e.to_string());
                    return false;
                }
            }
            let (index, effect) = (d.index, d.raw.clone());
            if !self.shader_command(ShaderAction::Update { index, effect }) {
                return false;
            }
        }
        if self.shaders.code != self.shaders.code_original && !self.save_shader_code() {
            return false;
        }
        true
    }

    pub(crate) fn select_shader(&mut self, index: usize) {
        if self.shaders.selected == Some(index) {
            return;
        }
        if !self.flush_shader_draft() {
            return;
        }
        self.shaders.selected = Some(index);
        self.shaders.selection.clear();
        self.shaders.offsets[1] = 0.;
        self.shaders.track = None;
        self.shaders.page = ShaderPage::Effect;
        self.shaders.key = None;
        self.refresh_shaders();
        self.windows.sidebar = true;
    }

    #[cfg(test)]
    pub(super) fn add_shader(&mut self) {
        if !self.flush_shader_draft() {
            return;
        }
        self.refresh_shaders();
        let time = self
            .editor
            .document()
            .tempo()
            .seconds_at(self.transport.beat);
        let beat = self
            .shaders
            .tempo
            .as_ref()
            .map_or(self.transport.beat, |t| t.beat_at(time));
        let start = Beat::from_grid(beat, self.timeline.division).unwrap_or(Beat::ZERO);
        let mut effect = ShaderEffect::new(
            start,
            start
                .checked_add(Beat::new(4, 0, 1).unwrap())
                .unwrap_or(Beat::ONE),
        );
        effect.shader = PRESETS[self.shaders.preset].into();
        let index = self.shaders.effects.len();
        if self.shader_command(ShaderAction::Insert {
            index,
            effect: json!(effect),
        }) {
            self.shaders.selected = Some(index);
            self.refresh_shaders();
        }
    }

    pub(super) fn shader_delete(&mut self) {
        if let Some(name) = self.shaders.animation_track().map(str::to_owned) {
            let Some(index) = self.shaders.animation.as_ref().map(|a| a.index) else {
                return;
            };
            if !self.flush_shader_draft() {
                return;
            }
            self.refresh_shaders();
            let Some(effect_index) = self.shaders.selected else {
                return;
            };
            let mut effect = self.shaders.source["effects"][effect_index].clone();
            if let Some(rows) = effect["vars"][&name].as_array_mut() {
                let value = rows.remove(index)["start"].clone();
                let next = index.min(rows.len().saturating_sub(1));
                if rows.is_empty() {
                    effect["vars"][&name] = value;
                }
                if self.shader_command(ShaderAction::Update {
                    index: effect_index,
                    effect,
                }) {
                    self.refresh_shaders();
                    self.shaders.animation = self
                        .shaders
                        .draft
                        .as_ref()
                        .and_then(|d| animation_draft(&d.raw, &name, next));
                }
            }
        } else if let Some(index) = self.shaders.selected {
            self.shaders.animation = None;
            self.shaders.draft = None;
            if self.shader_command(ShaderAction::Delete(index)) {
                self.shaders.selected = None;
                self.refresh_shaders();
            }
        }
    }
}
