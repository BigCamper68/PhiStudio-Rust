use super::*;
use egui::{Align2, ScrollArea, TextEdit, pos2, vec2};

fn draw(ctx: &Context, viewport: Vec2, restored: bool, anchored: bool) -> Rect {
    let mut rect = Rect::NOTHING;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, viewport)),
            ..Default::default()
        },
        |ctx| {
            let contents = |ui: &mut Ui| {
                ui.set_min_width(ui.available_width());
                ScrollArea::vertical()
                    .max_height(ui.available_height())
                    .show(ui, |ui| {
                        let mut value = "Keyboard input".to_owned();
                        ui.add(
                            TextEdit::singleline(&mut value)
                                .id(Id::new("editor"))
                                .desired_width(120.),
                        );
                        for _ in 0..40 {
                            ui.label("Window content");
                        }
                    });
            };
            if restored {
                let mut window = Window::new("Window", Id::new("window"))
                    .default_pos([320., 160.])
                    .default_size([400., 360.])
                    .resize(|r| r.max_size([viewport.x - 48., (viewport.y - 100.).max(100.)]));
                if anchored {
                    window = window.anchor(Align2::CENTER_CENTER, Vec2::ZERO);
                }
                rect = window.show(ctx, contents).unwrap().response.rect;
            } else {
                let mut window = egui::Window::new("Window")
                    .id(Id::new("window"))
                    .default_pos([320., 160.])
                    .default_size([400., 360.])
                    .resize(|r| r.max_size([viewport.x - 48., (viewport.y - 100.).max(100.)]));
                if anchored {
                    window = window.anchor(Align2::CENTER_CENTER, Vec2::ZERO);
                }
                rect = window.show(ctx, contents).unwrap().response.rect;
            }
        },
    );
    rect
}

#[test]
fn windows_recover_pre_keyboard_size_and_position_after_animated_insets() {
    let mut vanilla_loses_size = false;
    for (viewport, keyboard_height) in [(vec2(873., 393.), 180.), (vec2(1194., 834.), 348.)] {
        for anchored in [false, true] {
            for restored in [false, true] {
                let ctx = Context::default();
                ctx.set_style(crate::theme::style());
                let mut original = Rect::NOTHING;
                for _ in 0..5 {
                    original = draw(&ctx, viewport, restored, anchored);
                }
                ctx.memory_mut(|m| m.request_focus(Id::new("editor")));
                for fraction in [0.25, 0.5, 0.75, 1., 1., 1.] {
                    draw(
                        &ctx,
                        viewport - vec2(0., keyboard_height * fraction),
                        restored,
                        anchored,
                    );
                }
                let mut returned = Rect::NOTHING;
                for fraction in [0.75, 0.5, 0.25, 0., 0., 0.] {
                    returned = draw(
                        &ctx,
                        viewport - vec2(0., keyboard_height * fraction),
                        restored,
                        anchored,
                    );
                }
                if restored {
                    assert!(
                        (returned.size() - original.size()).length() <= 1.,
                        "Size: {viewport:?}/{anchored}: {original:?} -> {returned:?}"
                    );
                    assert!(
                        (returned.min - original.min).length() <= 1.,
                        "Position: {original:?} -> {returned:?}"
                    );
                } else {
                    // The same real egui window without the safeguard loses its size.
                    vanilla_loses_size |= returned.height() < original.height() - 20.;
                }
            }
        }
    }
    assert!(
        vanilla_loses_size,
        "The regression fixture must expose the original egui shrink"
    );
}

#[test]
fn rotation_during_keyboard_input_keeps_new_windows_inside_the_new_viewport() {
    let ctx = Context::default();
    let landscape = vec2(1194., 834.);
    for _ in 0..4 {
        draw(&ctx, landscape, true, false);
    }
    ctx.memory_mut(|m| m.request_focus(Id::new("editor")));
    draw(&ctx, vec2(1194., 486.), true, false);
    let portrait = vec2(834., 1194.);
    let mut rect = Rect::NOTHING;
    for _ in 0..4 {
        rect = draw(&ctx, portrait, true, false);
    }
    assert!(
        Rect::from_min_size(pos2(0., 0.), portrait)
            .expand(1.)
            .contains_rect(rect)
    );
}
