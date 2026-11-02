//! Size the native canvas for physical display pixels, retaining its 16:9 UVs.
use crate::{Preview, picking::PaintSnapshots};
use macroquad::prelude::*;

pub(crate) fn target_size(display: [f32; 2], aspect: f32, maximum: u32) -> Option<[u32; 2]> {
    if display.iter().any(|v| !v.is_finite() || *v <= 0.) || !aspect.is_finite() || aspect <= 0. {
        return None;
    }
    let (width, height) = if aspect > 16. / 9. {
        (1280., 1280. / aspect)
    } else {
        (720. * aspect, 720.)
    };
    let needed = (display[0] / width).max(display[1] / height).max(1.);
    let maximum = maximum.clamp(1, 4096) as f32;
    let cap = (maximum / 1280.)
        .min(maximum / 720.)
        .min((8_388_608_f32 / (1280. * 720.)).sqrt());
    let scale = ((needed * 8.).ceil() / 8.).min((cap * 8.).floor() / 8.);
    Some([
        ((1280. * scale).round() as u32).clamp(1, maximum as u32),
        ((720. * scale).round() as u32).clamp(1, maximum as u32),
    ])
}
impl Preview {
    pub fn render_size(&self) -> [u32; 2] {
        [
            self.target.texture.width() as u32,
            self.target.texture.height() as u32,
        ]
    }
    pub fn target_viewport(&self) -> (i32, i32, i32, i32) {
        let [w, h] = self.render_size();
        (0, 0, w as i32, h as i32)
    }
    /// Call before building egui shapes: replacing a texture after UI creation
    /// would leave that frame referring to its retired GL name.
    pub fn fit_display(&mut self, physical_size: [f32; 2], aspect: f32) -> bool {
        if self.maximum_target_edge == 0 {
            let mut maximum = 0;
            unsafe {
                macroquad::miniquad::gl::glGetIntegerv(
                    macroquad::miniquad::gl::GL_MAX_TEXTURE_SIZE,
                    &mut maximum,
                );
            }
            self.maximum_target_edge = if maximum > 0 { maximum as u32 } else { 2048 };
        }
        let Some([w, h]) = target_size(physical_size, aspect, self.maximum_target_edge) else {
            return false;
        };
        if self.render_size() == [w, h] {
            return false;
        }
        unsafe { get_internal_gl() }.flush();
        let next = render_target(w, h);
        let retired = std::mem::replace(&mut self.target, next);
        self.picking.resize(w, h);
        self.resource.last_vp = (0, 0, 0, 0);
        let viewport = self.target_viewport();
        self.resource.update_size(viewport);
        self.resource.camera.zoom.y = -self.resource.aspect_ratio;
        self.resource.camera.render_target = Some(self.effect_target());
        self.resize_paint_targets();
        // Present the last chart time immediately, without consuming input,
        // judging notes, moving the audio clock or aging particles twice.
        if self.last_time.is_finite() {
            let paints = PaintSnapshots::capture(&self.chart);
            self.chart.update(&mut self.resource);
            self.draw_frame(viewport, 0.);
            paints.restore(&self.chart);
        }
        retired.delete();
        true
    }
    fn resize_paint_targets(&mut self) {
        let (_, _, w, h) = self
            .resource
            .camera
            .viewport
            .unwrap_or(self.target_viewport());
        for line in &self.chart.lines {
            if let prpr::core::JudgeLineKind::Paint(_, state) = &line.kind {
                let mut state = state.borrow_mut();
                if let Some(pass) = state.0 {
                    let old = Texture2D::from_miniquad_texture(
                        pass.texture(unsafe { get_internal_gl() }.quad_context),
                    );
                    if old.width() as i32 == w && old.height() as i32 == h {
                        continue;
                    }
                    let pixels = old.get_texture_data();
                    let source = image::RgbaImage::from_raw(
                        pixels.width as u32,
                        pixels.height as u32,
                        pixels.bytes,
                    )
                    .unwrap();
                    let scaled = image::imageops::resize(
                        &source,
                        w as u32,
                        h as u32,
                        image::imageops::FilterType::Triangle,
                    );
                    let next = render_target(w as u32, h as u32);
                    next.texture.update(&Image {
                        width: w as u16,
                        height: h as u16,
                        bytes: scaled.into_raw(),
                    });
                    state.0 = Some(next.render_pass);
                    old.delete();
                    pass.delete(unsafe { get_internal_gl() }.quad_context);
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retina_fullscreen_and_ipad_aspects_have_enough_actual_pixels() {
        for (display, aspect) in [
            ([2388., 1343.25], 16. / 9.),
            ([2732., 1536.75], 16. / 9.),
            ([2732., 2048.], 4. / 3.),
            ([1920., 1080.], 16. / 9.),
        ] {
            let [w, h] = target_size(display, aspect, 16384).unwrap();
            let visible_w = (h as f32 * aspect).min(w as f32);
            let visible_h = (w as f32 / aspect).min(h as f32);
            assert!(
                visible_w >= display[0] && visible_h >= display[1],
                "{display:?}: {w}x{h}"
            );
            assert!(w as u64 * h as u64 <= 8_388_608);
        }
        assert_eq!(
            target_size([300., 168.75], 16. / 9., 4096),
            Some([1280, 720])
        );
        assert_eq!(
            target_size([100000., 80000.], 4. / 3., 2048),
            Some([1920, 1080])
        );
        assert_eq!(target_size([f32::NAN, 800.], 16. / 9., 4096), None);
    }
}
