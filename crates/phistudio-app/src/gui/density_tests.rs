use super::*;
#[test]
fn ui_percentage_uses_system_density_instead_of_deferred_egui_state() {
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(2.75);
    assert_eq!(
        ctx.pixels_per_point(),
        1.,
        "egui's setter is deferred until the frame starts"
    );
    assert_eq!(scaled_display_density(2.75, 1.), 2.75);
    assert_eq!(scaled_display_density(2.75, 0.8), 2.2);
    assert_eq!(scaled_display_density(3., 1.25), 3.75);
    assert_eq!(scaled_display_density(3.5, 1.), 3.5);
    assert_eq!(scaled_display_density(f32::NAN, f32::NAN), 1.);
}
