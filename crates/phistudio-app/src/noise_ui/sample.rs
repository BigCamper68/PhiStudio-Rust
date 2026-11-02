//! Confirmed, reversible animation point selection on the shared Preview grid.
use super::preview::project;
use super::*;
use egui::{FontId, Pos2, Rect, Stroke, vec2};

fn key(target: SampleTarget) -> &'static str {
    match target {
        SampleTarget::Anchor => "anchor",
        SampleTarget::Position => "endPosition",
    }
}
impl Workbench {
    pub(crate) fn open_noise_sample(&mut self, target: SampleTarget) {
        if let Some(draft) = &self.noise.key_draft {
            self.noise.sample_original = Some(draft.raw.clone());
            self.noise.sample_target = Some(target);
            self.noise.sample_drag = None;
            self.noise.preview_handles.clear();
            self.noise.preview_toolbar = None;
            self.preview.pick = None;
            self.open_fullscreen_preview();
        }
    }
    pub(super) fn cancel_noise_sample_drag(&mut self) {
        if let Some((original, _)) = self.noise.sample_drag.take()
            && let Some(draft) = &mut self.noise.key_draft
        {
            draft.raw = original;
        }
    }
    pub(super) fn cancel_noise_sample(&mut self) {
        self.noise.sample_drag = None;
        if let Some(original) = self.noise.sample_original.take()
            && let Some(draft) = &mut self.noise.key_draft
        {
            draft.raw = original;
        }
        self.noise.sample_target = None;
    }
    fn noise_sample_point(&self, raw: &Value, target: SampleTarget) -> [f64; 2] {
        let fallback = self
            .noise
            .draft
            .as_ref()
            .and_then(|d| noise::center(&d.raw).ok())
            .unwrap_or([0., 0.]);
        let value = &raw[key(target)];
        [
            value["x"].as_f64().unwrap_or(fallback[0]),
            value["y"].as_f64().unwrap_or(fallback[1]),
        ]
    }
    fn set_noise_sample_point(&mut self, pos: Pos2) {
        let Some(target) = self.noise.sample_target else {
            return;
        };
        let rect = self.preview.rect;
        if !rect.is_positive() {
            return;
        }
        let mut p = [
            ((pos.x - rect.left()) / rect.width()) as f64,
            ((rect.bottom() - pos.y) / rect.height()) as f64,
        ];
        for (axis, count, snap) in [
            (0, self.lanes.saturating_sub(1).max(1), self.options.snap_x),
            (1, self.division.max(1) as usize, self.options.snap_time),
        ] {
            if snap {
                p[axis] = (p[axis] * count as f64).round() / count as f64;
            }
        }
        if self.noise_format() == Format::Next {
            p = [(p[0] - 0.5) * 1350., (p[1] - 0.5) * 900.];
        }
        if let Some(draft) = &mut self.noise.key_draft {
            draft.raw[key(target)]["x"] = json!(p[0]);
            draft.raw[key(target)]["y"] = json!(p[1]);
        }
    }
    pub(super) fn noise_sample_press(&mut self, pos: Pos2) -> bool {
        if self.noise.preview_toolbar.is_some_and(|r| r.contains(pos)) {
            return true;
        }
        if self.noise.preview_handles.iter().any(|r| r.contains(pos))
            && let Some(draft) = &self.noise.key_draft
        {
            self.noise.sample_drag = Some((draft.raw.clone(), pos));
            return true;
        }
        false
    }
    pub(super) fn noise_sample_move(&mut self, pos: Pos2) -> bool {
        let Some((original, origin)) = &self.noise.sample_drag else {
            return false;
        };
        let Some(target) = self.noise.sample_target else {
            return false;
        };
        if pos.distance(*origin) < 3. {
            if let Some(draft) = &mut self.noise.key_draft {
                draft.raw = original.clone();
            }
            return true;
        }
        let point = self.noise_sample_point(original, target);
        let point = project(self.preview.rect, point, self.noise_format()) + (pos - *origin);
        self.set_noise_sample_point(point);
        true
    }
    pub(crate) fn noise_sample_release(&mut self, pos: Pos2) {
        if let Some(drag) = self.preview.drag.take()
            && !drag.moved
            && self.preview.rect.contains(pos)
        {
            self.set_noise_sample_point(pos);
        }
    }
    pub(crate) fn noise_sample_input(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let Some(target) = self.noise.sample_target else {
            return;
        };
        self.paint_noise_preview_grid(ui, rect);
        self.noise.preview_handles.clear();
        if let Some(draft) = &self.noise.key_draft {
            let point = self.noise_sample_point(&draft.raw, target);
            let position = project(rect, point, self.noise_format());
            let color = if target == SampleTarget::Anchor {
                hold()
            } else {
                accent()
            };
            let painter = ui.painter_at(rect);
            painter.circle_filled(position, 9., color);
            painter.circle_stroke(position, 12., Stroke::new(2_f32, color));
            let label = crate::i18n::tr(if target == SampleTarget::Anchor {
                "Anchor"
            } else {
                "Position"
            });
            let text = format!("{label} [{:.3}, {:.3}]", point[0], point[1]);
            let text_pos = position + vec2(17., 17.);
            let galley = painter.layout_no_wrap(text, FontId::proportional(13.), color);
            let text_pos = egui::pos2(
                text_pos
                    .x
                    .min(rect.right() - galley.size().x - 4.)
                    .max(rect.left() + 4.),
                text_pos
                    .y
                    .min(rect.bottom() - galley.size().y - 4.)
                    .max(rect.top() + 4.),
            );
            painter.rect_filled(
                Rect::from_min_size(text_pos, galley.size()).expand(3.),
                2.,
                panel(),
            );
            painter.galley(text_pos, galley);
            self.noise
                .preview_handles
                .push(Rect::from_center_size(position, vec2(44., 44.)));
        }
        let (done, cancel) = self.noise_preview_toolbar(
            ui,
            rect,
            "Apply field",
            "Tap to place or drag the point; swipe elsewhere to scrub time",
        );
        if done {
            self.apply_noise_sample();
        } else if cancel {
            self.close_fullscreen_preview();
        }
    }
    pub(crate) fn apply_noise_sample(&mut self) -> bool {
        if !self.flush_noise_draft() {
            return false;
        }
        self.noise.sample_original = None;
        self.close_fullscreen_preview();
        true
    }
}
