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
            let selected = self.shaders.selected.is_some()
                && (self.shaders.animation_track().is_none()
                    || self.shaders.animation.is_some()
                    || !self.shaders.selection.is_empty());
            for (label, key) in [("Duplicate", egui::Key::D), ("Copy", egui::Key::C)] {
                if ui
                    .add_enabled(selected, egui::Button::new(crate::i18n::tr(label)))
                    .clicked()
                {
                    self.shader_key(key, egui::Modifiers::COMMAND);
                }
            }
            let paste = if self.shaders.animation_track().is_some() {
                self.shaders.keyframe_clipboard.is_some()
            } else {
                self.shaders.clipboard.is_some()
            } || !self.shaders.selection_clipboard.is_empty();
            if ui
                .add_enabled(paste, egui::Button::new(crate::i18n::tr("Paste")))
                .clicked()
            {
                self.shader_key(egui::Key::V, egui::Modifiers::COMMAND);
            }
            if ui
                .add_enabled(selected, egui::Button::new(crate::i18n::tr("Delete")))
                .clicked()
            {
                self.shader_key(egui::Key::Delete, egui::Modifiers::NONE);
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
