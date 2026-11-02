use super::*;

impl Workbench {
    pub(in super::super) fn projects_home(&mut self, ctx: &egui::Context) {
        self.project_covers
            .get_or_insert_with(crate::covers::Covers::new)
            .update(ctx);
        let blocked = self.workspace.view.is_some()
            || self.files.busy
            || self.files.recovery.is_some()
            || self.files.new_chart.is_some()
            || self.files.export_package.is_some()
            || self.files.package_dialog
            || self.files.delete_project.is_some();
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(base()).inner_margin(20.))
            .show(ctx, |ui| {
                let gutter = ui.spacing().scroll_bar_width
                    + ui.spacing().scroll_bar_inner_margin
                    + ui.spacing().scroll_bar_outer_margin;
                let content_width = (ui.available_width() - gutter).max(1.);
                let button_width = |ui: &egui::Ui, text: &str| {
                    ui.painter()
                        .layout_no_wrap(
                            crate::i18n::tr(text),
                            egui::TextStyle::Button.resolve(ui.style()),
                            crate::theme::text(),
                        )
                        .size()
                        .x
                        + ui.spacing().button_padding.x * 2.
                };
                let full_labels = ["+ Create chart", "Import ZIP/PEZ…", "Export project…"];
                let minimum = |labels: [&str; 3]| {
                    labels.map(|s| button_width(ui, s).max(ui.spacing().interact_size.x))
                };
                let full_minimum = minimum(full_labels);
                let labels = if full_minimum.iter().sum::<f32>() + ui.spacing().item_spacing.x * 2.
                    > content_width
                {
                    ["+ Create chart", "Import…", "Export…"]
                } else {
                    full_labels
                };
                let natural = minimum(labels);
                let spare =
                    content_width - natural.iter().sum::<f32>() - ui.spacing().item_spacing.x * 2.;
                let action_widths = if spare >= 0. {
                    natural.map(|w| w + spare / 3.)
                } else {
                    [content_width; 3]
                };
                ui.scope(|ui| {
                    ui.set_max_width(content_width);
                    ui.horizontal(|ui| {
                        ui.heading(crate::i18n::tr("Projects"));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    !blocked,
                                    egui::Button::new("×").min_size(egui::vec2(36., 32.)),
                                )
                                .on_hover_text(crate::i18n::tr("Return to editor"))
                                .clicked()
                            {
                                self.show_projects = false;
                            }
                        });
                    });
                    ui.add_space(12.);
                    ui.add_enabled_ui(!blocked, |ui| {
                        let layout = if spare >= 0. {
                            egui::Layout::left_to_right(egui::Align::Center)
                        } else {
                            egui::Layout::top_down(egui::Align::Min)
                        };
                        let height = if spare >= 0. {
                            32.
                        } else {
                            96. + ui.spacing().item_spacing.y * 2.
                        };
                        ui.allocate_ui_with_layout(
                            egui::vec2(content_width, height),
                            layout,
                            |ui| {
                                if ui
                                    .add_sized(
                                        [action_widths[0], 32.],
                                        egui::Button::new(
                                            RichText::new(crate::i18n::tr(labels[0]))
                                                .color(accent()),
                                        ),
                                    )
                                    .clicked()
                                {
                                    self.files.new_chart = Some(NewChart::default());
                                }
                                if ui
                                    .add_sized(
                                        [action_widths[1], 32.],
                                        egui::Button::new(crate::i18n::tr(labels[1])),
                                    )
                                    .on_hover_text(crate::i18n::tr("Import ZIP/PEZ…"))
                                    .clicked()
                                {
                                    self.open_package_action();
                                }
                                if ui
                                    .add_enabled(
                                        self.file.is_some() || self.editor.is_dirty(),
                                        egui::Button::new(crate::i18n::tr(labels[2]))
                                            .min_size(egui::vec2(action_widths[2], 32.)),
                                    )
                                    .on_hover_text(crate::i18n::tr("Export project…"))
                                    .clicked()
                                {
                                    self.files.request = Some(FileRequest::ExportCurrent);
                                }
                            },
                        );
                    });
                    ui.add_space(12.);
                    ui.separator();
                    if self.files.busy {
                        ui.label(RichText::new(crate::i18n::tr(&self.status)).color(muted()));
                    }
                    if let Some(error) = &self.error {
                        ui.colored_label(flick(), crate::i18n::tr(error));
                    }
                });
                let blocked = self.workspace.view.is_some()
                    || self.files.busy
                    || self.files.recovery.is_some()
                    || self.files.new_chart.is_some()
                    || self.files.export_package.is_some()
                    || self.files.package_dialog
                    || self.files.delete_project.is_some();
                let mut selected = None;
                let mut export = None;
                let mut delete = None;
                let mut manage = None;
                if let Some(library) = &self.library {
                    egui::ScrollArea::vertical()
                        .id_source("project-list")
                        .max_height(
                            (ui.available_height()
                                - if self.files.last_deleted.is_some() {
                                    52.
                                } else {
                                    0.
                                })
                            .max(60.),
                        )
                        .touch_show(ui, |ui| {
                            ui.set_max_width(content_width);
                            ui.set_enabled(!blocked);
                            let more_width =
                                button_width(ui, "More…").max(ui.spacing().interact_size.x);
                            let delete_width = button_width(ui, "Delete").max(64.);
                            for project in &library.projects {
                                let current = self
                                    .file
                                    .as_ref()
                                    .is_some_and(|p| p.starts_with(&project.workspace));
                                ui.horizontal(|ui| {
                                    let width = (content_width
                                        - more_width
                                        - delete_width
                                        - ui.spacing().item_spacing.x * 2.)
                                        .max(90.);
                                    let (rect, response) = ui.allocate_exact_size(
                                        egui::vec2(width, 64.),
                                        egui::Sense::click(),
                                    );
                                    if ui.is_rect_visible(rect) {
                                        let texture = project.cover.as_deref().and_then(|path| {
                                            self.project_covers.as_mut().unwrap().texture(ctx, path)
                                        });
                                        let visuals = ui.style().interact(&response);
                                        let painter = ui.painter().with_clip_rect(rect);
                                        painter.rect_filled(rect, 3., visuals.bg_fill);
                                        let cover = egui::Rect::from_min_size(
                                            rect.min + egui::vec2(6., 6.),
                                            egui::vec2(84., 52.),
                                        );
                                        painter.rect_filled(cover, 2., panel());
                                        if let Some(texture) = texture {
                                            painter.image(
                                                texture,
                                                cover,
                                                egui::Rect::from_min_max(
                                                    egui::Pos2::ZERO,
                                                    egui::pos2(1., 1.),
                                                ),
                                                if blocked {
                                                    egui::Color32::from_gray(110)
                                                } else {
                                                    egui::Color32::WHITE
                                                },
                                            );
                                        } else {
                                            let center = cover.center();
                                            painter.circle_stroke(
                                                center + egui::vec2(12., -9.),
                                                4.,
                                                egui::Stroke::new(1_f32, muted()),
                                            );
                                            painter.add(egui::Shape::line(
                                                vec![
                                                    center + egui::vec2(-23., 13.),
                                                    center + egui::vec2(-8., -4.),
                                                    center + egui::vec2(4., 7.),
                                                    center + egui::vec2(12., 0.),
                                                    center + egui::vec2(23., 13.),
                                                ],
                                                egui::Stroke::new(1_f32, muted()),
                                            ));
                                        }
                                        let text_rect = egui::Rect::from_min_max(
                                            egui::pos2(cover.right() + 12., rect.top()),
                                            rect.max - egui::vec2(10., 0.),
                                        );
                                        let text = painter.with_clip_rect(text_rect);
                                        text.text(
                                            egui::pos2(text_rect.left(), rect.top() + 20.),
                                            Align2::LEFT_CENTER,
                                            format!(
                                                "{}{}",
                                                project.name,
                                                if current {
                                                    crate::i18n::tr("  ·  Current")
                                                } else {
                                                    String::new()
                                                }
                                            ),
                                            egui::FontId::proportional(14.),
                                            visuals.text_color(),
                                        );
                                        text.text(
                                            egui::pos2(text_rect.left(), rect.top() + 42.),
                                            Align2::LEFT_CENTER,
                                            {
                                                let __i18n_arg_0 = &(project.notes);
                                                let __i18n_arg_1 = &(project.source_name);
                                                crate::i18n::format(
                                                    "{0} notes · {1}",
                                                    &[
                                                        __i18n_arg_0.to_string(),
                                                        __i18n_arg_1.to_string(),
                                                    ],
                                                )
                                            },
                                            egui::FontId::proportional(12.),
                                            muted(),
                                        );
                                    }
                                    if response.clicked() {
                                        selected = Some(project.workspace.clone());
                                    }
                                    let interact_height = ui.spacing().interact_size.y;
                                    ui.scope(|ui| {
                                        ui.spacing_mut().interact_size.y = 64.;
                                        ui.spacing_mut().interact_size.x = more_width;
                                        ui.menu_button(crate::i18n::tr("More…"), |ui| {
                                            ui.spacing_mut().interact_size.y = interact_height;
                                            if ui.button(crate::i18n::tr("Export…")).clicked() {
                                                export = Some(project.workspace.clone());
                                                ui.close_menu();
                                            }
                                            if ui
                                                .button(crate::i18n::tr("Rename / duplicate…"))
                                                .clicked()
                                            {
                                                manage = Some((
                                                    crate::workspace_ui::View::Project(
                                                        project.workspace.clone(),
                                                    ),
                                                    project.name.clone(),
                                                ));
                                                ui.close_menu();
                                            }
                                            if ui.button(crate::i18n::tr("Versions…")).clicked() {
                                                manage = Some((
                                                    crate::workspace_ui::View::Versions(
                                                        project.workspace.clone(),
                                                    ),
                                                    String::new(),
                                                ));
                                                ui.close_menu();
                                            }
                                        });
                                    });
                                    if ui
                                        .add_sized(
                                            [delete_width, 64.],
                                            egui::Button::new(
                                                RichText::new(crate::i18n::tr("Delete"))
                                                    .color(flick()),
                                            ),
                                        )
                                        .clicked()
                                    {
                                        delete =
                                            Some((project.workspace.clone(), project.name.clone()));
                                    }
                                });
                                ui.add_space(4.);
                            }
                            if library.projects.is_empty() {
                                ui.label(crate::i18n::tr(
                                    "Create a chart or import a ZIP/PEZ package to start.",
                                ));
                            }
                            if !library.issues.is_empty() {
                                ui.collapsing(
                                    {
                                        let __i18n_arg_0 = &(library.issues.len());
                                        crate::i18n::format(
                                            "{0} project warnings",
                                            &[__i18n_arg_0.to_string()],
                                        )
                                    },
                                    |ui| {
                                        for issue in &library.issues {
                                            ui.colored_label(flick(), crate::i18n::tr(issue));
                                        }
                                    },
                                );
                            }
                        });
                }
                if let Some(trash) = &self.files.last_deleted {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label({
                            let __i18n_arg_0 = &(trash.name);
                            crate::i18n::format(
                                "Moved to Trash: {0}",
                                std::slice::from_ref(__i18n_arg_0),
                            )
                        });
                        if ui
                            .add_enabled(
                                !blocked,
                                egui::Button::new(crate::i18n::tr("Undo delete")),
                            )
                            .clicked()
                        {
                            self.files.request = Some(FileRequest::UndoDelete(trash.clone()));
                        }
                    });
                }
                if let Some(target) = delete {
                    self.files.delete_project = Some(target);
                }
                if let Some((view, name)) = manage {
                    self.workspace.name = name;
                    self.open_workspace(view);
                }
                if let Some(workspace) = selected {
                    self.files.request = Some(FileRequest::OpenProject(workspace));
                }
                if let Some(workspace) = export {
                    self.files.request = Some(FileRequest::ExportProject(workspace));
                }
            });
    }
}
