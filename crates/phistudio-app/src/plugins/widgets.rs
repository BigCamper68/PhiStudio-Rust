use crate::scroll::TouchCombo as _;
use phistudio_sdk::{UiEvent, Widget};
use serde_json::{Value, json};

/// Events are collected while drawing and dispatched after all UI borrows end.
pub(super) fn show(
    ui: &mut egui::Ui,
    panel: &str,
    widgets: &mut [Widget],
    events: &mut Vec<(UiEvent, bool)>,
    images: &super::images::Textures,
) {
    for widget in widgets {
        let event = match widget {
            Widget::Label { text } => {
                ui.label(crate::i18n::tr(text.as_str()));
                None
            }
            Widget::Button { id, text, enabled } => ui
                .add_enabled(*enabled, egui::Button::new(crate::i18n::tr(text.as_str())))
                .clicked()
                .then(|| (id.clone(), Value::Null, true)),
            Widget::Bool { id, text, value } => ui
                .checkbox(value, crate::i18n::tr(text.as_str()))
                .changed()
                .then(|| (id.clone(), json!(*value), false)),
            Widget::Number {
                id,
                text,
                value,
                min,
                max,
            } => {
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::tr(text.as_str()));
                    changed = ui
                        .add(
                            crate::numeric::Value::new(value)
                                .clamp_range(*min..=*max)
                                .speed(0.1)
                                .max_decimals(6),
                        )
                        .changed();
                });
                changed.then(|| (id.clone(), json!(*value), false))
            }
            Widget::Text { id, text, value } => {
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::tr(text.as_str()));
                    changed = ui
                        .add(
                            egui::TextEdit::singleline(value)
                                .id_source(id.as_str())
                                .desired_width(180.),
                        )
                        .changed();
                });
                changed.then(|| (id.clone(), json!(value), false))
            }
            Widget::Choice {
                id,
                text,
                value,
                options,
            } => {
                let old = value.clone();
                ui.label(crate::i18n::tr(text.as_str()));
                egui::ComboBox::from_id_source(id.as_str())
                    .selected_text(crate::i18n::tr(
                        options
                            .iter()
                            .find(|(k, _)| k == value)
                            .map_or(value.as_str(), |(_, s)| s.as_str()),
                    ))
                    .touch_show_ui(ui, |ui| {
                        for (key, label) in options {
                            ui.selectable_value(
                                value,
                                key.clone(),
                                crate::i18n::tr(label.as_str()),
                            );
                        }
                    });
                (*value != old).then(|| (id.clone(), json!(value), false))
            }
            Widget::Row { children } => {
                ui.horizontal_wrapped(|ui| show(ui, panel, children, events, images));
                None
            }
            Widget::Column { children } => {
                ui.vertical(|ui| show(ui, panel, children, events, images));
                None
            }
            Widget::Canvas { height, layers } => {
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width().max(1.), *height),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(rect, 3., crate::theme::base());
                super::drawing::paint(
                    ui.painter(),
                    rect,
                    &phistudio_sdk::Viewport {
                        size: [rect.width(), rect.height()],
                        timeline: None,
                    },
                    layers,
                    images,
                );
                None
            }
        };
        if let Some((widget, value, document)) = event {
            events.push((
                UiEvent {
                    panel: panel.into(),
                    widget,
                    value,
                },
                document,
            ));
        }
    }
}
