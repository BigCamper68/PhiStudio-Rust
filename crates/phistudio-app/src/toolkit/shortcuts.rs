use super::*;

impl Workbench {
    pub(super) fn nudge(&mut self, beats: i32, lanes: i32) {
        self.transform_selection(Transform::Shift {
            beats: Beat::new(0, beats, self.timeline.division).unwrap_or(Beat::ZERO),
            x: f64::from(lanes) * 1350. / (self.timeline.lanes.max(2) - 1) as f64,
        });
    }

    pub(crate) fn tool_shortcuts(&mut self, ctx: &egui::Context) {
        // Preserve hardware order when several complete shortcuts arrive in one
        // frame (for example Select all -> Copy -> Paste under render load).
        ctx.input_mut(|input| {
            let events = std::mem::take(&mut input.events);
            for event in events {
                let handled = if let egui::Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    ..
                } = &event
                {
                    !self.modal_open() && self.configured_tool_key(*key, *modifiers)
                } else {
                    false
                };
                if !handled {
                    input.events.push(event);
                }
            }
        });
    }

    pub(crate) fn default_tool_key(&mut self, key: egui::Key, modifiers: egui::Modifiers) -> bool {
        use egui::Key;
        if self.advanced.right_mode == crate::advanced_ui::RightMode::NoiseFields {
            return self.noise_key(key, modifiers);
        }
        if self.advanced.right_mode == crate::advanced_ui::RightMode::Shaders {
            return self.shader_key(key, modifiers);
        }
        let command = egui::Modifiers::COMMAND;
        if modifiers.matches(command | egui::Modifiers::SHIFT) {
            match key {
                Key::Z => self.history_step(true),
                Key::V => self.paste_selection(true, false),
                Key::A => self.select(None),
                _ => return false,
            }
        } else if modifiers.matches(command) {
            match key {
                Key::C => self.copy_selection(false),
                Key::X => self.copy_selection(true),
                Key::V => self.paste_selection(false, false),
                Key::D => self.duplicate_selection(),
                Key::A => self.select_scope(self.tools.scope, false),
                Key::I => self.select_scope(self.tools.scope, true),
                Key::Z => self.history_step(false),
                Key::Y => self.history_step(true),
                Key::S => self.save_action(),
                Key::O => self.open_package_action(),
                _ => return false,
            }
        } else if modifiers.matches(egui::Modifiers::NONE) {
            match key {
                Key::Space => self.transport.playing = !self.transport.playing,
                Key::Delete => self.delete_selected(),
                Key::Escape => {
                    self.timeline.cancel_gesture();
                    self.set_tool(Tool::Select);
                    if self.close_advanced_window() {
                        return true;
                    } else if self.windows.edit {
                        self.windows.edit = false;
                    } else if self.inspector.close_top_window() {
                    } else if self.tools.show {
                        self.tools.show = false;
                    } else {
                        self.select(None);
                    }
                }
                Key::V => self.set_tool(Tool::Select),
                Key::B => self.set_tool(Tool::BoxSelect),
                Key::E => self.set_tool(Tool::Erase),
                Key::Num1 => self.set_tool(Tool::Place(NoteKind::Tap)),
                Key::Num2 => self.set_tool(Tool::Place(NoteKind::Hold)),
                Key::Num3 => self.set_tool(Tool::Place(NoteKind::Flick)),
                Key::Num4 => self.set_tool(Tool::Place(NoteKind::Drag)),
                Key::M => self.transform_selection(Transform::MirrorX(0.)),
                Key::R => self.transform_selection(Transform::ReverseTime),
                Key::Q => self.transform_selection(Transform::Quantize(self.timeline.division)),
                Key::ArrowUp => self.nudge(1, 0),
                Key::ArrowDown => self.nudge(-1, 0),
                Key::ArrowLeft => self.nudge(0, -1),
                Key::ArrowRight => self.nudge(0, 1),
                _ => return false,
            }
        } else if modifiers.matches(egui::Modifiers::SHIFT) {
            match key {
                Key::ArrowUp => self.nudge(4, 0),
                Key::ArrowDown => self.nudge(-4, 0),
                Key::ArrowLeft => self.nudge(0, -4),
                Key::ArrowRight => self.nudge(0, 4),
                _ => return false,
            }
        } else {
            return false;
        }
        true
    }
}
