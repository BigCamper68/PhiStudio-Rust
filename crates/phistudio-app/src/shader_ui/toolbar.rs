use super::*;

impl Workbench {
    pub(crate) fn workspace_selector(&mut self, ui: &mut egui::Ui) {
        let before = self.advanced.right_mode;
        let mut next = before;
        egui::ComboBox::from_id_source("right-grid-mode")
            .width(104.)
            .selected_text(crate::i18n::tr(match before {
                RightMode::Events => "Events",
                RightMode::Storyboard => "Storyboard",
                RightMode::Shaders => "Shaders",
                RightMode::NoiseFields => "Noise Fields",
            }))
            .touch_show_ui(ui, |ui| {
                ui.selectable_value(&mut next, RightMode::Events, crate::i18n::tr("Events"));
                ui.selectable_value(
                    &mut next,
                    RightMode::Storyboard,
                    crate::i18n::tr("Storyboard"),
                );
                ui.selectable_value(&mut next, RightMode::Shaders, crate::i18n::tr("Shaders"));
                ui.selectable_value(
                    &mut next,
                    RightMode::NoiseFields,
                    crate::i18n::tr("Noise Fields"),
                );
            });
        if next != before {
            if !self.flush_shader_draft() {
                return;
            }
            self.advanced.right_mode = next;
            self.select(None);
            self.drag = None;
            self.grid_input.cancel();
            self.shaders.track = None;
            self.shaders.page = ShaderPage::Effect;
            self.shaders.animation = None;
            if next == RightMode::Storyboard {
                self.open_storyboard();
            }
            if next == RightMode::NoiseFields {
                self.show_context = true;
                self.show_edit = false;
                self.compact_properties = false;
                self.set_tool(crate::workbench::Tool::Select);
                self.refresh_noise();
            }
            if next == RightMode::Shaders {
                self.show_context = true;
                self.show_edit = false;
                self.compact_properties = false;
                self.set_tool(crate::workbench::Tool::Select);
                self.refresh_shaders();
            }
        }
    }

    pub(crate) fn shader_toolbar(&mut self, ui: &mut egui::Ui) {
        self.refresh_shaders();
        let valid = self.shaders.error.is_none();
        ui.add_enabled_ui(valid, |ui| {
            let selected = self.shaders.selected;
            if ui
                .add_enabled(
                    selected.is_some(),
                    egui::Button::new(crate::i18n::tr("Duplicate effect")),
                )
                .clicked()
                && self.flush_shader_draft()
            {
                self.refresh_shaders();
                let i = selected.unwrap();
                if self.shader_command(ShaderAction::Insert {
                    index: i + 1,
                    effect: self.shaders.source["effects"][i].clone(),
                }) {
                    self.shaders.selected = Some(i + 1);
                    self.refresh_shaders();
                }
            }
            if ui
                .add_enabled(
                    if self.shaders.animation_track().is_some() {
                        self.shaders.animation.is_some()
                    } else {
                        selected.is_some()
                    },
                    egui::Button::new(crate::i18n::tr(
                        if self.shaders.animation_track().is_some() {
                            "Delete keyframe"
                        } else {
                            "Delete effect"
                        },
                    )),
                )
                .clicked()
            {
                self.shader_delete();
            }
            if ui.button(crate::i18n::tr("Import GLSL")).clicked() {
                self.open_media_picker(MediaKind::Shader);
            }
            if ui.button(crate::i18n::tr("New GLSL")).clicked() {
                self.new_shader_code(TEMPLATE.into());
            }
            if ui
                .selectable_label(self.shaders.code_view, crate::i18n::tr("GLSL"))
                .clicked()
            {
                self.shaders.code_view = !self.shaders.code_view;
            }
            if ui
                .selectable_label(self.show_context, crate::i18n::tr("Properties"))
                .clicked()
            {
                self.show_context = !self.show_context;
            }
        });
    }
}
