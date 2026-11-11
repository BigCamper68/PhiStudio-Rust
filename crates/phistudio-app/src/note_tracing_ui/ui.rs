use super::*;
use crate::{
    i18n::{format, tr},
    scroll::TouchCombo as _,
    theme,
};
impl Workbench {
    pub(crate) fn note_tracing_ui(&mut self, ui: &mut egui::Ui) {
        if self
            .note_tracing
            .source
            .is_none_or(|(session, _)| session != self.session)
        {
            self.select_trace_source();
        }
        let lines = self.line_summaries();
        let selected = self.selection().notes.len() == 1;
        if let Some((_, id)) = self.note_tracing.source {
            ui.label(format(
                "Line {0} · Note {1}",
                &[(id.line + 1).to_string(), (id.index + 1).to_string()],
            ));
        } else {
            ui.label(tr("Choose a source note"));
        }
        if ui
            .add_enabled(selected, egui::Button::new(tr("Use selected note")))
            .clicked()
        {
            self.select_trace_source();
        }
        {
            let state = &mut self.note_tracing;
            let form = crate::form::Form::new(
                ui,
                &[
                    "Start beat",
                    "End beat",
                    "Destination",
                    "Layer",
                    "Position tolerance",
                    "Rotation tolerance (degrees)",
                ],
            );
            form.field(ui, "Start beat", |ui| {
                ui.text_edit_singleline(&mut state.start)
            });
            form.field(ui, "End beat", |ui| ui.text_edit_singleline(&mut state.end));
            form.field(ui, "Destination", |ui| {
                egui::ComboBox::from_id_source("note-trace-destination")
                    .selected_text(
                        state
                            .destination
                            .and_then(|index| lines.get(index))
                            .map_or_else(
                                || tr("New judgment line"),
                                |line| format!("L{} · {}", line.index, line.name),
                            ),
                    )
                    .touch_show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.destination, None, tr("New judgment line"));
                        for line in lines.iter() {
                            ui.selectable_value(
                                &mut state.destination,
                                Some(line.index),
                                format!("L{} · {}", line.index, line.name),
                            );
                        }
                    });
            });
            form.field(ui, "Layer", |ui| {
                ui.horizontal(|ui| {
                    for layer in 0..4 {
                        ui.selectable_value(&mut state.layer, layer, layer.to_string());
                    }
                });
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(tr("Sample rate"));
                for rate in [60, 120, 240] {
                    ui.selectable_value(&mut state.rate, rate, format!("{rate} Hz"));
                }
            });
            ui.checkbox(&mut state.rotation, tr("Track rotation"));
            ui.checkbox(&mut state.replace, tr("Replace events in range"));
            ui.collapsing(tr("Tracing precision"), |ui| {
                form.field(ui, "Position tolerance", |ui| {
                    ui.add(
                        crate::numeric::Value::new(&mut state.tolerance)
                            .speed(0.01)
                            .clamp_range(0.001..=100.),
                    )
                });
                form.field(ui, "Rotation tolerance (degrees)", |ui| {
                    ui.add(
                        crate::numeric::Value::new(&mut state.angle_tolerance)
                            .speed(0.001)
                            .clamp_range(0.001..=180.),
                    )
                });
            });
        }
        let key = self.tracing_key();
        if self
            .note_tracing
            .prepared
            .as_ref()
            .is_some_and(|(old, _)| key.as_ref().ok() != Some(old))
        {
            self.note_tracing.prepared = None;
        }
        if let Err(error) = &key {
            ui.small(tr(error));
        }
        let preview = ui.add_enabled(
            key.is_ok() && self.note_tracing.pending.is_none() && self.render_error.is_none(),
            egui::Button::new(tr("Preview tracing")),
        );
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new("note-tracing-preview"), preview.rect));
        if preview.clicked() {
            self.begin_note_trace();
        }
        if self.note_tracing.pending.is_some() {
            let state = &self.note_tracing;
            if let Some(Pending {
                stage: Stage::Sampling(sample),
                ..
            }) = &state.pending
            {
                ui.add(
                    egui::ProgressBar::new(
                        sample.samples.len() as f32 / sample.times.len().max(1) as f32,
                    )
                    .text(tr("Sampling note motion…")),
                );
            } else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr("Preparing tracing events…"));
                });
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
            if ui.button(tr("Cancel")).clicked() {
                self.note_tracing.cancel();
            }
        }
        let mut apply = false;
        if let Some((_, prepared)) = &self.note_tracing.prepared {
            ui.label(format(
                "Preview: {0} samples · {1} events",
                &[
                    prepared.samples.to_string(),
                    prepared
                        .edit
                        .as_ref()
                        .map_or(0, |e| e.event_count())
                        .to_string(),
                ],
            ));
            ui.small(tr("Destination motion"));
            plot(ui, &prepared.points);
            if let Err(error) = &prepared.edit {
                ui.colored_label(theme::flick(), tr(error));
            }
            let response = ui.add_enabled(
                prepared.edit.is_ok(),
                egui::Button::new(tr("Apply tracing")),
            );
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("note-tracing-apply"), response.rect));
            apply = response.clicked();
        }
        if let Some(error) = &self.note_tracing.error {
            ui.colored_label(theme::flick(), tr(error));
        }
        if apply {
            self.apply_note_trace_preview();
        }
    }
    pub(super) fn apply_note_trace_preview(&mut self) {
        let Some((key, prepared)) = self.note_tracing.prepared.take() else {
            return;
        };
        if self.tracing_key().as_ref().ok() != Some(&key) {
            self.note_tracing.error =
                Some("The chart changed. Preview tracing again before applying it.".into());
            return;
        }
        let anchor = self.anchor();
        let destination = key
            .spec
            .destination
            .unwrap_or(self.editor.document().line_count());
        let remap = key
            .spec
            .destination
            .is_none()
            .then(|| self.pending_line_remap(&phistudio_core::Command::AddLines { count: 1 }))
            .flatten();
        match prepared
            .edit
            .and_then(|edit| edit.apply(&mut self.editor).map_err(|e| e.to_string()))
        {
            Ok(selection) => {
                if let Some(remap) = remap {
                    self.apply_line_remap(remap);
                }
                self.line = destination;
                self.events.layer = key.spec.layer;
                self.set_selection(selection);
                self.remember_anchor(anchor);
                self.timeline.cancel_gesture();
                self.transport.playing = false;
                self.note_tracing.error = None;
                self.error = None;
                self.status = "Note Tracing".into();
            }
            Err(error) => self.note_tracing.error = Some(error),
        }
    }
    pub(crate) fn paint_note_trace_path(&self, painter: &egui::Painter, rect: egui::Rect) {
        if !self.tools.show
            || self.tools.page != crate::toolkit::Page::Motion
            || self.note_tracing.mode != MotionMode::NoteTracing
        {
            return;
        }
        let Some((key, prepared)) = &self.note_tracing.prepared else {
            return;
        };
        if self.tracing_key().as_ref().ok() != Some(key) || prepared.path.len() < 2 {
            return;
        }
        let points = prepared
            .path
            .iter()
            .map(|p| {
                rect.min
                    + egui::vec2(
                        p[0].clamp(-4., 5.) as f32 * rect.width(),
                        p[1].clamp(-4., 5.) as f32 * rect.height(),
                    )
            })
            .collect();
        painter.with_clip_rect(rect).add(egui::Shape::line(
            points,
            egui::Stroke::new(1.5_f32, theme::accent()),
        ));
    }
}
fn plot(ui: &mut egui::Ui, points: &[[f64; 2]]) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(1.), 140.),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4., theme::base());
    if points.len() < 2 {
        return;
    }
    let bounds = |axis| {
        points
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p[axis]), hi.max(p[axis]))
            })
    };
    let (left, right) = bounds(0);
    let (bottom, top) = bounds(1);
    let inner = rect.shrink(8.);
    let span = (right - left)
        .max(1.)
        .max((top - bottom).max(1.) * f64::from(inner.width() / inner.height()));
    let center = [(left + right) * 0.5, (bottom + top) * 0.5];
    let map = |p: &[f64; 2]| {
        inner.center()
            + egui::vec2(
                ((p[0] - center[0]) / span) as f32 * inner.width(),
                -((p[1] - center[1]) / span) as f32 * inner.width(),
            )
    };
    painter.add(egui::Shape::line(
        points.iter().map(map).collect(),
        egui::Stroke::new(1.5_f32, theme::accent()),
    ));
    painter.circle_filled(map(&points[0]), 3., theme::tap());
    painter.circle_filled(map(&points[points.len() - 1]), 3., theme::flick());
}
