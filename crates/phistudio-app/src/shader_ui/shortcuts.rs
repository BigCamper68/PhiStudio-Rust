use super::*;

impl Workbench {
    pub(super) fn shader_keyframe_shortcut(&mut self, key: egui::Key) {
        if !self.flush_shader_draft() {
            return;
        }
        self.refresh_shaders();
        if matches!(key, egui::Key::C | egui::Key::X | egui::Key::D) {
            self.shaders.selection_clipboard.clear();
        }
        let Some(name) = self.shaders.animation_track().map(str::to_owned) else {
            return;
        };
        let Some(effect_index) = self.shaders.selected else {
            return;
        };
        if matches!(key, egui::Key::C | egui::Key::X | egui::Key::D) {
            let Some(a) = &self.shaders.animation else {
                return;
            };
            self.shaders.keyframe_clipboard = Some(a.raw.clone());
            if key == egui::Key::C {
                return;
            }
            if key == egui::Key::X {
                self.shader_delete();
                return;
            }
        }
        let Some(keyframe) = self.shaders.keyframe_clipboard.clone() else {
            return;
        };
        let expected = self
            .shaders
            .parameters
            .iter()
            .find(|p| p.name == name)
            .and_then(|p| uniform_type(&p.default));
        if uniform_type(&keyframe["start"]) != expected
            || uniform_type(&keyframe["end"]) != expected
        {
            self.error = Some("Paste a keyframe with the same parameter type".into());
            return;
        }
        let mut effect = self.shaders.source["effects"][effect_index].clone();
        let index = if let Some(rows) = effect["vars"][&name]
            .as_array_mut()
            .filter(|rows| rows.is_empty() || rows[0].is_object())
        {
            let index = self
                .shaders
                .animation
                .as_ref()
                .map_or(rows.len(), |a| (a.index + 1).min(rows.len()));
            rows.insert(index, keyframe);
            index
        } else {
            effect["vars"][&name] = json!([keyframe]);
            0
        };
        if self.shader_command(ShaderAction::Update {
            index: effect_index,
            effect,
        }) {
            self.refresh_shaders();
            self.shaders.animation = self
                .shaders
                .draft
                .as_ref()
                .and_then(|d| animation_draft(&d.raw, &name, index));
        }
    }

    pub(crate) fn shader_key(&mut self, key: egui::Key, modifiers: egui::Modifiers) -> bool {
        use egui::{Key, Modifiers};
        self.refresh_shaders();
        if (modifiers.matches(Modifiers::COMMAND)
            && matches!(key, Key::C | Key::X | Key::D | Key::V)
            || modifiers.matches(Modifiers::NONE) && key == Key::Delete)
            && grid::selection_key(self, key)
        {
            return true;
        }
        if modifiers.matches(Modifiers::COMMAND | Modifiers::SHIFT) && key == Key::Z {
            self.history_step(true);
            return true;
        }
        if modifiers.matches(Modifiers::COMMAND)
            && self.shaders.animation_track().is_some()
            && matches!(key, Key::C | Key::X | Key::D | Key::V)
        {
            self.shader_keyframe_shortcut(key);
            return true;
        }
        if modifiers.matches(Modifiers::COMMAND) {
            match key {
                Key::Z => self.history_step(false),
                Key::Y => self.history_step(true),
                Key::S => self.save_action(),
                Key::C | Key::X | Key::D => {
                    self.shaders.selection_clipboard.clear();
                    if !self.flush_shader_draft() {
                        return true;
                    }
                    self.refresh_shaders();
                    if let Some(i) = self.shaders.selected {
                        let mut effect = self.shaders.source["effects"][i].clone();
                        if key == Key::D {
                            let lane = layout::lane(&effect, i) + 1.;
                            layout::place(&mut effect, lane);
                            self.shader_command(ShaderAction::Insert {
                                index: i + 1,
                                effect,
                            });
                            self.shaders.selected = Some(i + 1);
                        } else {
                            self.shaders.clipboard = Some(effect);
                            if key == Key::X {
                                self.shader_command(ShaderAction::Delete(i));
                                self.shaders.selected = None;
                            }
                        }
                    }
                }
                Key::V => {
                    if self.flush_shader_draft() {
                        self.refresh_shaders();
                        if let Some(mut effect) = self.shaders.clipboard.clone() {
                            let index = self.shaders.effects.len();
                            layout::place(&mut effect, index as f64);
                            self.shader_command(ShaderAction::Insert { index, effect });
                            self.shaders.selected = Some(index);
                        }
                    }
                }
                _ => return false,
            }
        } else if modifiers.matches(Modifiers::NONE) {
            match key {
                Key::Space => {
                    if self.flush_shader_draft() {
                        self.transport.playing = !self.transport.playing;
                    }
                }
                Key::Delete => self.shader_delete(),
                Key::Escape => self.back(),
                _ => return false,
            }
        } else {
            return false;
        }
        true
    }
}
