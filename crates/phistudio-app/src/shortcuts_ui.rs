//! Configurable editor bindings; focused text fields keep normal text shortcuts.
use crate::{scroll::TouchScroll as _, workbench::Workbench};
use egui::{Key, Modifiers};
fn bindings() -> Vec<(&'static str, Key, Modifiers)> {
    let n = Modifiers::NONE;
    let c = Modifiers::COMMAND;
    let s = Modifiers::SHIFT;
    vec![
        ("Select", Key::V, n),
        ("Box", Key::B, n),
        ("Erase", Key::E, n),
        ("Tap", Key::Num1, n),
        ("Hold", Key::Num2, n),
        ("Flick", Key::Num3, n),
        ("Drag", Key::Num4, n),
        ("Mirror X", Key::M, n),
        ("Reverse time", Key::R, n),
        ("Quantize", Key::Q, n),
        ("Play / pause", Key::Space, n),
        ("Delete", Key::Delete, n),
        ("Copy", Key::C, c),
        ("Cut", Key::X, c),
        ("Paste", Key::V, c),
        ("Duplicate", Key::D, c),
        ("Select all", Key::A, c),
        ("Invert selection", Key::I, c),
        ("Undo", Key::Z, c),
        ("Redo", Key::Y, c),
        ("Save", Key::S, c),
        ("Import package", Key::O, c),
        ("Redo (alternate)", Key::Z, c | s),
        ("Paste mirrored", Key::V, c | s),
        ("Clear selection", Key::A, c | s),
        ("Nudge later", Key::ArrowUp, n),
        ("Nudge earlier", Key::ArrowDown, n),
        ("Nudge left", Key::ArrowLeft, n),
        ("Nudge right", Key::ArrowRight, n),
        ("Nudge later ×4", Key::ArrowUp, s),
        ("Nudge earlier ×4", Key::ArrowDown, s),
        ("Nudge left ×4", Key::ArrowLeft, s),
        ("Nudge right ×4", Key::ArrowRight, s),
    ]
}
pub(crate) fn spelling(key: Key, m: Modifiers) -> String {
    format!(
        "{}{}{}{:?}",
        if m.command || m.ctrl || m.mac_cmd {
            "Ctrl+"
        } else {
            ""
        },
        if m.alt { "Alt+" } else { "" },
        if m.shift { "Shift+" } else { "" },
        key
    )
}
impl Workbench {
    pub(crate) fn configured_tool_key(&mut self, key: Key, m: Modifiers) -> bool {
        if self.plugin_key(key, m) {
            return true;
        }
        let pressed = spelling(key, m);
        for (_, original, mods) in bindings() {
            let default = spelling(original, mods);
            if self
                .options
                .shortcuts
                .get(&default)
                .is_some_and(|s| s == &pressed)
            {
                return self.default_tool_key(original, mods);
            }
        }
        if self
            .options
            .shortcuts
            .get(&pressed)
            .is_some_and(|s| s != &pressed)
        {
            return false;
        }
        self.default_tool_key(key, m)
    }
    pub(crate) fn capture_shortcut(&mut self, ctx: &egui::Context) {
        let Some(action) = self.advanced.shortcut_capture.clone() else {
            return;
        };
        let key = ctx.input_mut(|i| {
            let found = i.events.iter().find_map(|e| {
                if let egui::Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    ..
                } = e
                {
                    Some((*key, *modifiers))
                } else {
                    None
                }
            });
            if found.is_some() {
                i.events.retain(|e| !matches!(e, egui::Event::Key { .. }));
            }
            found
        });
        if let Some((key, m)) = key {
            if key == Key::Escape {
                self.advanced.shortcut_capture = None;
                return;
            }
            let text = spelling(key, m);
            let conflict = bindings().into_iter().find(|(_, k, m)| {
                let default = spelling(*k, *m);
                default != action
                    && self.options.shortcuts.get(&default).unwrap_or(&default) == &text
            });
            if let Some((name, _, _)) = conflict {
                self.advanced.shortcut_error = Some((text, name));
            } else {
                self.options.shortcuts.insert(action, text);
                self.advanced.shortcut_error = None;
                self.advanced.shortcut_capture = None;
            }
        }
    }
    pub(crate) fn shortcuts_window(&mut self, ctx: &egui::Context) {
        if !self.advanced.shortcuts_window {
            return;
        }
        let mut open = true;
        super::advanced_ui::window("Keyboard shortcuts", ctx)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_enabled(!self.modal_open());
                if self.advanced.shortcut_capture.is_some() {
                    ui.label(crate::i18n::tr(
                        "Press the new shortcut. Escape cancels recording.",
                    ));
                }
                if let Some((shortcut, action)) = &self.advanced.shortcut_error {
                    ui.colored_label(
                        crate::theme::flick(),
                        crate::i18n::format(
                            "{0} is already assigned to {1}",
                            &[shortcut.clone(), crate::i18n::tr(action)],
                        ),
                    );
                }
                egui::ScrollArea::vertical()
                    .id_source("shortcut-scroll")
                    .max_height(ui.available_height().max(90.))
                    .touch_show(ui, |ui| {
                        for (name, key, m) in bindings() {
                            let default = spelling(key, m);
                            let current = self
                                .options
                                .shortcuts
                                .get(&default)
                                .unwrap_or(&default)
                                .clone();
                            ui.horizontal_wrapped(|ui| {
                                ui.label(crate::i18n::tr(name));
                                ui.monospace(current);
                                if ui
                                    .button(crate::i18n::tr(
                                        if self.advanced.shortcut_capture.as_ref() == Some(&default)
                                        {
                                            "Recording…"
                                        } else {
                                            "Record"
                                        },
                                    ))
                                    .clicked()
                                {
                                    self.advanced.shortcut_capture = Some(default.clone());
                                    self.advanced.shortcut_error = None;
                                }
                                if ui.small_button(crate::i18n::tr("Reset")).clicked() {
                                    let conflict = bindings().into_iter().find(|(_, k, m)| {
                                        let other = spelling(*k, *m);
                                        other != default
                                            && self.options.shortcuts.get(&other).unwrap_or(&other)
                                                == &default
                                    });
                                    if let Some((name, _, _)) = conflict {
                                        self.advanced.shortcut_error = Some((default, name));
                                    } else {
                                        self.options.shortcuts.remove(&default);
                                    }
                                }
                            });
                        }
                        if ui.button(crate::i18n::tr("Reset all shortcuts")).clicked() {
                            self.options.shortcuts.clear();
                            self.advanced.shortcut_capture = None;
                        }
                    });
            });
        self.advanced.shortcuts_window = open;
        if !open {
            self.advanced.shortcut_capture = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binding_captions_and_conflicts_follow_language_changes() {
        let mut app = Workbench::new(
            phistudio_core::ChartDocument::empty("Raw title", 120.).unwrap(),
            None,
        );
        app.advanced.shortcuts_window = true;
        app.advanced.shortcut_error = Some(("Ctrl+S".into(), "Save"));
        let ctx = egui::Context::default();
        ctx.set_style(crate::theme::style());
        ctx.set_fonts(crate::fonts::definitions());
        for language in phistudio_core::Language::ALL {
            app.options.language = language;
            let expected = {
                let _language = crate::i18n::Scope::enter(language);
                if language != phistudio_core::Language::English {
                    for (name, _, _) in bindings() {
                        assert_ne!(crate::i18n::tr(name), name, "Missing binding: {name}");
                    }
                }
                crate::i18n::format(
                    "{0} is already assigned to {1}",
                    &["Ctrl+S".into(), crate::i18n::tr("Save")],
                )
            };
            let mut texts = Vec::new();
            for _ in 0..3 {
                texts = ctx
                    .run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1194., 834.),
                            )),
                            ..Default::default()
                        },
                        |ctx| app.ui(ctx, false),
                    )
                    .shapes
                    .into_iter()
                    .filter_map(|shape| match shape.1 {
                        egui::Shape::Text(t) => Some(t.galley.job.text.clone()),
                        _ => None,
                    })
                    .collect();
            }
            assert!(texts.contains(&expected), "Missing conflict: {expected}");
            assert_eq!(app.advanced.shortcut_error, Some(("Ctrl+S".into(), "Save")));
            assert_eq!(crate::i18n::tr("Save"), "Save");
        }
    }

    #[test]
    fn remapped_keys_replace_defaults_without_stealing_other_actions() {
        let mut app = Workbench::new(
            phistudio_core::ChartDocument::empty("Keys", 120.).unwrap(),
            None,
        );
        app.options.shortcuts.insert("Space".into(), "Alt+P".into());
        assert!(!app.configured_tool_key(Key::Space, Modifiers::NONE));
        assert!(!app.transport.playing);
        assert!(app.configured_tool_key(Key::P, Modifiers::ALT));
        assert!(app.transport.playing);
        assert!(app.configured_tool_key(Key::V, Modifiers::NONE));
        assert!(app.transport.playing);
        assert!(app.configured_tool_key(Key::P, Modifiers::ALT));
        assert!(!app.transport.playing);
    }
}
