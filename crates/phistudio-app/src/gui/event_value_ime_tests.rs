use super::*;
use crate::{scroll::TouchScroll as _, touch::TouchRouter, workbench::Workbench};
use phistudio_core::{ChartDocument, EventId, EventType, LineEvent};

fn frame(
    ctx: &egui::Context,
    app: &mut Workbench,
    gate: &mut ImeGate,
    focus: &mut Option<egui::Id>,
    events: Vec<egui::Event>,
    time: &mut f64,
) -> (egui::FullOutput, Option<bool>) {
    *time += 0.02;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(440., 320.),
            )),
            time: Some(*time),
            events,
            ..Default::default()
        },
        |ctx| {
            crate::scroll::begin_frame(ctx);
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_source("event-value-ime")
                    .touch_show(ui, |ui| app.event_properties(ui));
            });
        },
    );
    let tap = ctx.input(|i| i.pointer.any_click());
    collapse_focus_selection(ctx, tap || gate.pending > 0, focus);
    let text = keyboard_text_ready(
        ctx,
        output.platform_output.text_cursor_pos.is_some(),
        gate.visible,
    );
    let request = gate.update(text, tap, false);
    (output, request)
}

#[test]
fn real_event_value_fields_request_ime_after_separate_and_batched_phone_taps() {
    for kind in EventType::ALL {
        for label in ["17", "29"] {
            for batched in [false, true] {
                let ctx = egui::Context::default();
                ctx.set_style(crate::theme::style());
                let mut document = ChartDocument::empty("Event Value IME", 120.)
                    .unwrap()
                    .as_json()
                    .clone();
                document["judgeLineList"][0]["eventLayers"][0][kind.key()] =
                    serde_json::json!([LineEvent {
                        start: 17.,
                        end: 29.,
                        ..Default::default()
                    }]);
                let mut app = Workbench::new(ChartDocument::from_value(document).unwrap(), None);
                app.select_event(Some(EventId {
                    line: 0,
                    layer: 0,
                    kind,
                    index: 0,
                }));
                let original = app.editor.document().as_json().clone();
                let revision = app.editor.revision();
                let mut gate = ImeGate::default();
                let mut focus = None;
                let mut touch = TouchRouter::default();
                let mut time = 0.;
                for _ in 0..3 {
                    frame(&ctx, &mut app, &mut gate, &mut focus, vec![], &mut time);
                }
                let (output, _) = frame(&ctx, &mut app, &mut gate, &mut focus, vec![], &mut time);
                let point = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.1 {
                        egui::Shape::Text(text)
                            if text.galley.job.text.parse::<f64>().ok()
                                == Some(label.parse().unwrap()) =>
                        {
                            let point = text.pos + text.galley.rect.center().to_vec2();
                            shape.0.contains(point).then_some(point)
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("Missing {kind:?} Value {label}"));
                let mut events = touch
                    .event_with_context(&ctx, egui::TouchPhase::Start, 1, point, 0.2)
                    .0;
                if !batched {
                    frame(&ctx, &mut app, &mut gate, &mut focus, events, &mut time);
                    events = vec![];
                }
                events.extend(
                    touch
                        .event_with_context(&ctx, egui::TouchPhase::End, 1, point, 0.3)
                        .0,
                );
                let (_, request) = frame(&ctx, &mut app, &mut gate, &mut focus, events, &mut time);
                let mut opened = request == Some(true);
                for _ in 0..3 {
                    let (_, request) = frame(
                        &ctx,
                        &mut app,
                        &mut gate,
                        &mut focus,
                        touch.frame_start(),
                        &mut time,
                    );
                    opened |= request == Some(true);
                }
                assert!(
                    opened && gate.visible,
                    "No IME request for {kind:?} Value {label}, batched={batched}"
                );
                assert!(
                    ctx.memory(|m| m.focus())
                        .is_some_and(|id| egui::TextEdit::load_state(&ctx, id).is_some())
                );
                assert_eq!(app.editor.document().as_json(), &original);
                assert_eq!(app.editor.revision(), revision);
            }
        }
    }
}
