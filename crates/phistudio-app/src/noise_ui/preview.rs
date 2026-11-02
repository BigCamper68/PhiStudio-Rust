use super::*;
use egui::{Rect, Sense, Stroke, pos2, vec2};

fn bounds_names(format: Format) -> [&'static str; 2] {
    if format == Format::Native {
        ["bottomLeftPercentage", "topRightPercentage"]
    } else {
        ["bottomLeft", "topRight"]
    }
}
pub(super) fn project(rect: Rect, point: [f64; 2], format: Format) -> egui::Pos2 {
    let p = if format == Format::Native {
        point
    } else {
        [point[0] / 1350. + 0.5, point[1] / 900. + 0.5]
    };
    pos2(
        rect.left() + p[0] as f32 * rect.width(),
        rect.bottom() - p[1] as f32 * rect.height(),
    )
}
impl Workbench {
    pub(crate) fn noise_preview_override(&self) -> Option<(usize, Value)> {
        if self.noise.preview_original.is_none() && self.noise.sample_target.is_none() {
            return None;
        }
        let mut raw = self.noise.draft.as_ref()?.raw.clone();
        if self.noise.sample_target.is_some() {
            let index = self.noise.key_selected?;
            let key = raw.get_mut(self.noise.track.key())?.get_mut(index)?;
            *key = self.noise.key_draft.as_ref()?.raw.clone();
        }
        Some((self.noise.selected?, raw))
    }
    /// Runtime and GPU probes share the same draft-to-renderer update path.
    pub(crate) fn sync_noise_preview(
        &self,
        preview: &mut phistudio_prpr::Preview,
        cache: &mut Option<(usize, Value)>,
        force: bool,
    ) -> Result<(), phistudio_core::ChartError> {
        let draft = self.noise_preview_override();
        if force || draft != *cache {
            let field = draft
                .as_ref()
                .map(|(index, raw)| {
                    noise::Field::new(raw, self.editor.document().tempo())
                        .map(|field| (*index, field))
                })
                .transpose()?;
            preview.set_noise_field_override(field);
            *cache = draft;
        }
        Ok(())
    }
    pub(crate) fn cancel_noise_bounds_drag(&mut self) {
        self.cancel_noise_sample_drag();
        if let Some((original, _, _)) = self.noise.preview_drag.take()
            && let Some(draft) = &mut self.noise.draft
        {
            draft.raw = original;
        }
    }
    pub(crate) fn open_noise_preview_editor(&mut self) {
        if let Some(draft) = &self.noise.draft {
            self.noise.preview_original = Some(draft.raw.clone());
            self.noise.preview_drag = None;
            self.preview.pick = None;
            self.open_fullscreen_preview();
        }
    }
    pub(crate) fn cancel_noise_preview_editor(&mut self) {
        self.cancel_noise_sample();
        self.noise.preview_drag = None;
        if let Some(original) = self.noise.preview_original.take()
            && let Some(draft) = &mut self.noise.draft
        {
            draft.raw = original;
        }
        self.noise.preview_handles.clear();
        self.noise.preview_toolbar = None;
    }
    pub(super) fn paint_noise_preview_grid(&self, ui: &egui::Ui, rect: Rect) {
        let painter = ui.painter_at(rect);
        if self.options.noise_preview_grid {
            for (vertical, count) in [
                (true, self.lanes.saturating_sub(1).max(1)),
                (false, self.division.max(1) as usize),
            ] {
                for i in 0..=count {
                    let t = i as f32 / count as f32;
                    let (a, b) = if vertical {
                        (
                            pos2(rect.left() + t * rect.width(), rect.top()),
                            pos2(rect.left() + t * rect.width(), rect.bottom()),
                        )
                    } else {
                        (
                            pos2(rect.left(), rect.bottom() - t * rect.height()),
                            pos2(rect.right(), rect.bottom() - t * rect.height()),
                        )
                    };
                    painter.line_segment([a, b], Stroke::new(1_f32, border().gamma_multiply(0.8)));
                }
            }
        }
    }
    fn paint_noise_surface(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        raw: &Value,
        format: Format,
        full: bool,
    ) -> Vec<Rect> {
        self.paint_noise_preview_grid(ui, rect);
        let painter = ui.painter_at(rect);
        let Ok([a, b]) = noise::bounds(raw) else {
            return vec![];
        };
        let corners = [project(rect, a, format), project(rect, b, format)];
        let shape = Rect::from_two_pos(corners[0], corners[1]);
        let rgb = noise::color(raw);
        let color = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        if !full {
            painter.rect_filled(shape, 0., color.gamma_multiply(0.2));
        }
        painter.rect_stroke(shape, 0., Stroke::new(1.5_f32, color));
        let mut points = corners.to_vec();
        if full {
            points.push(shape.center());
        }
        for (i, point) in points.iter().enumerate() {
            painter.circle_filled(*point, if full { 9. } else { 6. }, color);
            if i == 2 {
                painter.line_segment(
                    [*point - vec2(5., 0.), *point + vec2(5., 0.)],
                    Stroke::new(1_f32, base()),
                );
                painter.line_segment(
                    [*point - vec2(0., 5.), *point + vec2(0., 5.)],
                    Stroke::new(1_f32, base()),
                );
            }
        }
        points
            .into_iter()
            .map(|p| Rect::from_center_size(p, vec2(44., 44.)))
            .collect()
    }
    fn drag_noise_bounds(&self, raw: &mut Value, rect: Rect, format: Format, pos: egui::Pos2) {
        let Some((old, origin, handle)) = &self.noise.preview_drag else {
            return;
        };
        if pos.distance(*origin) < 3. {
            *raw = old.clone();
            return;
        }
        let Ok(bounds) = noise::bounds(old) else {
            return;
        };
        let scale = if format == Format::Native {
            [1., 1.]
        } else {
            [1350., 900.]
        };
        let mut delta = [
            (pos.x - origin.x) as f64 / rect.width() as f64 * scale[0],
            -(pos.y - origin.y) as f64 / rect.height() as f64 * scale[1],
        ];
        // Moving snaps one corner and preserves the field's dimensions.
        let anchor = bounds[(*handle).min(1)];
        for (axis, count, snap) in [
            (0, self.lanes.saturating_sub(1).max(1), self.options.snap_x),
            (1, self.division.max(1) as usize, self.options.snap_time),
        ] {
            if snap {
                let step = scale[axis] / count as f64;
                let offset = if format == Format::Native {
                    0.
                } else {
                    scale[axis] * 0.5
                };
                delta[axis] = ((anchor[axis] + delta[axis] + offset) / step).round() * step
                    - offset
                    - anchor[axis];
            }
        }
        for (i, key) in bounds_names(format).iter().enumerate() {
            if *handle == 2 || *handle == i {
                raw[*key]["x"] = json!(bounds[i][0] + delta[0]);
                raw[*key]["y"] = json!(bounds[i][1] + delta[1]);
            }
        }
    }
    /// Inspector edits remain staged until Apply, including canceled pointer gestures.
    pub(super) fn noise_bounds_preview(
        &mut self,
        ui: &mut egui::Ui,
        raw: &mut Value,
        format: Format,
    ) {
        let width = ui.available_width().max(1.);
        let (rect, response) = ui.allocate_exact_size(vec2(width, width / 1.5), Sense::drag());
        ui.painter().rect_filled(rect, 2., base());
        let handles = self.paint_noise_surface(ui, rect, raw, format, false);
        if response.drag_started()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(pos);
            let handle = handles.iter().position(|r| r.contains(origin)).unwrap_or(2);
            self.noise.preview_drag = Some((raw.clone(), origin, handle));
        }
        if response.dragged()
            && let Some(pos) = response.interact_pointer_pos()
        {
            self.drag_noise_bounds(raw, rect, format, pos);
        }
        if response.drag_released() {
            self.noise.preview_drag = None;
        }
    }
    pub(crate) fn noise_preview_editor(&mut self, ui: &mut egui::Ui, rect: Rect) {
        if self.noise.preview_original.is_none() {
            return;
        }
        if let Some(draft) = &self.noise.draft {
            let format = noise::format(&draft.raw).unwrap_or_default();
            self.noise.preview_handles =
                self.paint_noise_surface(ui, rect, &draft.raw, format, true);
        }
        let (done, cancel) = self.noise_preview_toolbar(
            ui,
            rect,
            "Apply field",
            "Drag handles to edit; swipe elsewhere to scrub time",
        );
        if done && self.flush_noise_draft() {
            self.noise.preview_original = None;
            self.close_fullscreen_preview();
        } else if cancel {
            self.close_fullscreen_preview();
        }
    }
    pub(super) fn noise_preview_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        apply: &str,
        help: &str,
    ) -> (bool, bool) {
        let mut done = false;
        let mut cancel = false;
        let toolbar = ui.allocate_ui_at_rect(
            Rect::from_min_size(
                rect.left_top() + vec2(8., 8.),
                vec2((rect.width() - 72.).max(180.), 1.),
            ),
            |ui| {
                egui::Frame::none()
                    .fill(panel())
                    .inner_margin(6.)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            done = ui.button(crate::i18n::tr(apply)).clicked();
                            cancel = ui.button(crate::i18n::tr("Cancel")).clicked();
                            ui.checkbox(
                                &mut self.options.noise_preview_grid,
                                crate::i18n::tr("Field preview grid"),
                            );
                            ui.label(format!("{:.3}", self.beat));
                        });
                        ui.small(crate::i18n::tr(help));
                        if let Some(error) = &self.error {
                            ui.colored_label(flick(), crate::i18n::tr(error));
                        }
                    });
            },
        );
        self.noise.preview_toolbar = Some(toolbar.response.rect);
        (done, cancel)
    }
    pub(crate) fn noise_preview_end_gesture(&mut self) {
        self.noise.sample_drag = None;
        self.noise.preview_drag = None;
    }
    pub(crate) fn noise_preview_press(&mut self, pos: egui::Pos2) -> bool {
        if self.noise.sample_target.is_some() {
            return self.noise_sample_press(pos);
        }
        if self.noise.preview_original.is_none() {
            return false;
        }
        if self.noise.preview_toolbar.is_some_and(|r| r.contains(pos)) {
            return true;
        }
        if let Some(handle) = self
            .noise
            .preview_handles
            .iter()
            .position(|r| r.contains(pos))
            && let Some(draft) = &self.noise.draft
        {
            self.noise.preview_drag = Some((draft.raw.clone(), pos, handle));
            return true;
        }
        false
    }
    pub(crate) fn noise_preview_move(&mut self, pos: egui::Pos2) -> bool {
        if self.noise.sample_target.is_some() {
            return self.noise_sample_move(pos);
        }
        if self.noise.preview_original.is_none() || self.noise.preview_drag.is_none() {
            return false;
        }
        if let Some(mut draft) = self.noise.draft.take() {
            let format = noise::format(&draft.raw).unwrap_or_default();
            self.drag_noise_bounds(&mut draft.raw, self.preview.rect, format, pos);
            self.noise.draft = Some(draft);
        }
        true
    }
}
