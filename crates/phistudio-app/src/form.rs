//! Measured label columns keep property controls close and aligned.
use egui::{Align, Layout, Ui, vec2};

pub(crate) struct Form {
    label_width: f32,
    stacked: bool,
}

impl Form {
    pub(crate) fn new(ui: &Ui, labels: &[&str]) -> Self {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let color = ui.visuals().text_color();
        let labels: Vec<_> = labels.iter().map(crate::i18n::tr).collect();
        let natural = labels
            .iter()
            .map(|label| {
                ui.painter()
                    .layout_no_wrap(label.clone(), font.clone(), color)
                    .size()
                    .x
                    .ceil()
            })
            .fold(0., f32::max);
        // Reserve a usable numeric input; only long labels need a wrapped column.
        let label_width = natural
            .min(120.)
            .min((ui.available_width() - 88. - 8.).max(0.));
        let wrapped: Vec<_> = labels
            .iter()
            .map(|label| {
                ui.painter()
                    .layout(label.clone(), font.clone(), color, label_width)
            })
            .collect();
        let stacked = label_width < 24. || wrapped.iter().any(|label| label.rows.len() > 2);
        let label_width = wrapped
            .iter()
            .map(|label| label.size().x.ceil())
            .fold(0., f32::max);
        Self {
            label_width,
            stacked,
        }
    }

    pub(crate) fn field<R>(
        &self,
        ui: &mut Ui,
        label: &str,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> R {
        ui.push_id(label, |ui| {
            let width = ui.available_width();
            let stacked = self.stacked;
            let label_width = if stacked { width } else { self.label_width };
            let value_width = if stacked {
                width
            } else {
                width - label_width - 8.
            };
            let layout = if stacked {
                Layout::top_down(Align::Min)
            } else {
                Layout::left_to_right(Align::Center)
            };
            ui.allocate_ui_with_layout(vec2(width, if stacked { 0. } else { 32. }), layout, |ui| {
                ui.spacing_mut().item_spacing = vec2(8., 3.);
                ui.allocate_ui_with_layout(
                    vec2(label_width, if stacked { 0. } else { 32. }),
                    if stacked {
                        Layout::top_down(Align::Min)
                    } else {
                        Layout::left_to_right(Align::Center)
                    },
                    |ui| {
                        ui.set_min_size(vec2(label_width, if stacked { 0. } else { 32. }));
                        ui.add(egui::Label::new(crate::i18n::tr(label)).wrap(true));
                    },
                );
                ui.allocate_ui_with_layout(
                    vec2(value_width, 32.),
                    if stacked {
                        Layout::top_down(Align::Min)
                    } else {
                        Layout::left_to_right(Align::Center)
                    },
                    |ui| {
                        ui.set_min_size(vec2(value_width, 32.));
                        ui.spacing_mut().text_edit_width = value_width;
                        content(ui)
                    },
                )
                .inner
            })
            .inner
        })
        .inner
    }
}
