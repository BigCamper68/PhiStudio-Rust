use super::*;
use crate::theme;
use egui::{Align2, FontId, Pos2, Rect, Stroke, pos2, vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Bounds {
    pub lo: [f64; 2],
    pub span: [f64; 2],
}
impl Default for Bounds {
    fn default() -> Self {
        Self {
            lo: [-0.1, -0.15],
            span: [1.2, 1.3],
        }
    }
}
impl Bounds {
    pub fn fit(curve: &Curve) -> Self {
        let (mut lo, mut hi) = ([0_f64, 0_f64], [1_f64, 1_f64]);
        if let Ok(segments) = curve.segments() {
            for segment in segments {
                for point in segment.points {
                    for axis in 0..2 {
                        lo[axis] = lo[axis].min(point[axis]);
                        hi[axis] = hi[axis].max(point[axis]);
                    }
                }
            }
        }
        let span = [(hi[0] - lo[0]).max(0.1), (hi[1] - lo[1]).max(0.1)];
        Self {
            lo: [lo[0] - span[0] * 0.1, lo[1] - span[1] * 0.1],
            span: [span[0] * 1.2, span[1] * 1.2],
        }
    }
    pub fn clamp(&mut self) {
        for axis in 0..2 {
            self.span[axis] = self.span[axis].clamp(0.0001, 40_000.);
            self.lo[axis] = self.lo[axis].clamp(-20_000., 20_000.);
        }
    }
}
#[derive(Clone, Copy)]
pub(super) struct View {
    pub rect: Rect,
    pub bounds: Bounds,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Hit {
    Node(usize),
    Handle { index: usize, outgoing: bool },
}
impl View {
    pub fn point(&self, p: [f64; 2]) -> Pos2 {
        pos2(
            self.rect.left()
                + ((p[0] - self.bounds.lo[0]) / self.bounds.span[0]) as f32 * self.rect.width(),
            self.rect.bottom()
                - ((p[1] - self.bounds.lo[1]) / self.bounds.span[1]) as f32 * self.rect.height(),
        )
    }
    pub fn world(&self, p: Pos2) -> [f64; 2] {
        [
            self.bounds.lo[0]
                + f64::from((p.x - self.rect.left()) / self.rect.width()) * self.bounds.span[0],
            self.bounds.lo[1]
                + f64::from((self.rect.bottom() - p.y) / self.rect.height()) * self.bounds.span[1],
        ]
    }
    pub fn hit(&self, draft: &Draft, pos: Pos2) -> Option<Hit> {
        let node = draft
            .curve
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (i, self.point(n.point()).distance(pos)))
            .filter(|(_, d)| *d <= 14.)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = node {
            return Some(Hit::Node(i));
        }
        let material = draft.curve.materialized().ok()?;
        let mut nearest = None;
        for &index in &draft.selected {
            let node = material.nodes.get(index)?;
            for (outgoing, handle) in [(false, node.incoming), (true, node.outgoing)] {
                let Some(handle) = handle else { continue };
                let distance = self.point(handle).distance(pos);
                if distance <= 17. && nearest.is_none_or(|(_, old)| distance < old) {
                    nearest = Some((Hit::Handle { index, outgoing }, distance));
                }
            }
        }
        nearest.map(|(hit, _)| hit)
    }
}

pub(super) fn canvas(ui: &mut egui::Ui, state: &mut State, height: f32) {
    let (outer, response) = ui.allocate_exact_size(
        vec2(ui.available_width().max(1.), height),
        egui::Sense::click_and_drag(),
    );
    let rect = Rect::from_min_max(outer.min + vec2(38., 12.), outer.max - vec2(12., 26.));
    if rect.width() < 20. || rect.height() < 20. {
        return;
    }
    let mut view = View {
        rect,
        bounds: state.view,
    };
    if !ui.is_enabled() {
        state.cancel_gesture();
    } else {
        input::handle(ui, state, &view, &response);
    }
    view.bounds = state.view;
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("custom-easing-plot"), rect));
    let painter = ui.painter_at(outer);
    painter.rect_filled(outer, 4., theme::base());
    let plot = ui.painter_at(rect);
    for axis in 0..2 {
        let raw = state.view.span[axis] / 6.;
        let power = 10_f64.powf(raw.log10().floor());
        let step = [1., 2., 5., 10.]
            .into_iter()
            .map(|n| n * power)
            .find(|n| *n >= raw)
            .unwrap_or(power * 10.);
        let first = (state.view.lo[axis] / step).ceil();
        for i in 0..=10 {
            let n = (first + f64::from(i)) * step;
            if n > state.view.lo[axis] + state.view.span[axis] {
                break;
            }
            let p = if axis == 0 {
                view.point([n, 0.])
            } else {
                view.point([0., n])
            };
            let line = if axis == 0 {
                [pos2(p.x, rect.top()), pos2(p.x, rect.bottom())]
            } else {
                [pos2(rect.left(), p.y), pos2(rect.right(), p.y)]
            };
            plot.line_segment(
                line,
                Stroke::new(
                    1_f32,
                    if n.abs() < step * 0.01 {
                        theme::muted()
                    } else {
                        theme::border()
                    },
                ),
            );
            painter.text(
                if axis == 0 {
                    pos2(p.x, rect.bottom() + 4.)
                } else {
                    pos2(rect.left() - 4., p.y)
                },
                if axis == 0 {
                    Align2::CENTER_TOP
                } else {
                    Align2::RIGHT_CENTER
                },
                egui::emath::format_with_decimals_in_range(n, 0..=4),
                FontId::proportional(10.),
                theme::muted(),
            );
        }
    }
    if let Some(request) = &state.request {
        let domain = request.domain;
        if domain != [0., 1.] {
            let a = view.point([domain[0].min(domain[1]), 0.]).x;
            let b = view.point([domain[0].max(domain[1]), 0.]).x;
            plot.rect_filled(
                Rect::from_min_max(pos2(a, rect.top()), pos2(b, rect.bottom())),
                0.,
                theme::accent().linear_multiply(0.08),
            );
        }
    }
    if let Ok(segments) = state.draft.curve.segments() {
        for (index, segment) in segments.into_iter().enumerate() {
            if segment.hold {
                let a = segment.points[0];
                let b = state.draft.curve.nodes[index + 1].point();
                plot.line_segment(
                    [view.point(a), view.point([b[0], a[1]])],
                    Stroke::new(2_f32, theme::accent()),
                );
                plot.line_segment(
                    [view.point([b[0], a[1]]), view.point(b)],
                    Stroke::new(1_f32, theme::muted()),
                );
            } else {
                plot.add(egui::Shape::line(
                    (0..=32)
                        .map(|i| view.point(segment.point(f64::from(i) / 32.)))
                        .collect(),
                    Stroke::new(2_f32, theme::accent()),
                ));
            }
        }
    }
    if let Ok(material) = state.draft.curve.materialized() {
        for &index in &state.draft.selected {
            let Some(node) = material.nodes.get(index) else {
                continue;
            };
            for handle in [node.incoming, node.outgoing].into_iter().flatten() {
                let p = view.point(handle);
                plot.line_segment(
                    [view.point(node.point()), p],
                    Stroke::new(1_f32, theme::muted()),
                );
                plot.circle_filled(p, 5., theme::panel());
                plot.circle_stroke(p, 5., Stroke::new(1.5_f32, theme::drag()));
            }
        }
    }
    for (index, node) in state.draft.curve.nodes.iter().enumerate() {
        let p = view.point(node.point());
        let selected = state.draft.selected.contains(&index);
        plot.circle_filled(
            p,
            if selected { 6. } else { 4.5 },
            if selected {
                theme::accent()
            } else {
                theme::text()
            },
        );
        plot.circle_stroke(
            p,
            if selected { 8. } else { 5.5 },
            Stroke::new(1_f32, theme::base()),
        );
    }
    if let Ok(geometry) = state.draft.curve.geometry() {
        let p = view.point([state.preview, geometry.sample(state.preview)]);
        plot.circle_filled(p, 4., theme::text());
        plot.line_segment(
            [pos2(p.x, rect.top()), pos2(p.x, rect.bottom())],
            Stroke::new(1_f32, theme::text().linear_multiply(0.3)),
        );
    }
}
