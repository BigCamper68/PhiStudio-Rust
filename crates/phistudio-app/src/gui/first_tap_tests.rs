use super::*;
#[test]
fn a_batched_touch_opens_a_number_on_the_next_frame_and_keeps_a_caret() {
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    let mut gate = ImeGate::default();
    let mut focus = None;
    let mut value = 123.0;
    let mut rect = egui::Rect::NOTHING;
    let raw = |events| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(400., 300.),
        )),
        events,
        ..Default::default()
    };
    let show = |ctx: &egui::Context, value: &mut f64, rect: &mut egui::Rect| {
        egui::CentralPanel::default().show(ctx, |ui| {
            *rect = ui.add(egui::DragValue::new(value)).rect;
        });
    };
    for _ in 0..2 {
        let _ = ctx.run(raw(vec![]), |ctx| show(ctx, &mut value, &mut rect));
    }
    let pos = rect.center();
    let events = vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerGone,
    ];
    let out = ctx.run(raw(events), |ctx| show(ctx, &mut value, &mut rect));
    collapse_focus_selection(&ctx, true, &mut focus);
    assert_eq!(
        gate.update(out.platform_output.text_cursor_pos.is_some(), true, false),
        None
    );
    let out = ctx.run(raw(vec![]), |ctx| show(ctx, &mut value, &mut rect));
    collapse_focus_selection(&ctx, true, &mut focus);
    assert_eq!(
        gate.update(out.platform_output.text_cursor_pos.is_some(), false, false),
        Some(true)
    );
    let id = ctx.memory(|m| m.focus()).unwrap();
    let range = egui::TextEdit::load_state(&ctx, id)
        .unwrap()
        .ccursor_range()
        .unwrap();
    assert_eq!(
        range.primary, range.secondary,
        "Opening a field must not select its contents"
    );
    assert_eq!(range.primary.index, 3);
    gate.update(false, false, true);
    assert_eq!(
        gate.update(true, false, false),
        None,
        "Restored focus must not open IME"
    );
}
#[test]
fn ime_resize_keeps_the_focused_editor_open_and_button_focus_does_not_request_ime() {
    let ctx = egui::Context::default();
    let mut value = "Text".to_owned();
    let show = |ctx: &egui::Context, value: &mut String| {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(220.);
                ui.text_edit_singleline(value).request_focus();
            });
        });
    };
    for _ in 0..3 {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(500., 400.),
                )),
                ..Default::default()
            },
            |ctx| show(ctx, &mut value),
        );
    }
    let out = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(500., 150.),
            )),
            ..Default::default()
        },
        |ctx| show(ctx, &mut value),
    );
    assert!(keyboard_text_ready(
        &ctx,
        out.platform_output.text_cursor_pos.is_some(),
        true
    ));
    let mut gate = ImeGate {
        visible: true,
        ..Default::default()
    };
    assert_eq!(
        gate.update(keyboard_text_ready(&ctx, false, true), false, false),
        None
    );
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("ordinary-button")));
    assert!(!keyboard_text_ready(&ctx, true, true));
    assert_eq!(
        gate.update(keyboard_text_ready(&ctx, true, true), true, false),
        Some(false)
    );
}
