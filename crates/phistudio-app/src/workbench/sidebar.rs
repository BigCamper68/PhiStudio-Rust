use super::*;
impl Workbench {
    pub(super) fn sidebar(&mut self, ctx: &egui::Context, canceled: bool) -> bool {
        self.line = self.line.min(self.editor.document().line_count() - 1);
        let resizing_sidebar = self.windows.sidebar && self.capture_sidebar_resize(ctx, canceled);
        if self.windows.sidebar {
            let line_summaries = self.line_summaries();
            let limits = sidebar_widths(ctx.screen_rect().width());
            let width = (ctx.screen_rect().width() * 0.30)
                .clamp(210., 330.)
                .clamp(*limits.start(), *limits.end());
            egui::SidePanel::right("context")
                .default_width(width)
                .width_range(limits)
                .resizable(true)
                .frame(Frame::none().fill(panel()))
                .show(ctx, |ui| {
                    ui.set_enabled(!self.modal_open());
                    Frame::none().inner_margin(8.).show(ui, |ui| {
                        ui.style_mut().wrap = Some(true);
                        ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                        egui::ScrollArea::vertical()
                            .id_source("context-scroll")
                            .always_show_scroll(true)
                            .max_width(ui.available_width())
                            .min_scrolled_height(0.)
                            .max_height(ui.available_height())
                            .touch_show_y(ui, |ui| {
                                self.sidebar_preview(ui);
                                self.plugin_panels(ui, phistudio_sdk::PanelLocation::Sidebar);
                                if !self.windows.floating_properties {
                                    self.plugin_panels(ui, phistudio_sdk::PanelLocation::Inspector);
                                }
                                if self.advanced.right_mode
                                    == crate::advanced_ui::RightMode::NoiseFields
                                {
                                    if !self.windows.floating_properties {
                                        if self.windows.scroll_to_properties {
                                            ui.scroll_to_cursor(Some(Align::TOP));
                                            self.windows.scroll_to_properties = false;
                                        }
                                        if ui
                                            .button(format!(
                                                "{} ↗",
                                                crate::i18n::tr("Noise Field properties")
                                            ))
                                            .clicked()
                                        {
                                            self.windows.floating_properties = true;
                                        }
                                        self.noise_properties(ui);
                                    }
                                    return;
                                }
                                if self.advanced.right_mode
                                    == crate::advanced_ui::RightMode::Shaders
                                {
                                    if !self.windows.floating_properties {
                                        if self.windows.scroll_to_properties {
                                            ui.scroll_to_cursor(Some(Align::TOP));
                                            self.windows.scroll_to_properties = false;
                                        }
                                        if ui
                                            .button(crate::i18n::tr("Shader properties ↗"))
                                            .clicked()
                                        {
                                            self.windows.floating_properties = true;
                                            self.windows.scroll_to_properties = true;
                                        }
                                        self.shader_properties(ui);
                                    }
                                    return;
                                }
                                let groups =
                                    self.editor.document().group_names().unwrap_or_default();
                                if self
                                    .inspector
                                    .group_filter
                                    .is_some_and(|i| i >= groups.len())
                                {
                                    self.inspector.group_filter = None;
                                }
                                egui::ComboBox::from_id_source("line-group-filter")
                                    .selected_text(
                                        self.inspector
                                            .group_filter
                                            .and_then(|i| groups.get(i))
                                            .map_or_else(
                                                || crate::i18n::tr("All line groups"),
                                                String::clone,
                                            ),
                                    )
                                    .touch_show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut self.inspector.group_filter,
                                            None,
                                            crate::i18n::tr("All line groups"),
                                        );
                                        for (i, name) in groups.iter().enumerate() {
                                            ui.selectable_value(
                                                &mut self.inspector.group_filter,
                                                Some(i),
                                                name,
                                            );
                                        }
                                    });
                                egui::ComboBox::from_id_source("line-state-filter")
                                    .selected_text(crate::i18n::tr(
                                        [
                                            "All line states",
                                            "Visible lines",
                                            "Visible notes",
                                            "Active events",
                                        ][self.advanced.line_filter],
                                    ))
                                    .touch_show_ui(ui, |ui| {
                                        for (i, label) in [
                                            "All line states",
                                            "Visible lines",
                                            "Visible notes",
                                            "Active events",
                                        ]
                                        .iter()
                                        .enumerate()
                                        {
                                            ui.selectable_value(
                                                &mut self.advanced.line_filter,
                                                i,
                                                crate::i18n::tr(label),
                                            );
                                        }
                                    });
                                let old_line = self.line;
                                ui.horizontal(|ui| {
                                    let add_width = ui.spacing().interact_size.y;
                                    let combo_width = (ui.available_width()
                                        - add_width
                                        - ui.spacing().item_spacing.x)
                                        .max(1.);
                                    ui.allocate_ui_with_layout(
                                        Vec2::new(combo_width, add_width),
                                        egui::Layout::top_down(Align::Min),
                                        |ui| {
                                            egui::ComboBox::from_id_source("line-selector")
                                                .width(combo_width)
                                                .selected_text(
                                                    if self.options.mark_active_line_id {
                                                        format!(
                                                            "L{} · {}",
                                                            self.line,
                                                            line_summaries[self.line].name
                                                        )
                                                    } else {
                                                        line_summaries[self.line].name.clone()
                                                    },
                                                )
                                                .touch_show_ui(ui, |ui| {
                                                    let activity = (self.advanced.line_filter == 3)
                                                        .then(|| {
                                                            self.line_activity.get(
                                                                self.editor.document(),
                                                                self.editor.revision(),
                                                            )
                                                        });
                                                    for line in line_summaries.iter() {
                                                        if line.index != self.line {
                                                            let show = match self
                                                                .advanced
                                                                .line_filter
                                                            {
                                                                1 => self
                                                                    .advanced
                                                                    .line_states
                                                                    .get(line.index)
                                                                    .is_some_and(|s| s.alpha > 0.),
                                                                2 => self
                                                                    .advanced
                                                                    .visible_lines
                                                                    .contains(&line.index),
                                                                3 => activity.as_ref().is_some_and(
                                                                    |index| {
                                                                        index.contains(
                                                                            line.index,
                                                                            self.transport.beat,
                                                                        )
                                                                    },
                                                                ),
                                                                _ => true,
                                                            };
                                                            if !show {
                                                                continue;
                                                            }
                                                        }
                                                        if self.inspector.group_filter.is_some_and(
                                                            |group| {
                                                                self.editor
                                                                    .document()
                                                                    .line_group(line.index)
                                                                    != group
                                                            },
                                                        ) && line.index != self.line
                                                        {
                                                            continue;
                                                        }
                                                        ui.selectable_value(
                                                            &mut self.line,
                                                            line.index,
                                                            {
                                                                let __i18n_arg_0 = &(line.index);
                                                                let __i18n_arg_1 = &(line.name);
                                                                let __i18n_arg_2 =
                                                                    &(line.note_count);
                                                                crate::i18n::format(
                                                                    "L{0} · {1} · {2}",
                                                                    &[
                                                                        __i18n_arg_0.to_string(),
                                                                        __i18n_arg_1.to_string(),
                                                                        __i18n_arg_2.to_string(),
                                                                    ],
                                                                )
                                                            },
                                                        );
                                                    }
                                                });
                                        },
                                    );
                                    if ui
                                        .add_sized([add_width, add_width], egui::Button::new("+"))
                                        .on_hover_text(crate::i18n::tr("Add a judge line"))
                                        .clicked()
                                    {
                                        let line = self.editor.document().line_count();
                                        self.execute(Command::AddLines { count: 1 });
                                        self.line = line;
                                    }
                                });
                                if old_line != self.line {
                                    self.select(None);
                                    self.timeline.drag = None;
                                    self.unlink_invalid_xy();
                                }
                                ui.small(
                                    RichText::new({
                                        let __i18n_arg_0 = &(self.editor.document().line_count());
                                        let __i18n_arg_1 = &(self.editor.document().note_count());
                                        crate::i18n::format(
                                            "{0} lines · {1} notes",
                                            &[__i18n_arg_0.to_string(), __i18n_arg_1.to_string()],
                                        )
                                    })
                                    .color(muted()),
                                );
                                ui.horizontal_wrapped(|ui| {
                                    if ui.button(crate::i18n::tr("Line monitor")).clicked() {
                                        self.advanced.monitor = true;
                                    }
                                    if ui.button(crate::i18n::tr("Groups")).clicked() {
                                        self.open_groups();
                                    }
                                    if ui.button(crate::i18n::tr("Controls")).clicked() {
                                        self.open_controls(phistudio_core::ControlKind::Position);
                                    }
                                    if ui.button(crate::i18n::tr("Line properties")).clicked() {
                                        self.windows.line_properties = true;
                                    }
                                    if ui.button(crate::i18n::tr("BPM")).clicked() {
                                        self.windows.bpm = true;
                                    }
                                });
                                ui.separator();
                                if !self.windows.floating_properties {
                                    if self.windows.scroll_to_properties {
                                        ui.scroll_to_cursor(Some(Align::TOP));
                                        self.windows.scroll_to_properties = false;
                                    }
                                    if self.advanced.story_selected.is_some()
                                        || (self.advanced.right_mode
                                            == crate::advanced_ui::RightMode::Storyboard
                                            && self.selection().is_empty())
                                    {
                                        if ui
                                            .button(crate::i18n::tr("Storyboard properties ↗"))
                                            .clicked()
                                        {
                                            self.windows.floating_properties = true;
                                        }
                                        self.story_properties(ui);
                                    } else if self.selection().len() > 1 {
                                        self.selection_properties(ui);
                                    } else if self.events.selected.is_some() {
                                        if ui
                                            .button(crate::i18n::tr("Event properties ↗"))
                                            .clicked()
                                        {
                                            self.windows.floating_properties = true;
                                            self.windows.scroll_to_properties = true;
                                        }
                                        self.event_properties(ui);
                                    } else if self.selected.is_some() {
                                        if ui.button(crate::i18n::tr("Note properties ↗")).clicked()
                                        {
                                            self.windows.floating_properties = true;
                                            self.windows.scroll_to_properties = true;
                                        }
                                        self.properties(ui);
                                    } else {
                                        ui.small(
                                            RichText::new(crate::i18n::tr(
                                                "Select a note or an event",
                                            ))
                                            .color(muted()),
                                        );
                                    }
                                }
                            });
                    });
                });
        }

        resizing_sidebar
    }
}
