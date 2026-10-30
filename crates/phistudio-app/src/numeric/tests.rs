use super::*;
use egui::{Context, Event, Key, Modifiers, RawInput, Rect, pos2, vec2};

fn frame(ctx: &Context, value: &mut f64, events: Vec<Event>) -> (Response, egui::FullOutput) {
    let mut response = None;
    let output = ctx.run(
        RawInput {
            events,
            screen_rect: Some(Rect::from_min_size(pos2(0., 0.), vec2(600., 300.))),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                response = Some(ui.add(Value::new(value).speed(0.2)));
                let next = ui.button("Next").id;
                ui.data_mut(|d| d.insert_temp(egui::Id::new("next-control"), next));
            });
        },
    );
    (response.unwrap(), output)
}
fn key(key: Key) -> Event {
    Event::Key {
        key,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}
fn enter_text(
    ctx: &Context,
    value: &mut f64,
    text: &str,
    finish: Key,
) -> (Response, egui::FullOutput) {
    let (response, _) = frame(ctx, value, vec![]);
    ctx.memory_mut(|m| m.request_focus(response.id));
    frame(ctx, value, vec![]);
    let mut state = egui::TextEdit::load_state(ctx, response.id).unwrap();
    state.set_ccursor_range(Some(egui::text::CCursorRange::two(
        egui::text::CCursor::new(0),
        egui::text::CCursor::new(100),
    )));
    state.store(ctx, response.id);
    frame(ctx, value, vec![Event::Text(text.into())]);
    frame(ctx, value, vec![key(finish)])
}

#[test]
fn real_egui_text_input_commits_once_on_enter_rejects_invalid_and_cancels_escape() {
    let ctx = Context::default();
    let mut value = 12.;
    let (response, _) = enter_text(&ctx, &mut value, "675/2", Key::Enter);
    assert_eq!(value, 337.5);
    assert!(response.changed() && response.lost_focus());
    let next = ctx.data_mut(|d| d.get_temp::<egui::Id>(egui::Id::new("next-control")));
    let (response, output) = enter_text(&ctx, &mut value, "sqrt(-1)", Key::Enter);
    assert_eq!(value, 337.5);
    assert!(!response.changed());
    assert_eq!(
        ctx.data_mut(|d| d.get_temp::<egui::Id>(egui::Id::new("next-control"))),
        next
    );
    assert!(output.shapes.iter().any(|shape| matches!(&shape.1,
        egui::Shape::Text(t) if t.galley.job.text == ERROR)));
    enter_text(&ctx, &mut value, "123*4", Key::Escape);
    assert_eq!(value, 337.5);
    enter_text(&ctx, &mut value, "360/7", Key::Enter);
    assert!((value - 360. / 7.).abs() < 1e-10);
}

#[test]
fn numeric_types_and_ranges_keep_native_semantics() {
    assert_eq!(parse::<u16>("128*2"), Some(256.));
    assert_eq!(parse::<u16>("1/2"), None);
    assert_eq!(parse::<f32>("1e100"), None);
    assert_eq!(parse::<f64>("0*t"), None);
    let ctx = Context::default();
    let mut value = 0.;
    enter_text(&ctx, &mut value, "1+2", Key::Enter);
    // Formula input still uses the same response ID as native DragValue, so
    // focus, the IME adapter and existing per-field Undo commit paths survive.
    let (response, _) = frame(&ctx, &mut value, vec![]);
    let mut native = None;
    let _ = ctx.run(RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            native = Some(ui.add(DragValue::new(&mut value).speed(0.2)).id);
        });
    });
    assert_eq!(Some(response.id), native);
}

fn drag(
    value: f64,
    delta: egui::Vec2,
    options: phistudio_core::NumericDragOptions,
    frames: usize,
) -> f64 {
    let ctx = Context::default();
    configure(&ctx, options);
    let mut value = value;
    let (r, _) = frame(&ctx, &mut value, vec![]);
    let p = r.rect.center();
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(
        &ctx,
        &mut value,
        vec![Event::PointerMoved(p), button(p, true)],
    );
    for i in 1..=frames {
        frame(
            &ctx,
            &mut value,
            vec![Event::PointerMoved(p + delta * i as f32 / frames as f32)],
        );
    }
    frame(&ctx, &mut value, vec![button(p + delta, false)]);
    value
}
#[test]
fn horizontal_drags_honor_disable_sensitivity_and_retain_exact_typed_formulas() {
    let defaults = phistudio_core::NumericDragOptions::default();
    let ordinary = drag(0., vec2(100., 0.), defaults, 10);
    assert!((ordinary - 20.).abs() < 1e-8, "{ordinary}");
    assert_eq!(drag(12., vec2(0., -100.), defaults, 10), 12.);
    assert_eq!(
        drag(
            12.,
            vec2(100., 0.),
            phistudio_core::NumericDragOptions {
                enabled: false,
                ..defaults
            },
            10
        ),
        12.
    );
    assert!(
        (drag(
            0.,
            vec2(100., 0.),
            phistudio_core::NumericDragOptions {
                sensitivity: 2.,
                ..defaults
            },
            10
        ) - 2. * ordinary)
            .abs()
            < 1e-8
    );
    let ctx = Context::default();
    configure(
        &ctx,
        phistudio_core::NumericDragOptions {
            enabled: false,
            snap: true,
            ..defaults
        },
    );
    let mut value = 12.;
    enter_text(&ctx, &mut value, "1/8", Key::Enter);
    assert_eq!(value, 0.125);
}
#[test]
fn snapped_drags_accumulate_substeps_and_work_on_negative_and_fractional_values() {
    let options = phistudio_core::NumericDragOptions {
        snap: true,
        ..Default::default()
    };
    assert_eq!(drag(0., vec2(100., 0.), options, 100), 20.);
    assert_eq!(drag(0., vec2(-100., 0.), options, 100), -20.);
    assert_eq!(
        drag(12., vec2(0., 0.), options, 1),
        12.,
        "A tap must not snap the value"
    );
    let fraction = drag(
        0.1,
        vec2(1., 0.),
        phistudio_core::NumericDragOptions {
            step: 0.05,
            ..options
        },
        10,
    );
    assert!((fraction - 0.3).abs() < 1e-7, "{fraction}");
}
