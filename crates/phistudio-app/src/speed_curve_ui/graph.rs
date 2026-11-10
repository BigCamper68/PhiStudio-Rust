mod input;
use super::*;
use crate::{i18n::tr, theme};
use egui::{Align2, FontId, Pos2, Rect, Shape, Stroke, pos2, vec2};
pub(super) use input::PointerGesture;
struct View {
    rect: Rect,
    start: f64,
    span: f64,
    limit: f64,
}
impl View {
    fn point(&self, beat: f64, distance: f64) -> Pos2 {
        pos2(
            self.rect.left() + ((beat - self.start) / self.span) as f32 * self.rect.width(),
            self.rect.center().y - (distance / self.limit) as f32 * self.rect.height() * 0.5,
        )
    }
    fn world(&self, pos: Pos2) -> [f64; 2] {
        [
            self.start + f64::from((pos.x - self.rect.left()) / self.rect.width()) * self.span,
            f64::from((self.rect.center().y - pos.y) / (self.rect.height() * 0.5)) * self.limit,
        ]
    }
    fn nearest(&self, points: &[SpeedCurvePoint], pos: Pos2) -> Option<usize> {
        points
            .iter()
            .enumerate()
            .map(|(i, p)| (i, self.point(p.beat.as_f64(), p.distance).distance(pos)))
            .filter(|(_, d)| *d <= 16.)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|p| p.0)
    }
}
pub(super) fn distance_at(profile: &SpeedCurveProfile, beat: f64) -> f64 {
    let samples = &profile.samples;
    let index = samples
        .partition_point(|p| p[0] < beat)
        .min(samples.len() - 1);
    if index == 0 {
        return samples[0][1];
    }
    let a = samples[index - 1];
    let b = samples[index];
    let u = ((beat - a[0]) / (b[0] - a[0])).clamp(0., 1.);
    a[1] + (b[1] - a[1]) * u
}
pub(super) fn canvas(
    ui: &mut egui::Ui,
    state: &mut State,
    division: i32,
    cursor: f64,
    height: f32,
) -> bool {
    let (outer, response) = ui.allocate_exact_size(
        vec2(ui.available_width().max(1.), height),
        egui::Sense::click_and_drag(),
    );
    let rect = Rect::from_min_max(outer.min + vec2(52., 20.), outer.max - vec2(12., 26.));
    if rect.width() < 16. || rect.height() < 16. {
        return false;
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(egui::Id::new("speed-curve-plot"), rect));
    let view = View {
        rect,
        start: state.start,
        span: state.span.max(1e-9),
        limit: state.limit.max(1.),
    };
    let painter = ui.painter_at(outer);
    painter.rect_filled(outer, 4., theme::base());
    let plot = ui.painter_at(rect);
    let step = 2_f64.powf((view.span / 8.).log2().ceil()).max(0.03125);
    let first = (view.start / step).ceil();
    for i in 0..=12 {
        let beat = (first + i as f64) * step;
        if beat > view.start + view.span {
            break;
        }
        let x = view.point(beat, 0.).x;
        plot.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(1_f32, theme::border()),
        );
        painter.text(
            pos2(x, rect.bottom() + 5.),
            Align2::CENTER_TOP,
            format!("{beat:.2}")
                .trim_end_matches('0')
                .trim_end_matches('.'),
            FontId::proportional(10.),
            theme::muted(),
        );
    }
    for i in -2..=2 {
        let y = view.limit * i as f64 / 2.;
        let pos = view.point(view.start, y);
        plot.line_segment(
            [pos2(rect.left(), pos.y), pos2(rect.right(), pos.y)],
            Stroke::new(1_f32, theme::border()),
        );
        painter.text(
            pos2(rect.left() - 5., pos.y),
            Align2::RIGHT_CENTER,
            format!("{y:.0}"),
            FontId::proportional(10.),
            theme::muted(),
        );
    }
    painter.text(
        outer.min + vec2(5., 3.),
        Align2::LEFT_TOP,
        tr("Distance"),
        FontId::proportional(11.),
        theme::text(),
    );
    painter.text(
        pos2(outer.right() - 6., outer.bottom() - 2.),
        Align2::RIGHT_BOTTOM,
        tr("Beat"),
        FontId::proportional(10.),
        theme::muted(),
    );
    if let Some(profile) = &state.profile {
        for beat in &profile.bpm_markers {
            let x = view.point(*beat, 0.).x;
            if x >= rect.left() && x <= rect.right() {
                plot.line_segment(
                    [pos2(x, rect.top()), pos2(x, rect.bottom())],
                    Stroke::new(1_f32, theme::muted()),
                );
                painter.text(
                    pos2(x + 3., rect.top() + 2.),
                    Align2::LEFT_TOP,
                    "BPM",
                    FontId::proportional(9.),
                    theme::muted(),
                );
            }
        }
        let map = |data: &[[f64; 2]]| {
            data.iter()
                .map(|p| view.point(p[0], p[1]))
                .collect::<Vec<_>>()
        };
        plot.add(Shape::line(
            map(&profile.samples),
            Stroke::new(
                1.5_f32,
                if state.preview.is_some() {
                    theme::muted()
                } else {
                    theme::accent()
                },
            ),
        ));
        if let Some(preview) = &state.preview {
            plot.add(Shape::line(
                map(&preview.samples),
                Stroke::new(2_f32, theme::accent()),
            ));
        } else if state.drag.is_some() {
            plot.add(Shape::line(
                state
                    .points
                    .iter()
                    .map(|p| view.point(p.beat.as_f64(), p.distance))
                    .collect(),
                Stroke::new(1_f32, theme::drag()),
            ));
        }
        let hit = view.point(profile.judgment, profile.judgment_distance);
        if hit.x >= rect.left() && hit.x <= rect.right() {
            for i in 0..(rect.height() / 8.).ceil() as usize {
                let y = rect.top() + i as f32 * 8.;
                plot.line_segment(
                    [pos2(hit.x, y), pos2(hit.x, (y + 4.).min(rect.bottom()))],
                    Stroke::new(1_f32, theme::accent()),
                );
            }
        }
    }
    let x = view.point(cursor, 0.).x;
    if x >= rect.left() && x <= rect.right() {
        plot.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(1_f32, theme::text()),
        );
    }
    for (i, point) in state.points.iter().enumerate() {
        let pos = view.point(point.beat.as_f64(), point.distance);
        if !rect.expand(8.).contains(pos) {
            continue;
        }
        let selected = state.selected == Some(i);
        plot.circle_filled(
            pos,
            if selected { 6. } else { 4.5 },
            if point.fixed {
                theme::muted()
            } else {
                theme::accent()
            },
        );
        plot.circle_stroke(
            pos,
            if selected { 8. } else { 5.5 },
            Stroke::new(1_f32, theme::base()),
        );
        if selected {
            plot.text(
                pos + vec2(0., -12.),
                Align2::CENTER_BOTTOM,
                format!("{} · {:.1}", point.beat, point.distance),
                FontId::proportional(11.),
                theme::text(),
            );
        }
    }
    if state.busy() {
        return false;
    }
    if response.hovered() {
        let (scroll, zoom) =
            ui.input(|i| (i.scroll_delta, i.modifiers.ctrl || i.modifiers.command));
        if scroll != egui::Vec2::ZERO {
            if zoom {
                state.span = (state.span * (f64::from(scroll.y) * -0.005).exp()).clamp(
                    (state.range[1].as_f64() - state.range[0].as_f64()).min(0.05),
                    state.range[1].as_f64() - state.range[0].as_f64(),
                );
            } else {
                state.start -= f64::from(scroll.x + scroll.y) * state.span / 600.;
            }
            state.start = state.start.clamp(
                state.range[0].as_f64(),
                (state.range[1].as_f64() - state.span).max(state.range[0].as_f64()),
            );
        }
    }
    input::handle(ui, state, &view, division)
}
