//! Shared preview geometry: normalized curves use a square, XY uses equal units.
use egui::{Pos2, Rect, Ui, pos2, vec2};
pub(crate) fn square(ui: &mut Ui) -> Rect {
    let side = ui
        .available_width()
        .min(280.)
        .min(ui.clip_rect().height().max(64.))
        .max(1.);
    ui.allocate_exact_size(vec2(side, side), egui::Sense::hover())
        .0
}
pub(crate) struct Cartesian {
    center: Pos2,
    origin: [f64; 2],
    scale: f64,
}
impl Cartesian {
    pub fn fit(rect: Rect, lo: [f64; 2], hi: [f64; 2]) -> Self {
        let span = [(hi[0] - lo[0]).max(1e-9), (hi[1] - lo[1]).max(1e-9)];
        Self {
            center: rect.center(),
            origin: [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5],
            scale: (f64::from(rect.width()) / span[0]).min(f64::from(rect.height()) / span[1]),
        }
    }
    pub fn point(&self, p: [f64; 2]) -> Pos2 {
        pos2(
            self.center.x + ((p[0] - self.origin[0]) * self.scale) as f32,
            self.center.y - ((p[1] - self.origin[1]) * self.scale) as f32,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn circles_keep_equal_radii_in_wide_and_tall_preview_rectangles() {
        for size in [vec2(600., 104.), vec2(120., 300.), vec2(300., 200.)] {
            let rect = Rect::from_min_size(pos2(20., 30.), size);
            let map = Cartesian::fit(rect, [-675., -450.], [675., 450.]);
            let center = map.point([0., 0.]);
            let radius = center.distance(map.point([300., 0.]));
            for i in 0..64 {
                let t = i as f64 * std::f64::consts::TAU / 64.;
                let p = map.point([300. * t.cos(), 300. * t.sin()]);
                assert!((center.distance(p) - radius).abs() < 0.0001);
                assert!(rect.contains(p));
            }
        }
    }
}
