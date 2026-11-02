use super::*;

impl Workbench {
    pub(crate) fn refresh_shaders(&mut self) {
        let key = (self.session, self.editor.revision());
        if self.shaders.key == Some(key) {
            return;
        }
        let selected = self.shaders.selected;
        let animation_index = self.shaders.animation.as_ref().map_or(0, |a| a.index);
        self.shaders.key = Some(key);
        self.shaders.selection.clear();
        self.shaders.draft = None;
        self.shaders.animation = None;

        let result =
            load_shader_extra(self.editor.document(), self.asset_root()).and_then(|source| {
                let effects = shader_effects(&source)?;
                let tempo = shader_tempo(&source)?;
                Ok((source, effects, tempo))
            });
        match result {
            Ok((source, effects, tempo)) => {
                self.shaders.source = source;
                self.shaders.effects = effects;
                self.shaders.tempo = Some(tempo);
                self.shaders.error = None;
                self.shaders.selected = selected.filter(|&i| i < self.shaders.effects.len());
                if let Some(index) = self.shaders.selected {
                    self.load_shader_draft(index);
                    if matches!(
                        self.shaders.page,
                        ShaderPage::Parameters | ShaderPage::Animation
                    ) && !self
                        .shaders
                        .parameters
                        .iter()
                        .any(|p| Some(&p.name) == self.shaders.track.as_ref())
                    {
                        self.shaders.track =
                            self.shaders.parameters.first().map(|p| p.name.clone());
                    }
                    if self.shaders.page == ShaderPage::Animation
                        && let Some(name) = &self.shaders.track
                    {
                        self.shaders.animation = self.shaders.draft.as_ref().and_then(|d| {
                            animation_draft(&d.raw, name, animation_index)
                                .or_else(|| animation_draft(&d.raw, name, 0))
                        });
                    }
                } else {
                    self.shaders.parameters.clear();
                    self.shaders.track = None;
                    if self.shaders.page != ShaderPage::Tempo {
                        self.shaders.page = ShaderPage::Effect;
                    }
                }
            }
            Err(e) => {
                self.shaders.error = Some(e.to_string());
                self.shaders.effects.clear();
                self.shaders.selected = None;
            }
        }
    }

    pub(super) fn shader_source_text(&self, shader: &str) -> Result<String, String> {
        if let Some(path) = shader.strip_prefix('/') {
            read_shader_source(self.editor.document(), self.asset_root(), path)
                .map_err(|e| e.to_string())
        } else {
            phistudio_prpr::shader_preset(shader)
                .map(str::to_owned)
                .ok_or_else(|| format!("Unknown shader: {shader}"))
        }
    }

    pub(super) fn load_shader_draft(&mut self, index: usize) {
        let e = &self.shaders.effects[index];
        self.shaders.draft = Some(EffectDraft {
            index,
            raw: self.shaders.source["effects"][index].clone(),
            start: e.start.to_string(),
            end: e.end.to_string(),
        });
        let name = e.shader.clone();
        self.shaders.animation = None;
        match self.shader_source_text(&name) {
            Ok(code) => {
                self.shaders.parameters = shader_parameters(&code);
                self.shaders.code = code.clone();
                self.shaders.code_original = code;
                self.shaders.code_path = if name.starts_with('/') {
                    name.trim_start_matches('/').into()
                } else {
                    format!("shaders/{name}-custom.glsl")
                };
                self.shaders.code_target = name;
            }
            Err(e) => {
                self.shaders.parameters.clear();
                self.shaders.code.clear();
                self.shaders.code_original.clear();
                self.shaders.code_target = name.clone();
                self.shaders.code_path = name.trim_start_matches('/').into();
                self.error = Some(e);
            }
        }
        if let Some(vars) = self.shaders.source["effects"][index]["vars"].as_object() {
            for (name, value) in vars {
                if !self.shaders.parameters.iter().any(|p| p.name == *name) {
                    let default = if is_animated(value) {
                        value[0]["start"].clone()
                    } else {
                        value.clone()
                    };
                    if uniform_type(&default).is_some() {
                        self.shaders.parameters.push(ShaderParameter {
                            name: name.clone(),
                            default,
                        });
                    }
                }
            }
        }
    }
}
