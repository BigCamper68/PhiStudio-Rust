use super::*;
use crate::{
    i18n::{format, tr},
    scroll::TouchScroll as _,
    theme,
};
use egui::{Align2, FontId, Rect, Stroke, pos2, vec2};

/// Returns true when the ordinary platform import picker should open.
pub(crate) fn draw(
    ui: &mut egui::Ui,
    curve: &mut crate::custom_easing_ui::State,
    state: &mut State,
    height: f32,
) -> bool {
    state.request_load();
    let mut import = false;
    ui.horizontal_wrapped(|ui| {
        ui.strong(tr("Curve templates"));
        if state.pending.is_some() {
            ui.spinner();
        }
    });
    let list = egui::ScrollArea::vertical()
        .id_source("curve-template-library")
        .max_height(height)
        .auto_shrink([false, false])
        .touch_show(ui, |ui| {
            if let Some(error) = &state.error {
                ui.colored_label(theme::flick(), tr(error));
            }
            if !state.ready && (state.pending.is_some() || state.error.is_none()) {
                ui.label(tr("Loading curve templates…"));
            }
            if ui
                .add_enabled(
                    state.pending.is_none(),
                    egui::Button::new(tr("Reload library")),
                )
                .clicked()
            {
                state.confirm = None;
                if state.sender.is_some() {
                    state.send(Action::Reload);
                } else {
                    state.request_load();
                }
            }
            let active = state.ready && state.pending.is_none() && state.confirm.is_none();
            let prepared = state.prepared.clone();
            let selected = prepared.as_ref().and_then(|p| {
                p.library
                    .templates
                    .iter()
                    .find(|t| Some(&t.id) == state.selected.as_ref())
            });
            ui.add_enabled_ui(active, |ui| {
                let name = ui.add(
                    egui::TextEdit::singleline(&mut state.name)
                        .hint_text(tr("Template name"))
                        .desired_width(ui.available_width()),
                );
                ui.ctx()
                    .data_mut(|d| d.insert_temp(egui::Id::new("curve-template-name"), name.rect));
                if state.name.chars().count() > 160 {
                    state.name = state.name.chars().take(160).collect();
                }
                ui.horizontal_wrapped(|ui| {
                    let room = prepared.as_ref().is_some_and(|p| {
                        p.library.templates.len() < phistudio_core::custom_easing::MAX_TEMPLATES
                    });
                    let button = ui.add_enabled(room, egui::Button::new(tr("Save current curve")));
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("curve-template-save"), button.rect)
                    });
                    if button.clicked() {
                        state.send(Action::Save {
                            name: state.name.clone(),
                            curve: curve.draft.curve.clone(),
                            replace: None,
                        });
                    }
                    let button = ui.add_enabled(room, egui::Button::new(tr("Import templates…")));
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new("curve-template-import"), button.rect)
                    });
                    import = button.clicked();
                });
                let actions = egui::CollapsingHeader::new(tr("Template actions"))
                    .id_source("curve-template-actions")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(
                                    selected.is_some(),
                                    egui::Button::new(tr("Rename template")),
                                )
                                .clicked()
                                && let Some(template) = selected
                            {
                                state.send(Action::Rename {
                                    id: template.id.clone(),
                                    name: state.name.clone(),
                                });
                            }
                            if ui
                                .add_enabled(
                                    selected.is_some(),
                                    egui::Button::new(tr("Replace template")),
                                )
                                .clicked()
                                && let Some(template) = selected
                            {
                                state.confirm = Some(Confirm::Replace {
                                    id: template.id.clone(),
                                    name: template.name.clone(),
                                    curve: curve.draft.curve.clone(),
                                });
                            }
                            if ui
                                .add_enabled(
                                    selected.is_some(),
                                    egui::Button::new(tr("Delete template")),
                                )
                                .clicked()
                                && let Some(template) = selected
                            {
                                state.confirm = Some(Confirm::Remove {
                                    id: template.id.clone(),
                                    name: template.name.clone(),
                                });
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            let export = ui.button(tr("Export library…"));
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::new("curve-template-export"), export.rect)
                            });
                            if export.clicked() {
                                state.send(Action::Export(None));
                            }
                            if ui
                                .add_enabled(
                                    selected.is_some(),
                                    egui::Button::new(tr("Export selected…")),
                                )
                                .clicked()
                            {
                                state.send(Action::Export(state.selected.clone()));
                            }
                        });
                    });
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        egui::Id::new("curve-template-actions"),
                        actions.header_response.rect,
                    )
                });
            });
            if let Some(confirm) = &state.confirm {
                let (message, name, action) = match confirm {
                    Confirm::Replace { id, name, curve } => (
                        "Replace the saved curve “{0}”?",
                        name,
                        Action::Save {
                            name: name.clone(),
                            curve: curve.clone(),
                            replace: Some(id.clone()),
                        },
                    ),
                    Confirm::Remove { id, name } => (
                        "Delete curve template “{0}”?",
                        name,
                        Action::Remove(id.clone()),
                    ),
                };
                ui.label(format(message, std::slice::from_ref(name)));
                ui.horizontal_wrapped(|ui| {
                    if ui.button(tr("Confirm")).clicked() {
                        state.confirm = None;
                        state.send(action);
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        state.confirm = None;
                    }
                });
            }
            ui.separator();
            ui.add(
                egui::TextEdit::singleline(&mut state.filter)
                    .hint_text(tr("Search templates"))
                    .desired_width(ui.available_width()),
            );
            if state.filter.chars().count() > 160 {
                state.filter = state.filter.chars().take(160).collect();
            }
            if let Some(prepared) = prepared {
                if prepared.library.templates.is_empty() {
                    ui.label(tr(
                        "No saved templates. Save the current curve or import a library.",
                    ));
                }
                let filter = state.filter.to_lowercase();
                for (index, entry) in prepared.library.templates.iter().enumerate() {
                    if !entry.name.to_lowercase().contains(&filter) {
                        continue;
                    }
                    let (rect, response) = ui
                        .allocate_exact_size(vec2(ui.available_width(), 64.), egui::Sense::click());
                    let response = response.on_hover_text(&entry.name);
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            egui::Id::new(("curve-template-row", entry.id.as_str())),
                            rect,
                        )
                    });
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    let selected = state.selected.as_ref() == Some(&entry.id);
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(
                        rect,
                        4.,
                        if selected {
                            theme::raised()
                        } else {
                            theme::base()
                        },
                    );
                    painter.rect_stroke(
                        rect,
                        4.,
                        Stroke::new(
                            1_f32,
                            if selected {
                                theme::accent()
                            } else {
                                theme::border()
                            },
                        ),
                    );
                    let thumb = Rect::from_min_size(rect.min + vec2(8., 8.), vec2(76., 48.));
                    if let Some(preview) = prepared.previews.get(index) {
                        thumbnail(ui, thumb, preview);
                    }
                    let text = Rect::from_min_max(
                        pos2(thumb.right() + 8., rect.top()),
                        rect.max - vec2(6., 0.),
                    );
                    let painter = ui.painter_at(text.intersect(rect));
                    painter.text(
                        text.min + vec2(0., 10.),
                        Align2::LEFT_TOP,
                        &entry.name,
                        FontId::proportional(13.),
                        theme::text(),
                    );
                    painter.text(
                        text.min + vec2(0., 35.),
                        Align2::LEFT_TOP,
                        format("Points: {0}", &[entry.curve.nodes.len().to_string()]),
                        FontId::proportional(11.),
                        theme::muted(),
                    );
                    if active && response.clicked() {
                        state.selected = Some(entry.id.clone());
                        state.name = entry.name.clone();
                    }
                }
            }
        });
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("curve-template-viewport"), list.inner_rect));
    import
}
fn thumbnail(ui: &egui::Ui, rect: Rect, preview: &Thumbnail) {
    let map = |p: &[f32; 2]| {
        pos2(
            rect.left() + p[0] * rect.width(),
            rect.bottom()
                - (p[1] - preview.range[0]) / (preview.range[1] - preview.range[0]).max(1e-6)
                    * rect.height(),
        )
    };
    ui.painter_at(rect).add(egui::Shape::line(
        preview.points.iter().map(map).collect(),
        Stroke::new(1.5_f32, theme::accent()),
    ));
}
