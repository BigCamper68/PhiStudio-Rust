use super::ImeGate;
#[test]
fn actual_text_cursor_output_distinguishes_restored_and_button_focus_from_a_field_tap() {
    let ctx = egui::Context::default();
    let mut value = String::new();
    let mut gate = ImeGate::default();
    let mut rect = egui::Rect::NOTHING;
    let raw = |events| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(600., 400.),
        )),
        events,
        ..Default::default()
    };
    let out = ctx.run(raw(vec![]), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.button("A button").request_focus();
            rect = ui.text_edit_singleline(&mut value).rect;
        });
    });
    assert!(ctx.wants_keyboard_input());
    assert!(out.platform_output.text_cursor_pos.is_none());
    assert_eq!(gate.update(false, false, false), None);
    for _ in 0..2 {
        let _ = ctx.run(raw(vec![]), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let _ = ui.button("A button");
                ui.text_edit_singleline(&mut value).request_focus();
            });
        });
    }
    let out = ctx.run(raw(vec![]), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let _ = ui.button("A button");
            rect = ui.text_edit_singleline(&mut value).rect;
        });
    });
    assert!(out.platform_output.text_cursor_pos.is_some());
    assert_eq!(gate.update(true, false, false), None);
    let events = [true, false]
        .into_iter()
        .map(|pressed| egui::Event::PointerButton {
            pos: rect.center(),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        })
        .collect();
    let out = ctx.run(raw(events), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let _ = ui.button("A button");
            ui.text_edit_singleline(&mut value);
        });
    });
    assert!(out.platform_output.text_cursor_pos.is_some());
    assert_eq!(gate.update(true, true, false), Some(true));
    assert_eq!(gate.update(true, false, true), Some(false));
}
