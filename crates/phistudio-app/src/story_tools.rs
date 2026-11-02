//! Storyboard uses the same sparse authoring history and touch-safe forms.
use crate::{
    advanced_ui::StoryId,
    scroll::{TouchCombo as _, TouchScroll as _},
    workbench::Workbench,
};
use phistudio_core::{
    Beat, ChartError,
    story_tools::{self, StoryEdit},
    tools::Transform,
};

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct StoryToolsState {
    shift: String,
    scale: String,
    spacing: String,
    count: usize,
    destination: usize,
    move_source: bool,
    range: [String; 2],
}
impl Default for StoryToolsState {
    fn default() -> Self {
        Self {
            shift: "1".into(),
            scale: "2".into(),
            spacing: "4".into(),
            count: 1,
            destination: 0,
            move_source: false,
            range: ["0".into(), "4".into()],
        }
    }
}
impl Workbench {
    pub(crate) fn story_selection(&self) -> Vec<StoryId> {
        if self.advanced.story_selection.is_empty() {
            self.advanced.story_selected.into_iter().collect()
        } else {
            self.advanced.story_selection.clone()
        }
    }
    pub(crate) fn set_story_selection(&mut self, mut ids: Vec<StoryId>) {
        ids.sort_unstable();
        ids.dedup();
        self.set_object_selection(phistudio_core::tools::Selection::default(), ids);
    }
    pub(crate) fn set_object_selection(
        &mut self,
        normal: phistudio_core::tools::Selection,
        mut ids: Vec<StoryId>,
    ) {
        ids.sort_unstable();
        ids.dedup();
        self.set_selection(normal);
        self.advanced.story_selected = ids.first().copied();
        self.advanced.story_selection = ids;
        self.advanced.story = None;
        if self.advanced.story_selected.is_some() {
            self.windows.sidebar = true;
            self.windows.scroll_to_properties = true;
            self.tools.scope = if self.selection().is_empty() {
                crate::toolkit::Scope::Events
            } else {
                crate::toolkit::Scope::Both
            };
        }
        if self.options.auto_copy_selection
            && self.selection().len() + self.story_selection().len() > 1
        {
            self.copy_selection(false);
        }
    }
    pub(crate) fn apply_object_tool(
        &mut self,
        edit: Result<phistudio_core::objects::ObjectEdit, ChartError>,
    ) -> bool {
        if edit.is_ok() {
            self.remember_tool_parameters();
        }
        let anchor = self.anchor();
        match edit.and_then(|edit| edit.apply(&mut self.editor)) {
            Ok(Some(selected)) => {
                self.remember_anchor(anchor);
                if let Some(line) = selected
                    .normal
                    .notes
                    .first()
                    .map(|id| id.line)
                    .or_else(|| selected.normal.events.first().map(|id| id.line))
                    .or_else(|| selected.story.first().map(|id| id.line))
                {
                    self.line = line;
                }
                if let Some(id) = selected.normal.events.first() {
                    self.events.layer = id.layer;
                }
                self.set_object_selection(selected.normal, selected.story);
                self.timeline.cancel_gesture();
                self.transport.playing = false;
                self.error = None;
                self.status = self.editor.undo_label().unwrap_or("Objects updated").into();
                true
            }
            Ok(None) => {
                self.error = None;
                self.status = "No changes".into();
                false
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }
    pub(crate) fn toggle_story(&mut self, id: Option<StoryId>, additive: bool) {
        if !additive {
            self.select_story(id);
            return;
        }
        let mut ids = self.story_selection();
        if let Some(id) = id {
            if let Some(at) = ids.iter().position(|i| *i == id) {
                ids.remove(at);
            } else {
                ids.push(id);
            }
            self.set_object_selection(self.selection(), ids);
        }
    }
    pub(crate) fn apply_story_tool(&mut self, edit: Result<StoryEdit, ChartError>) -> bool {
        if edit.is_ok() {
            self.remember_tool_parameters();
        }
        let anchor = self.anchor();
        match edit.and_then(|edit| edit.apply(&mut self.editor)) {
            Ok(Some(ids)) => {
                self.remember_anchor(anchor);
                if let Some(id) = ids.first() {
                    self.line = id.line;
                }
                self.set_story_selection(ids);
                self.timeline.cancel_gesture();
                self.transport.playing = false;
                self.error = None;
                self.status = self
                    .editor
                    .undo_label()
                    .unwrap_or("Storyboard updated")
                    .into();
                true
            }
            Ok(None) => {
                self.error = None;
                self.status = "No changes".into();
                false
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }
    pub(crate) fn paste_story_selection(&mut self, original_time: bool) {
        self.paste_selection(false, original_time);
    }
    pub(crate) fn visible_story_ids(&self) -> Result<Vec<StoryId>, ChartError> {
        let mut ids = vec![];
        for line in self.target_lines(true) {
            ids.extend(story_tools::all(self.editor.document(), line)?);
        }
        Ok(ids)
    }
    pub(crate) fn select_all_stories(&mut self, invert: bool) {
        match self.visible_story_ids() {
            Ok(mut ids) => {
                if invert {
                    let old = self.story_selection();
                    ids.retain(|i| !old.contains(i));
                }
                self.set_story_selection(ids);
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    pub(crate) fn story_tools_ui(&mut self, ui: &mut egui::Ui) {
        ui.strong(crate::i18n::tr("Storyboard tools"));
        ui.small({
            let __i18n_arg_0 = &(self.story_selection().len());
            let __i18n_arg_1 = &(self.tools.clipboard.as_ref().map_or(0, |c| c.len()));
            crate::i18n::format(
                "{0} selected · Clipboard: {1}",
                &[__i18n_arg_0.to_string(), __i18n_arg_1.to_string()],
            )
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Select all")).clicked() {
                self.select_all_stories(false);
            }
            if ui.button(crate::i18n::tr("Invert")).clicked() {
                self.select_all_stories(true);
            }
            ui.checkbox(
                &mut self.tools.additive,
                crate::i18n::tr("Add to selection"),
            );
        });
        ui.horizontal_wrapped(|ui| {
            let selected = !self.story_selection().is_empty();
            for (label, action) in [("Cut", 0), ("Copy", 1), ("Duplicate", 2), ("Delete", 3)] {
                if ui
                    .add_enabled(selected, egui::Button::new(crate::i18n::tr(label)))
                    .clicked()
                {
                    match action {
                        0 => self.copy_selection(true),
                        1 => self.copy_selection(false),
                        2 => self.duplicate_selection(),
                        _ => self.delete_selected(),
                    }
                }
            }
            if ui
                .add_enabled(
                    self.tools.clipboard.is_some(),
                    egui::Button::new(crate::i18n::tr("Paste at cursor")),
                )
                .clicked()
            {
                self.paste_story_selection(false);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_source("story-tools-scroll")
            .max_height(ui.available_height().max(70.))
            .touch_show(ui, |ui| {
                ui.label(crate::i18n::tr("Select time range (beats)"));
                ui.horizontal_wrapped(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.story_tools.range[0])
                            .desired_width(90.),
                    );
                    ui.label(crate::i18n::tr("To"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.story_tools.range[1])
                            .desired_width(90.),
                    );
                });
                if ui.button(crate::i18n::tr("Select range")).clicked() {
                    let result = (|| {
                        let start = self.story_tools.range[0].parse::<Beat>()?;
                        let end = self.story_tools.range[1].parse::<Beat>()?;
                        if end < start {
                            return Err(ChartError::Invalid(
                                "Selection range must be ordered".into(),
                            ));
                        }
                        self.target_lines(true)
                            .into_iter()
                            .map(|line| {
                                story_tools::range(self.editor.document(), line, start, end)
                            })
                            .collect::<Result<Vec<_>, _>>()
                            .map(|groups| groups.into_iter().flatten().collect::<Vec<_>>())
                    })();
                    match result {
                        Ok(mut ids) => {
                            if self.tools.additive {
                                ids.extend(self.story_selection());
                            }
                            self.set_story_selection(ids);
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
                ui.separator();
                ui.strong(crate::i18n::tr("Time transforms"));
                ui.horizontal_wrapped(|ui| {
                    ui.label(crate::i18n::tr("Shift (beats)"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.story_tools.shift).desired_width(90.),
                    );
                    if ui.button(crate::i18n::tr("Shift")).clicked() {
                        let edit = self.story_tools.shift.parse::<Beat>().and_then(|beats| {
                            story_tools::transform(
                                self.editor.document(),
                                &self.story_selection(),
                                Transform::Shift { beats, x: 0. },
                            )
                        });
                        self.apply_story_tool(edit);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(crate::i18n::tr("Time scale"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.story_tools.scale).desired_width(90.),
                    );
                    if ui.button(crate::i18n::tr("Scale")).clicked() {
                        let edit = self.story_tools.scale.parse::<Beat>().and_then(|factor| {
                            story_tools::transform(
                                self.editor.document(),
                                &self.story_selection(),
                                Transform::ScaleTime(factor),
                            )
                        });
                        self.apply_story_tool(edit);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for (label, op) in [
                        ("Reverse time", Transform::ReverseTime),
                        ("Swap values", Transform::ReverseValues),
                        ("Quantize", Transform::Quantize(self.timeline.division)),
                    ] {
                        if ui.button(crate::i18n::tr(label)).clicked() {
                            self.transform_selection(op);
                        }
                    }
                });
                ui.separator();
                ui.strong(crate::i18n::tr("Clone / move"));
                egui::ComboBox::from_id_source("story-clone-line")
                    .selected_text({
                        let __i18n_arg_0 = &(self.story_tools.destination);
                        crate::i18n::format("L{0}", &[__i18n_arg_0.to_string()])
                    })
                    .touch_show_ui(ui, |ui| {
                        for line in self.line_summaries().iter() {
                            ui.selectable_value(&mut self.story_tools.destination, line.index, {
                                let __i18n_arg_0 = &(line.index);
                                let __i18n_arg_1 = &(line.name);
                                crate::i18n::format(
                                    "L{0} · {1}",
                                    &[__i18n_arg_0.to_string(), __i18n_arg_1.to_string()],
                                )
                            });
                        }
                    });
                ui.horizontal_wrapped(|ui| {
                    ui.label(crate::i18n::tr("Copies"));
                    ui.add(
                        crate::numeric::Value::new(&mut self.story_tools.count)
                            .clamp_range(1..=2000),
                    );
                    ui.label(crate::i18n::tr("Spacing (beats)"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.story_tools.spacing)
                            .desired_width(90.),
                    );
                });
                ui.checkbox(
                    &mut self.story_tools.move_source,
                    crate::i18n::tr("Move source"),
                );
                if ui
                    .add_enabled(
                        !self.story_selection().is_empty(),
                        egui::Button::new(crate::i18n::tr("Apply clone / move")),
                    )
                    .clicked()
                {
                    let edit = self
                        .story_tools
                        .spacing
                        .parse::<Beat>()
                        .and_then(|spacing| {
                            story_tools::repeat(
                                self.editor.document(),
                                &self.story_selection(),
                                self.story_tools.count,
                                spacing,
                                Some(self.story_tools.destination),
                                self.story_tools.move_source,
                            )
                        });
                    self.apply_story_tool(edit);
                }
            });
    }
}
