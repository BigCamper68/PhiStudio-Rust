mod events;
mod generators;
mod selection;
mod transform;
use super::*;
use crate::scroll::{TouchCombo as _, TouchScroll as _};
use std::fmt::Debug;

fn choice<T: Copy + PartialEq + Debug>(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut T,
    options: &[(T, &str)],
) {
    egui::ComboBox::from_id_source(id)
        .width(180.)
        .selected_text(crate::i18n::tr(
            options
                .iter()
                .find(|o| o.0 == *value)
                .map_or("Choose", |o| o.1),
        ))
        .touch_show_ui(ui, |ui| {
            for (option, label) in options {
                ui.selectable_value(value, *option, crate::i18n::tr(*label));
            }
        });
}
fn beat_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(crate::i18n::tr(label));
        ui.add(egui::TextEdit::singleline(value).desired_width(130.));
    });
}
fn number(ui: &mut egui::Ui, label: &str, value: &mut f64) {
    ui.horizontal(|ui| {
        ui.label(crate::i18n::tr(label));
        ui.add(crate::numeric::Value::new(value).speed(1.).max_decimals(5));
    });
}
fn easing(ui: &mut egui::Ui, value: &mut i32) {
    ui.horizontal(|ui| {
        ui.label(crate::i18n::tr("Easing"));
        egui::ComboBox::from_id_source("tool-easing")
            .width(180.)
            .selected_text(crate::i18n::tr(
                crate::authoring_ui::EASINGS[(*value - 1).clamp(0, 28) as usize],
            ))
            .touch_show_ui(ui, |ui| {
                for (i, label) in crate::authoring_ui::EASINGS.iter().enumerate() {
                    ui.selectable_value(
                        value,
                        i as i32 + 1,
                        format!("{} · {}", i + 1, crate::i18n::tr(label)),
                    );
                }
            });
    });
}
const KINDS: [(NoteKind, &str); 4] = [
    (NoteKind::Tap, "Tap"),
    (NoteKind::Hold, "Hold"),
    (NoteKind::Flick, "Flick"),
    (NoteKind::Drag, "Drag"),
];
impl Workbench {
    pub(crate) fn tools_window(&mut self, ctx: &egui::Context) {
        if !self.tools.show || self.show_projects {
            self.tools.nav_context = None;
            return;
        }
        let mut open = true;
        crate::i18n::window("Tools")
            .id(egui::Id::new("authoring-tools"))
            .open(&mut open)
            .default_width(430.)
            .default_height((ctx.screen_rect().height() - 112.).clamp(150., 560.))
            .resize(|r| {
                r.max_size([
                    (ctx.screen_rect().width() - 32.).max(250.),
                    (ctx.screen_rect().height() - 94.).max(150.),
                ])
            })
            .default_pos([54., 56.])
            .show(ctx, |ui| {
                ui.set_enabled(!self.modal_open());
                let controls_top = ui.cursor().top();
                let story = crate::story_grid::active(self)
                    && (self.tools.scope == Scope::Events || !self.story_selection().is_empty());
                if self.tools.nav_context != Some(story) {
                    if story && !matches!(self.tools.page, Page::Configurations | Page::Clipboard) {
                        self.tools.page = Page::Story;
                    } else if !story && self.tools.page == Page::Story {
                        self.tools.page = Page::Selection;
                    }
                    self.tools.nav_context = Some(story);
                }
                let selected = self.selection();
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::tr("Tool group"));
                    choice(
                        ui,
                        "tools-page",
                        &mut self.tools.page,
                        &[
                            (Page::Selection, "Selection"),
                            (Page::Transform, "Transform / arrange"),
                            (Page::Values, "Batch values"),
                            (Page::Notes, "Note generator"),
                            (Page::Events, "Event tools"),
                            (Page::Motion, "Motion generator"),
                            (Page::Text, "Text from notes / lines"),
                            (Page::Configurations, "Tool configurations"),
                            (Page::Clipboard, "Clipboard history"),
                            (Page::Story, "Storyboard tools"),
                        ],
                    );
                });
                if !matches!(self.tools.page, Page::Configurations | Page::Clipboard) {
                    self.tools.last_page = self.tools.page;
                }
                if !matches!(
                    self.tools.page,
                    Page::Configurations | Page::Clipboard | Page::Story | Page::Motion
                ) {
                    ui.small({
                        let __i18n_arg_0 = &(selected.notes.len());
                        let __i18n_arg_1 = &(selected.events.len());
                        let __i18n_arg_2 = &(self.tools.clipboard.as_ref().map_or(0, |c| c.len()));
                        crate::i18n::format(
                            "{0} notes · {1} events selected · Clipboard: {2}",
                            &[
                                __i18n_arg_0.to_string(),
                                __i18n_arg_1.to_string(),
                                __i18n_arg_2.to_string(),
                            ],
                        )
                    });
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(
                                !selected.is_empty(),
                                egui::Button::new(crate::i18n::tr("Cut")),
                            )
                            .clicked()
                        {
                            self.copy_selection(true);
                        }
                        if ui
                            .add_enabled(
                                !selected.is_empty(),
                                egui::Button::new(crate::i18n::tr("Copy")),
                            )
                            .clicked()
                        {
                            self.copy_selection(false);
                        }
                        if ui
                            .add_enabled(
                                self.tools.clipboard.is_some(),
                                egui::Button::new(crate::i18n::tr("Paste at cursor")),
                            )
                            .clicked()
                        {
                            self.paste_selection(false, false);
                        }
                        if ui
                            .add_enabled(
                                !selected.is_empty(),
                                egui::Button::new(crate::i18n::tr("Duplicate")),
                            )
                            .clicked()
                        {
                            self.duplicate_selection();
                        }
                        if ui
                            .add_enabled(
                                !selected.is_empty(),
                                egui::Button::new(crate::i18n::tr("Delete")),
                            )
                            .clicked()
                        {
                            self.delete_selected();
                        }
                    });
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_source(("tools-content", self.tools.page as u8))
                    .max_height(
                        ui.available_height()
                            .min(
                                (ctx.screen_rect().height()
                                    - (ui.cursor().top() - controls_top)
                                    - 72.)
                                    .max(24.),
                            )
                            .max(24.),
                    )
                    .touch_show(ui, |ui| {
                        match self.tools.page {
                            Page::Selection => self.selection_tools_ui(ui),
                            Page::Transform => self.transform_tools_ui(ui),
                            Page::Values => self.value_tools_ui(ui),
                            Page::Notes => self.note_generator_ui(ui),
                            Page::Events => self.event_tools_ui(ui),
                            Page::Motion => self.motion_generator_ui(ui),
                            Page::Text => self.text_composition_ui(ui),
                            Page::Configurations => self.tool_presets_ui(ui),
                            Page::Clipboard => self.clipboard_history_ui(ui),
                            Page::Story => self.story_tools_ui(ui),
                        }
                        if let Some(error) = &self.error {
                            ui.separator();
                            ui.colored_label(flick(), crate::i18n::tr(error));
                        }
                    });
            });
        self.tools.show = open;
    }
}
fn pattern_preview(ui: &mut egui::Ui, notes: &[phistudio_core::Note]) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 80.), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 3., base());
    let Some(first) = notes.first() else {
        return;
    };
    let last = notes.last().unwrap();
    let lo = notes.iter().map(|n| n.x).fold(-675., f64::min);
    let hi = notes.iter().map(|n| n.x).fold(675., f64::max);
    let start = first.start_time.as_f64();
    let duration = (last.start_time.as_f64() - start).max(0.01);
    for note in notes.iter().step_by((notes.len() / 180).max(1)) {
        let x = rect.left() + 8. + ((note.x - lo) / (hi - lo)) as f32 * (rect.width() - 16.);
        let y = rect.bottom()
            - 6.
            - ((note.start_time.as_f64() - start) / duration) as f32 * (rect.height() - 12.);
        p.circle_filled(egui::pos2(x, y), 2., accent());
    }
}
