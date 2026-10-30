//! Effective pending DPI shared by frame bounds and physical input.
pub(crate) fn pixels_per_point(input: &egui::RawInput, ctx: &egui::Context) -> f32 {
    input
        .pixels_per_point
        .unwrap_or_else(|| ctx.pixels_per_point())
}
pub(crate) fn pointer_position(
    input: &egui::RawInput,
    ctx: &egui::Context,
    x: f32,
    y: f32,
) -> egui::Pos2 {
    let scale = pixels_per_point(input, ctx);
    egui::pos2(x / scale, y / scale)
}
pub(crate) fn set_screen_size(input: &mut egui::RawInput, ctx: &egui::Context, size: (f32, f32)) {
    let points = egui::vec2(size.0, size.1) / pixels_per_point(input, ctx);
    input.screen_rect = Some(egui::Rect::from_min_size(Default::default(), points));
}

pub(crate) fn inset_screen_rect(input: &mut egui::RawInput, ctx: &egui::Context, insets: [f32; 4]) {
    let scale = pixels_per_point(input, ctx);
    if let Some(rect) = &mut input.screen_rect {
        let [left, top, right, bottom] =
            insets.map(|v| if v.is_finite() { v.max(0.) / scale } else { 0. });
        rect.min.x = (rect.min.x + left).min(rect.max.x);
        rect.min.y = (rect.min.y + top).min(rect.max.y);
        rect.max.x = (rect.max.x - right).max(rect.min.x);
        rect.max.y = (rect.max.y - bottom).max(rect.min.y);
    }
}

#[cfg(test)]
mod density_tests {
    use super::*;
    #[test]
    fn ipad_safe_area_and_keyboard_preserve_full_framebuffer_touch_coordinates() {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput {
            pixels_per_point: Some(2.),
            ..Default::default()
        };
        set_screen_size(&mut input, &ctx, (2388., 1668.));
        let full = input.screen_rect.unwrap();
        inset_screen_rect(&mut input, &ctx, [0.; 4]);
        assert_eq!(input.screen_rect.unwrap(), full);
        inset_screen_rect(&mut input, &ctx, [0., 48., 0., 696.]);
        assert_eq!(
            input.screen_rect.unwrap(),
            egui::Rect::from_min_max(egui::pos2(0., 24.), egui::pos2(1194., 486.))
        );
        assert_eq!(
            pointer_position(&input, &ctx, 200., 100.),
            egui::pos2(100., 50.)
        );
    }
    #[test]
    fn first_frame_and_density_changes_use_the_same_scale_for_layout_and_pointer() {
        let ctx = egui::Context::default();
        for scale in [2.75, 3.5, 2.475, 1.] {
            let mut input = egui::RawInput {
                pixels_per_point: Some(scale),
                ..Default::default()
            };
            set_screen_size(&mut input, &ctx, (2400., 1080.));
            let size = egui::vec2(2400. / scale, 1080. / scale);
            assert_eq!(input.screen_rect.unwrap().size(), size);
            let pos = pointer_position(&input, &ctx, 90. * scale, 98. * scale);
            assert!((pos - egui::pos2(90., 98.)).length() < 0.001);
            let mut clicked = false;
            for press in [None, Some(true), Some(false)] {
                if let Some(pressed) = press {
                    input.events.push(egui::Event::PointerMoved(pos));
                    input.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    });
                }
                let _ = ctx.run(input.take(), |ctx| {
                    assert_eq!(ctx.pixels_per_point(), scale);
                    assert_eq!(ctx.screen_rect().size(), size);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        clicked |= ui
                            .put(
                                egui::Rect::from_min_size(
                                    egui::pos2(30., 80.),
                                    egui::vec2(120., 36.),
                                ),
                                egui::Button::new("Tap target"),
                            )
                            .clicked();
                    });
                });
                set_screen_size(&mut input, &ctx, (2400., 1080.));
            }
            assert!(clicked, "Pointer missed after density changed to {scale}");
        }
    }
}
