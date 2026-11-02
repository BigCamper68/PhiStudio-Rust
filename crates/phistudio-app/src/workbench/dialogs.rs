use super::*;

impl Workbench {
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        self.workspace_window(ctx);
        self.playtest_setup(ctx);
        self.video_window(ctx);
        self.project_dialogs(ctx);
        self.edit_window(ctx);
        self.inspector_windows(ctx);
        self.advanced_windows(ctx);
        self.tools_window(ctx);
        self.audio_guide_window(ctx);
        self.audio_notes_window(ctx);
        self.multi_line_window(ctx);
        if !self.show_projects && self.windows.settings {
            let mut open = true;
            crate::i18n::window("Settings")
                .open(&mut open)
                .default_width(320.)
                .default_pos([48., 56.])
                .resize(|r| {
                    r.max_size([
                        (ctx.screen_rect().width() - 32.).max(180.),
                        (ctx.screen_rect().height() - 90.).max(100.),
                    ])
                })
                .show(ctx, |ui| {
                    ui.set_enabled(!self.modal_open());
                    egui::ScrollArea::vertical()
                        .id_source("settings-scroll")
                        .max_height((ctx.screen_rect().height() - 100.).max(80.))
                        .touch_show(ui, |ui| {
                            self.settings_ui(ui);
                        });
                });
            self.windows.settings &= open;
        }
        if !self.show_projects && self.preview.floating && !self.options.preview_behind_grid {
            let mut open = true;
            crate::i18n::window("Preview")
                .open(&mut open)
                .default_width(340.)
                .default_pos([ctx.screen_rect().right() - 360., 48.])
                .show(ctx, |ui| self.preview_panel(ui));
            self.preview.floating = open;
        }
        if !self.show_projects && self.windows.grid {
            let mut open = true;
            crate::i18n::window("Grid")
                .open(&mut open)
                .default_width(250.)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .id_source("grid-settings-scroll")
                        .max_height((ctx.screen_rect().height() - 110.).max(80.))
                        .touch_show(ui, |ui| {
                            {
                                ui.label(crate::i18n::tr("Vertical grid lines"));
                                ui.horizontal_wrapped(|ui| {
                                    for count in [3, 5, 7, 9, 11, 13, 17, 21, 25, 33] {
                                        ui.selectable_value(
                                            &mut self.timeline.lanes,
                                            count,
                                            count.to_string(),
                                        );
                                    }
                                });
                                ui.separator();
                            }
                            ui.label(crate::i18n::tr("Horizontal grid lines per beat"));
                            ui.horizontal_wrapped(|ui| {
                                for count in [1, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64] {
                                    ui.selectable_value(
                                        &mut self.timeline.division,
                                        count,
                                        count.to_string(),
                                    );
                                }
                            });
                            ui.separator();
                            if self.advanced.right_mode == crate::advanced_ui::RightMode::Shaders {
                                ui.checkbox(&mut self.options.snap_x, crate::i18n::tr("Snap shader X to grid"));
                                ui.checkbox(&mut self.options.snap_time, crate::i18n::tr("Snap time to grid"));
                                ui.separator();
                            }
                            if self.advanced.right_mode == crate::advanced_ui::RightMode::NoiseFields {
                                ui.checkbox(&mut self.options.snap_x,crate::i18n::tr("Snap field X to grid"));
                                ui.checkbox(&mut self.options.snap_time,crate::i18n::tr("Snap time to grid"));
                                ui.checkbox(&mut self.options.noise_preview_grid, crate::i18n::tr("Field preview grid"));
                                ui.separator();
                            }
                            ui.add(
                                egui::Slider::new(&mut self.timeline.zoom, 24. ..=280.).text(crate::i18n::tr("px / beat")),
                            );
                            if ui.button(crate::i18n::tr("Reset zoom")).clicked() {
                                self.timeline.zoom = 96.;
                            }
                            ui.small(crate::i18n::tr(if self.advanced.right_mode == crate::advanced_ui::RightMode::Shaders {
                                "Pinch to zoom. Swipe to scrub time. Draw upwards with + Shader or + Keyframe."
                            } else { "Pinch to zoom and pan. Swipe to scroll. Tap to place a note." }));
                        });
                });
            self.windows.grid = open;
        }
        if !self.show_projects && self.windows.floating_properties {
            let mut open = true;
            crate::i18n::window(
                if self.advanced.right_mode == crate::advanced_ui::RightMode::NoiseFields {
                    "Noise Field properties"
                } else if self.advanced.right_mode == crate::advanced_ui::RightMode::Shaders {
                    "Shader properties"
                } else if self.advanced.story_selected.is_some() {
                    "Storyboard properties"
                } else if self.selection().len() > 1 {
                    "Selection properties"
                } else if self.events.selected.is_some() {
                    "Event properties"
                } else {
                    "Note properties"
                },
            )
            .id(egui::Id::new("floating-properties"))
            .open(&mut open)
            .default_width(280.)
            .default_pos([48., 56.])
            .default_height((ctx.screen_rect().height() - 170.).clamp(100., 300.))
            .resize(|r| {
                r.max_size([
                    ctx.screen_rect().width() - 32.,
                    (ctx.screen_rect().height() - 110.).max(100.),
                ])
            })
            .show(ctx, |ui| {
                ui.set_enabled(!self.modal_open());
                let reset = std::mem::take(&mut self.windows.scroll_to_properties);
                let mut scroll = egui::ScrollArea::vertical();
                if reset {
                    scroll = scroll.vertical_scroll_offset(0.);
                }
                scroll
                    .id_source("note-properties-scroll")
                    .max_height(ui.available_height().max(80.))
                    .touch_show(ui, |ui| {
                        if self.advanced.right_mode == crate::advanced_ui::RightMode::NoiseFields {
                            self.noise_properties(ui)
                        } else if self.advanced.right_mode == crate::advanced_ui::RightMode::Shaders
                        {
                            self.shader_properties(ui)
                        } else if self.advanced.story_selected.is_some() {
                            self.story_properties(ui)
                        } else if self.selection().len() > 1 {
                            self.selection_properties(ui)
                        } else if self.events.selected.is_some() {
                            self.event_properties(ui)
                        } else {
                            self.properties(ui)
                        }
                    });
            });
            if !open {
                self.windows.scroll_to_properties = true;
            }
            self.windows.floating_properties = open;
        }
        if !self.show_projects {
            self.authoring_windows(ctx);
        }
        if let Some(project) = &self.files.imported_project {
            let mut selected = None;
            let mut cancel = false;
            crate::i18n::window("Choose a chart")
                .collapsible(false)
                .resizable(true)
                .default_width(360.)
                .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
                .show(ctx, |ui| {
                    ui.label(&project.name);
                    egui::ScrollArea::vertical()
                        .max_height((ctx.screen_rect().height() - 140.).max(80.))
                        .touch_show(ui, |ui| {
                            for (index, chart) in project.charts.iter().enumerate() {
                                if ui
                                    .button({
                                        let __i18n_arg_0 = &(chart.name);
                                        let __i18n_arg_1 = &(crate::i18n::tr(chart.format.label()));
                                        let __i18n_arg_2 = &(chart.notes);
                                        let __i18n_arg_3 = &(chart
                                            .source
                                            .strip_prefix(&project.assets)
                                            .unwrap_or(&chart.source)
                                            .display());
                                        crate::i18n::format(
                                            "{0} · {1} · {2} notes\n{3}",
                                            &[
                                                __i18n_arg_0.to_string(),
                                                __i18n_arg_1.to_string(),
                                                __i18n_arg_2.to_string(),
                                                __i18n_arg_3.to_string(),
                                            ],
                                        )
                                    })
                                    .clicked()
                                {
                                    selected = Some(index);
                                }
                            }
                        });
                    if ui.button(crate::i18n::tr("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            if let Some(index) = selected {
                let project = self.files.imported_project.take().unwrap();
                self.files.pending_asset_directory = Some(project.assets);
                self.open(project.charts[index].path.clone());
            } else if cancel {
                self.files.imported_project = None;
            }
        }
        if self.files.package_dialog {
            crate::i18n::window("Import ZIP/PEZ package")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
                .show(ctx, |ui| {
                    ui.set_min_width(360.);
                    ui.label(crate::i18n::tr("Package path"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.files.path_input).desired_width(360.),
                    );
                    ui.small(crate::i18n::tr(
                        "ZIP/PEZ packages include the chart, music and artwork.",
                    ));
                    ui.horizontal(|ui| {
                        if ui.button(crate::i18n::tr("Import package")).clicked()
                            && !self.files.path_input.trim().is_empty()
                        {
                            let path = PathBuf::from(self.files.path_input.trim());
                            self.files.package_dialog = false;
                            self.files.request = Some(crate::files::FileRequest::ImportPath {
                                name: path
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into(),
                                path,
                            });
                        }
                        if ui.button(crate::i18n::tr("Cancel")).clicked() {
                            self.files.package_dialog = false;
                        }
                    });
                });
        }
        if let Some(path) = self
            .files
            .pending_open
            .clone()
            .filter(|_| !self.files.package_dialog)
        {
            crate::i18n::window("Unsaved changes")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
                .show(ctx, |ui| {
                    ui.label(crate::i18n::tr(
                        "Save this chart before opening another file.",
                    ));
                    if ui.button(crate::i18n::tr("Save current chart")).clicked() {
                        self.save_action();
                    }
                    if ui
                        .button(RichText::new(crate::i18n::tr("Discard and open")).color(flick()))
                        .clicked()
                    {
                        self.files.request = Some(crate::files::FileRequest::DiscardAndOpen(path));
                    }
                    if ui.button(crate::i18n::tr("Keep editing")).clicked() {
                        self.files.pending_open = None;
                        self.files.pending_asset_directory = None;
                    }
                });
        }
        if self.windows.help {
            crate::i18n::window("PhiStudio Rust · Help").open(&mut self.windows.help).default_width(440.).show(ctx,|ui| {
                egui::ScrollArea::vertical().id_source("help-scroll").max_height((ctx.screen_rect().height()-110.).max(80.)).touch_show(ui,|ui|{
                ui.label(crate::i18n::tr("Tap / Flick / Drag: tap to place, swipe to scroll. Hold: drag upward. Select: tap an object, then drag to move it; drag the endpoints to resize a Hold. Repeated taps on overlapping objects cycle from front to back. Two fingers: zoom and pan. Mouse wheel: scroll. Ctrl+wheel: zoom."));
                ui.separator();
                ui.label(crate::i18n::tr("Box (B): select an area across either grid. Add or Shift/Ctrl: toggle objects with taps. Drag a selected object to move the whole group. Erase (E): swipe to erase, with one Undo for the entire stroke. Select and the beat ruler scroll time."));
                ui.separator();
                ui.label(crate::i18n::tr("Edit: Cut/Copy/Paste, mirrored paste, duplicate, select all/invert and clear. Ctrl+X/C/V/D/A/I use these actions; Ctrl+Shift+V pastes mirrored. Arrows nudge a selection; Shift+arrows nudge four steps. M: mirror X. R: reverse time. Q: quantize. 1–4: Tap/Hold/Flick/Drag. V: Select. Escape: close Tools or clear the selection."));
                ui.separator();
                ui.label(crate::i18n::tr("Tools: range filters, timing/position transforms, note type/side/Fake conversion, line/layer transfer, repeats, batch profiles, note patterns, event split/subdivide/join/connect/easing and paired motion generators. Beat and scale fields accept exact rational values such as 2:1/3. Every tool action has one Undo; failed operations keep the chart unchanged."));
                ui.separator();
                ui.label(crate::i18n::tr("Space: play/pause. Ctrl+Z: undo. Ctrl+Shift+Z or Ctrl+Y: redo. Ctrl+S: save. Delete: delete the selection."));
                ui.separator();
                ui.label(crate::i18n::tr("Events: choose a layer and Create, then drag upward in one of the five tracks. Select an event first, then drag to move it; drag its endpoints to resize. Properties apply values and the native easing curve. Layers above 3 are read only. Windows: floating preview, properties, judge line properties, BPM timeline and grid settings. File: import ZIP/PEZ packages and save projects in the library. Preview uses Phira autoplay and package music. Playtest uses native judgement: Perfect keeps the AP line color, Good changes it to FC, and Bad/Miss turns it white."));
                ui.separator();
                ui.label(crate::i18n::tr("Shaders: choose Shaders in the Events/Storyboard selector. Use + Shader to draw an interval upward. Effect, Parameters, Animation and FX BPM stay above the grid. Choose a parameter, Apply its value or Animate it. Back to effect and Escape/Android Back return from parameter editing. The Animation grid edits only the selected parameter's keyframes."));
                });
            });
        }
        if self.close_requested && !self.files.package_dialog {
            crate::i18n::window("Quit the editor?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
                .show(ctx, |ui| {
                    ui.label(crate::i18n::tr("This chart has unsaved changes."));
                    if ui.button(crate::i18n::tr("Save")).clicked() {
                        self.save_action();
                    }
                    if ui
                        .button(RichText::new(crate::i18n::tr("Discard and quit")).color(flick()))
                        .clicked()
                    {
                        self.close_confirmed = true;
                    }
                    if ui.button(crate::i18n::tr("Keep editing")).clicked() {
                        self.close_requested = false;
                    }
                });
        }
    }
}
