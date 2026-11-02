use super::*;
impl Workbench {
    pub(super) fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("tools")
            .exact_height(64.)
            .frame(
                Frame::none()
                    .fill(panel())
                    .inner_margin(egui::Margin::symmetric(6., 4.)),
            )
            .show(ctx, |ui| {
                ui.set_enabled(!self.modal_open());
                ui.spacing_mut().scroll_bar_width = 3.;
                ui.spacing_mut().scroll_bar_inner_margin = 1.;
                egui::ScrollArea::horizontal()
                    .id_source("editor-tools-scroll")
                    .touch_show_x(ui, |ui| {
                        ui.horizontal(|ui| {
                            let noise = self.advanced.right_mode
                                == crate::advanced_ui::RightMode::NoiseFields;
                            let shaders = self.advanced.right_mode.dedicated();
                            for (tool, label, color) in [
                                (Tool::Select, "Select", text()),
                                (Tool::Place(NoteKind::Tap), "Tap", tap()),
                                (Tool::Place(NoteKind::Hold), "Hold", hold()),
                                (Tool::Place(NoteKind::Flick), "Flick", flick()),
                                (Tool::Place(NoteKind::Drag), "Drag", drag()),
                                (
                                    Tool::Event,
                                    if noise {
                                        self.noise_placement_label()
                                    } else if shaders {
                                        self.shader_placement_label()
                                    } else {
                                        "+ Event"
                                    },
                                    accent(),
                                ),
                                (Tool::BoxSelect, "Box", text()),
                                (Tool::Erase, "Erase", flick()),
                            ] {
                                if shaders && matches!(tool, Tool::Place(_)) {
                                    continue;
                                }
                                if ui
                                    .add_enabled(
                                        shaders || tool != Tool::Event || self.events.layer < 4,
                                        egui::SelectableLabel::new(
                                            self.timeline.tool == tool,
                                            RichText::new(crate::i18n::tr(label)).color(color),
                                        ),
                                    )
                                    .clicked()
                                {
                                    self.set_tool(tool);
                                }
                            }
                            ui.separator();
                            if ui.button(crate::i18n::tr("Grid")).clicked() {
                                self.windows.grid = !self.windows.grid;
                            }
                            if noise {
                                self.noise_toolbar(ui);
                                return;
                            }
                            if shaders {
                                self.shader_toolbar(ui);
                                return;
                            }
                            if ui.button(crate::i18n::tr("Edit")).clicked() {
                                self.windows.edit = !self.windows.edit;
                            }
                            if ui.button(crate::i18n::tr("Tools")).clicked() {
                                self.tools.show = !self.tools.show;
                            }
                            if ui
                                .selectable_label(self.tools.additive, crate::i18n::tr("Add"))
                                .on_hover_text(crate::i18n::tr(
                                    "Add or remove objects by tapping; hold Shift on desktop",
                                ))
                                .clicked()
                            {
                                self.tools.additive = !self.tools.additive;
                            }
                        });
                    });
                ui.spacing_mut().interact_size.y = 16.;
                ui.horizontal(|ui| {
                    ui.small(
                        RichText::new({
                            let __i18n_arg_0 = &(self
                                .editor
                                .document()
                                .tempo()
                                .seconds_at(self.transport.beat));
                            let __i18n_arg_1 = &(crate::i18n::tr(
                                if self.advanced.right_mode
                                    == crate::advanced_ui::RightMode::Shaders
                                {
                                    "FX beat"
                                } else {
                                    "Beat"
                                },
                            ));
                            let __i18n_arg_2 = &(self.grid_beat());
                            let __i18n_arg_3 = &(self.grid_bpm());
                            crate::i18n::format(
                                "{0} s   {1} {2}   BPM {3}",
                                &[
                                    format!("{:.2}", __i18n_arg_0),
                                    __i18n_arg_1.to_string(),
                                    format!("{:.2}", __i18n_arg_2),
                                    format!("{:.1}", __i18n_arg_3),
                                ],
                            )
                        })
                        .monospace(),
                    );
                    ui.separator();
                    let message = self
                        .error
                        .as_deref()
                        .or(self.render_error.as_deref())
                        .or(self.resource_warning.as_deref())
                        .unwrap_or(&self.status);
                    let color = if self.error.is_some()
                        || self.render_error.is_some()
                        || self.resource_warning.is_some()
                    {
                        flick()
                    } else {
                        muted()
                    };
                    ui.add(
                        egui::Label::new(
                            RichText::new(crate::i18n::tr(message))
                                .color(color)
                                .size(11.),
                        )
                        .wrap(false),
                    );
                });
            });
    }
}
