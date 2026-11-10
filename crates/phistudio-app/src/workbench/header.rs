use super::*;
impl Workbench {
    pub(super) fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("titlebar")
            .exact_height(40.)
            .frame(
                Frame::none()
                    .fill(panel())
                    .inner_margin(egui::Margin::symmetric(6., 4.)),
            )
            .show(ctx, |ui| {
                ui.set_enabled(!self.modal_open());
                ui.horizontal(|ui| {
                    let compact = ctx.screen_rect().width() < 720.;
                    ui.menu_button(crate::i18n::tr("File"), |ui| {
                        egui::ScrollArea::vertical()
                            .id_source("file-menu-scroll")
                            .max_height((ctx.screen_rect().height() - 60.).max(60.))
                            .touch_show(ui, |ui| {
                                if ui
                                    .add_enabled(
                                        self.managed_storage,
                                        egui::Button::new(crate::i18n::tr("Projects…")),
                                    )
                                    .clicked()
                                {
                                    self.open_projects();
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui
                                    .button(crate::i18n::tr("Import ZIP/PEZ package…"))
                                    .clicked()
                                {
                                    self.open_package_action();
                                    ui.close_menu();
                                }
                                if ui.button(crate::i18n::tr("Export project…")).clicked()
                                    && self.flush_shader_draft()
                                {
                                    self.files.request =
                                        Some(crate::files::FileRequest::ExportCurrent);
                                    ui.close_menu();
                                }
                                if ui.button(crate::i18n::tr("Patterns…")).clicked() {
                                    self.open_workspace(crate::workspace_ui::View::Patterns);
                                    ui.close_menu();
                                }
                                if ui
                                    .add_enabled(
                                        self.current_workspace().is_some(),
                                        egui::Button::new(crate::i18n::tr("Project resources…")),
                                    )
                                    .clicked()
                                {
                                    self.open_workspace(crate::workspace_ui::View::Resources);
                                    ui.close_menu();
                                }
                                if let Some(workspace) = self.current_workspace()
                                    && ui.button(crate::i18n::tr("Project versions…")).clicked()
                                {
                                    self.open_workspace(crate::workspace_ui::View::Versions(
                                        workspace,
                                    ));
                                    ui.close_menu();
                                }
                                if ui.button(crate::i18n::tr("Chart properties…")).clicked() {
                                    self.open_chart_properties();
                                    ui.close_menu();
                                }
                                if ui.button(crate::i18n::tr("Playtest…")).clicked() {
                                    self.playtest.setup = true;
                                    ui.close_menu();
                                }
                                if ui.button(crate::i18n::tr("Export MP4…")).clicked() {
                                    self.open_video();
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui.button(crate::i18n::tr("Save")).clicked() {
                                    self.save_action();
                                    ui.close_menu();
                                }
                                self.plugin_editor_menu(ui, phistudio_sdk::EditorMenu::File);
                            });
                    });
                    ui.menu_button(crate::i18n::tr("Windows"), |ui| {
                        if compact {
                            ui.set_min_width((ctx.screen_rect().width() - 24.).min(260.));
                        }
                        egui::ScrollArea::vertical()
                            .id_source("windows-menu-scroll")
                            .max_height((ctx.screen_rect().height() - 60.).max(60.))
                            .touch_show(ui, |ui| {
                                ui.checkbox(&mut self.windows.sidebar, crate::i18n::tr("Sidebar"));
                                ui.add_enabled(
                                    !self.options.preview_behind_grid,
                                    egui::Checkbox::new(
                                        &mut self.preview.floating,
                                        crate::i18n::tr("Floating preview"),
                                    ),
                                );
                                if ui
                                    .checkbox(
                                        &mut self.windows.floating_properties,
                                        crate::i18n::tr("Floating properties"),
                                    )
                                    .changed()
                                {
                                    self.windows.scroll_to_properties = true;
                                }
                                if ui
                                    .checkbox(
                                        &mut self.multi_line.show,
                                        crate::i18n::tr("Multiple lines"),
                                    )
                                    .changed()
                                {
                                    if self.multi_line.show {
                                        self.open_multi_line();
                                    }
                                    ui.close_menu();
                                }
                                if ui
                                    .checkbox(
                                        &mut self.audio_guide.show,
                                        crate::i18n::tr("Audio guide"),
                                    )
                                    .changed()
                                {
                                    ui.close_menu();
                                }
                                if ui
                                    .checkbox(
                                        &mut self.windows.settings,
                                        crate::i18n::tr("Settings"),
                                    )
                                    .changed()
                                {
                                    ui.close_menu();
                                }
                                ui.checkbox(
                                    &mut self.windows.line_properties,
                                    crate::i18n::tr("Judge line properties"),
                                );
                                ui.checkbox(&mut self.windows.bpm, crate::i18n::tr("BPM timeline"));
                                ui.checkbox(
                                    &mut self.windows.grid,
                                    crate::i18n::tr("Grid settings"),
                                );
                                ui.checkbox(&mut self.advanced.history, crate::i18n::tr("History"));
                                self.plugin_editor_menu(ui, phistudio_sdk::EditorMenu::Windows);
                                if compact {
                                    ui.separator();
                                    ui.with_layout(egui::Layout::top_down(Align::Min), |ui| {
                                        ui.label(crate::i18n::tr("Playback speed"));
                                    });
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().button_padding.x = 4.;
                                        ui.spacing_mut().item_spacing.x = 4.;
                                        for rate in phistudio_core::PLAYBACK_RATES {
                                            ui.selectable_value(
                                                &mut self.advanced.playback_rate,
                                                rate,
                                                format!("{rate}×"),
                                            );
                                        }
                                    });
                                }
                                if ui.button(crate::i18n::tr("Controls and help")).clicked() {
                                    self.windows.help = true;
                                    ui.close_menu();
                                }
                            });
                    });
                    if !compact {
                        playback_rate(ui, &mut self.advanced.playback_rate, "playback-rate");
                    }
                    ui.separator();
                    if ui
                        .add_sized(
                            [36., 32.],
                            egui::Button::new(if self.transport.playing { "Ⅱ" } else { "▶" }),
                        )
                        .on_hover_text(crate::i18n::tr("Play / pause · Space"))
                        .clicked()
                    {
                        self.request_editor_playback(!self.transport.playing, true);
                    }
                    if !compact && ui.button(crate::i18n::tr("Save")).clicked() {
                        self.save_action();
                    }
                    if crate::theme::history_button(ui, false, self.editor.can_undo())
                        .on_hover_text(crate::i18n::tr("Undo · Ctrl+Z"))
                        .clicked()
                    {
                        self.history_step(false);
                    }
                    if crate::theme::history_button(ui, true, self.editor.can_redo())
                        .on_hover_text(crate::i18n::tr("Redo · Ctrl+Shift+Z"))
                        .clicked()
                    {
                        self.history_step(true);
                    }
                    let notes_text = crate::i18n::tr("Notes");
                    let notes_width = (ui
                        .painter()
                        .layout_no_wrap(
                            notes_text.clone(),
                            egui::TextStyle::Button.resolve(ui.style()),
                            text(),
                        )
                        .size()
                        .x
                        + ui.spacing().button_padding.x * 2.
                        + 2.)
                        .max(52.);
                    let seek_width = (ui.available_width() * 0.42).clamp(80., 260.).min(
                        (ui.available_width() - notes_width - ui.spacing().item_spacing.x * 2.)
                            .max(32.),
                    );
                    let width = ui.available_width()
                        - seek_width
                        - notes_width
                        - ui.spacing().item_spacing.x * 2.;
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(width.max(0.), 32.), Sense::hover());
                    ui.painter().with_clip_rect(rect).text(
                        rect.left_center(),
                        egui::Align2::LEFT_CENTER,
                        format!(
                            "{}{}",
                            self.project_options
                                .display_field(self.editor.document(), "name"),
                            if self.editor.is_dirty() { " •" } else { "" }
                        ),
                        egui::FontId::proportional(13.),
                        muted(),
                    );
                    if ui
                        .add_sized(
                            [notes_width, 32.],
                            egui::SelectableLabel::new(self.options.seek_notes_only, notes_text),
                        )
                        .on_hover_text(crate::i18n::tr(
                            "Limit seek range to notes · Off: seek across the whole song",
                        ))
                        .clicked()
                    {
                        self.options.seek_notes_only = !self.options.seek_notes_only;
                    }
                    self.seek_bar(ui, seek_width);
                });
            });
    }
}

fn playback_rate(ui: &mut egui::Ui, selected: &mut f64, id: &str) {
    egui::ComboBox::from_id_source(id)
        .width(70.)
        .selected_text(format!("{selected}×"))
        .touch_show_ui(ui, |ui| {
            for rate in phistudio_core::PLAYBACK_RATES {
                ui.selectable_value(selected, rate, format!("{rate}×"));
            }
        });
}
