use super::*;
use phistudio_core::{Beat, ChartDocument, Note};
use phistudio_plugins::Builtin;
use phistudio_sdk::*;
use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

struct Gate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
struct Stamp {
    gate: Option<Arc<Gate>>,
    seen: Arc<Mutex<Vec<InputBatch>>>,
    gesture: ToolGesture,
}
impl Plugin for Stamp {
    fn call(&mut self, r: &Request) -> Result<Response, String> {
        Ok(match r {
            Request::Activate { .. } => Response {
                contributions: vec![
                    Contribution::Tool {
                        id: "stamp".into(),
                        title: "Stamp test".into(),
                        surfaces: vec![
                            InputSurfaceKind::Notes,
                            InputSurfaceKind::Preview,
                            InputSurfaceKind::Track,
                        ],
                        buttons: default_input_buttons(),
                        gesture: self.gesture,
                    },
                    Contribution::Track {
                        id: "clock".into(),
                        title: "Stamp clock".into(),
                    },
                ],
                ..Default::default()
            },
            Request::Input { input, context, .. } => {
                assert!(context.chart.is_none());
                assert!(context.selection.is_null());
                self.seen.lock().unwrap().push(input.clone());
                if input.surface.kind == InputSurfaceKind::Preview {
                    return Ok(Response::default());
                }
                if input.phase() == Some(InputPhase::End) {
                    if let Some(gate) = &self.gate {
                        gate.entered.send(()).unwrap();
                        gate.release
                            .lock()
                            .unwrap()
                            .recv_timeout(Duration::from_secs(5))
                            .unwrap();
                    }
                    let p = &input.events.last().unwrap().position;
                    let note = Note {
                        start_time: Beat::new(
                            p.beat.unwrap()[0],
                            p.beat.unwrap()[1],
                            p.beat.unwrap()[2],
                        )
                        .unwrap(),
                        end_time: Beat::new(
                            p.beat.unwrap()[0],
                            p.beat.unwrap()[1],
                            p.beat.unwrap()[2],
                        )
                        .unwrap(),
                        x: p.chart_x.unwrap(),
                        ..Default::default()
                    };
                    Response {
                        effects: vec![Effect::Edit {
                            transaction: Transaction {
                                version: context.version,
                                label: "Plugin stamp".into(),
                                operations: vec![PatchOperation::Add {
                                    path: format!("/judgeLineList/{}/notes/-", context.line),
                                    value: json!(note),
                                }],
                            },
                        }],
                        ..Default::default()
                    }
                } else {
                    Response::default()
                }
            }
            _ => Response::default(),
        })
    }
}
fn app(
    gesture: ToolGesture,
    gate: Option<Arc<Gate>>,
    seen: Arc<Mutex<Vec<InputBatch>>>,
) -> Workbench {
    let manifest:Manifest=serde_json::from_value(json!({"api":1,"id":"test.stamp","name":"Stamp","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["input","timeline","document_edit"]})).unwrap();
    let mut app = Workbench::new(ChartDocument::empty("Stamp test", 120.).unwrap(), None);
    app.plugins.runtime = Some(
        Runtime::spawn(
            None,
            vec![Builtin {
                manifest,
                enabled: true,
                create: Arc::new(move || {
                    Box::new(Stamp {
                        gate: gate.clone(),
                        seen: seen.clone(),
                        gesture,
                    })
                }),
            }],
        )
        .unwrap(),
    );
    app.plugins.pending.insert(0, None);
    settle(&mut app);
    let origin = app.plugins.registry.plugins[0].origin.clone().unwrap();
    app.select_plugin_tool(&origin, Some("stamp"));
    app
}
fn settle(app: &mut Workbench) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.poll_plugins();
        if !app.plugins.working() {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(app.error.is_none(), "{:?}", app.error);
}
fn grid(app: &Workbench) -> Grid {
    crate::grid_input::geometry(Rect::from_min_size(Pos2::ZERO, egui::vec2(700., 480.)), app)
}
fn events(app: &mut Workbench, events: &[Event], canceled: bool) {
    let g = grid(app);
    crate::grid_input::handle(app, events, g, g.full_rect(), |_| true, canceled);
}
fn pointer(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn batched_sdk_gesture_uses_final_snapped_coordinates_and_one_shared_history_action() {
    let seen = Arc::new(Mutex::new(vec![]));
    let mut app = app(ToolGesture::Drag, None, seen.clone());
    app.transport.beat = 8.;
    let g = grid(&app);
    let down = egui::pos2(g.x(135.), g.y(10.));
    let up = egui::pos2(g.x(270.), g.y(11.));
    events(&mut app, &[pointer(down, true), pointer(up, false)], false);
    settle(&mut app);
    let note = app.editor.document().note(0, 0).unwrap();
    assert_eq!(note.x, 270.);
    assert_eq!(note.start_time, Beat::new(11, 0, 1).unwrap());
    assert_eq!(app.editor.history_rows(), vec![("Plugin stamp", true)]);
    assert_eq!(seen.lock().unwrap()[0].events.len(), 2);
    app.plugin_action(Action::Disable("test.stamp".into()));
    settle(&mut app);
    assert!(!app.plugin_tool_active());
    assert!(app.editor.undo());
    assert_eq!(app.editor.document().note_count(), 0);
    assert!(app.editor.redo());
    assert_eq!(app.editor.document().note_count(), 1);
    app.plugins.shutdown().unwrap();
}

#[test]
fn touch_taps_commit_once_and_swipes_yield_to_the_existing_navigation() {
    let mut app = app(ToolGesture::Tap, None, Arc::default());
    app.transport.beat = 8.;
    let g = grid(&app);
    let down = egui::pos2(g.x(135.), g.y(10.));
    let mut touch = crate::touch::TouchRouter::default();
    for phase in [egui::TouchPhase::Start, egui::TouchPhase::End] {
        let (ev, cancel) = touch.event(phase, 1, down);
        events(&mut app, &ev, cancel);
    }
    settle(&mut app);
    assert_eq!(app.editor.document().note_count(), 1);
    app.editor.mark_saved();
    let moved = down + egui::vec2(0., 96.);
    for (phase, pos) in [
        (egui::TouchPhase::Start, down),
        (egui::TouchPhase::Move, moved),
        (egui::TouchPhase::End, moved),
    ] {
        let (ev, cancel) = touch.event(phase, 2, pos);
        events(&mut app, &ev, cancel);
    }
    settle(&mut app);
    assert_eq!(app.editor.document().note_count(), 1);
    assert!(!app.editor.is_dirty());
    assert!((app.transport.beat - 9.).abs() < 1e-9);
    app.plugins.shutdown().unwrap();
}

#[test]
fn multitouch_and_cancellation_do_not_commit_or_promote_the_remaining_finger() {
    let mut app = app(ToolGesture::Drag, None, Arc::default());
    let g = grid(&app);
    let a = egui::pos2(g.x(0.), g.y(1.));
    let b = a + egui::vec2(50., 0.);
    let mut touch = crate::touch::TouchRouter::default();
    for (phase, id, pos) in [
        (egui::TouchPhase::Start, 1, a),
        (egui::TouchPhase::Start, 2, b),
        (egui::TouchPhase::End, 1, a),
        (egui::TouchPhase::Move, 2, b),
        (egui::TouchPhase::End, 2, b),
    ] {
        let (ev, cancel) = touch.event(phase, id, pos);
        events(&mut app, &ev, cancel);
    }
    settle(&mut app);
    assert!(!app.editor.is_dirty());
    assert!(!app.editor.can_undo());
    events(&mut app, &[pointer(a, true)], false);
    events(&mut app, &[pointer(a, false)], true);
    settle(&mut app);
    assert_eq!(app.editor.document().note_count(), 0);
    app.plugins.shutdown().unwrap();
}

#[test]
fn cancellation_preflight_rejects_an_end_result_that_is_already_waiting_for_ui() {
    let (entered, entry) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut app = app(
        ToolGesture::Drag,
        Some(Arc::new(Gate {
            entered,
            release: Mutex::new(gate),
        })),
        Arc::default(),
    );
    let p = egui::pos2(grid(&app).x(0.), grid(&app).y(1.));
    events(&mut app, &[pointer(p, true), pointer(p, false)], false);
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let completion = loop {
        if let Some(c) = app.plugins.runtime.as_ref().unwrap().poll().unwrap() {
            break c;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    };
    let effect = completion
        .result
        .unwrap()
        .effects
        .into_iter()
        .next()
        .unwrap();
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        app.plugin_input_preflight(ctx, true)
    });
    app.apply_plugin_effect(effect);
    assert!(!app.editor.is_dirty());
    assert!(!app.editor.can_undo());
    app.plugins.pending.remove(&completion.id);
    app.plugins.input.complete(completion.id);
    settle(&mut app);
    app.plugins.shutdown().unwrap();
}

#[test]
fn changing_to_a_builtin_tool_cancels_sdk_work_and_preserves_native_tool_placement() {
    let mut app = app(ToolGesture::Drag, None, Arc::default());
    let p = egui::pos2(grid(&app).x(0.), grid(&app).y(1.));
    events(&mut app, &[pointer(p, true)], false);
    app.set_tool(crate::workbench::Tool::Place(phistudio_core::NoteKind::Tap));
    events(&mut app, &[pointer(p, false)], false);
    settle(&mut app);
    assert_eq!(app.editor.document().note_count(), 0);
    events(&mut app, &[pointer(p, true), pointer(p, false)], false);
    assert_eq!(app.editor.document().note_count(), 1);
    assert!(!app.plugin_tool_active());
    app.plugins.shutdown().unwrap();
}

#[test]
fn held_modifiers_survive_pointer_moves_and_layout_changes_cancel_the_frozen_transform() {
    let seen = Arc::new(Mutex::new(vec![]));
    let mut app = app(ToolGesture::Drag, None, seen.clone());
    let p = egui::pos2(grid(&app).x(0.), grid(&app).y(1.));
    let shift = egui::Modifiers {
        shift: true,
        ..Default::default()
    };
    events(
        &mut app,
        &[Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: shift,
        }],
        false,
    );
    settle(&mut app);
    events(
        &mut app,
        &[Event::PointerMoved(p + egui::vec2(2., 2.))],
        false,
    );
    settle(&mut app);
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .all(|b| b.events.iter().all(|e| e.modifiers.shift))
    );
    app.timeline.split += 0.1;
    events(&mut app, &[pointer(p, false)], false);
    settle(&mut app);
    assert!(!app.editor.can_undo());
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .any(|b| b.phase() == Some(InputPhase::Cancel))
    );
    app.plugins.shutdown().unwrap();
}

#[test]
fn zoom_changes_and_focus_loss_revoke_results_before_the_ui_polls_them() {
    for focus_lost in [false, true] {
        let mut app = app(ToolGesture::Drag, None, Arc::default());
        let p = egui::pos2(grid(&app).x(0.), grid(&app).y(1.));
        events(&mut app, &[pointer(p, true), pointer(p, false)], false);
        let scope = app.plugins.input.capture.as_ref().unwrap().scope.clone();
        if !focus_lost {
            app.timeline.zoom *= 2.;
        }
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                has_focus: !focus_lost,
                ..Default::default()
            },
            |ctx| {
                app.plugin_input_preflight(ctx, false);
            },
        );
        assert!(!scope.is_active());
        settle(&mut app);
        assert!(!app.editor.is_dirty());
        assert!(!app.editor.can_undo());
        app.plugins.shutdown().unwrap();
    }
}

fn preview_frame(app: &mut Workbench, ctx: &egui::Context, events: Vec<Event>) {
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(960., 640.))),
            events,
            ..Default::default()
        },
        |ctx| {
            app.plugin_input_preflight(ctx, false);
            app.poll_plugins();
            app.fullscreen_preview(ctx);
        },
    );
}

#[test]
fn fullscreen_preview_routes_one_touch_stream_yields_swipes_and_keeps_the_exit_button() {
    let seen = Arc::new(Mutex::new(vec![]));
    let mut app = app(ToolGesture::Tap, None, seen.clone());
    app.open_fullscreen_preview();
    app.transport.beat = 8.;
    let ctx = egui::Context::default();
    preview_frame(&mut app, &ctx, vec![]);
    let rect = app.preview.rect;
    let p = rect.min + rect.size() * 0.5;
    let mut touch = crate::touch::TouchRouter::default();
    for phase in [egui::TouchPhase::Start, egui::TouchPhase::End] {
        let (ev, _) = touch.event(phase, 1, p);
        preview_frame(&mut app, &ctx, ev);
    }
    settle(&mut app);
    assert!(app.preview.pick.is_none());
    let batches = seen.lock().unwrap();
    assert_eq!(
        batches
            .iter()
            .filter(|b| b.phase() == Some(InputPhase::End))
            .count(),
        1
    );
    assert!(
        batches
            .iter()
            .all(|b| b.surface.kind == InputSurfaceKind::Preview)
    );
    assert_eq!(batches.last().unwrap().start.normalized, [0.5, 0.5]);
    drop(batches);
    let moved = p + egui::vec2(0., 96.);
    for (phase, pos) in [
        (egui::TouchPhase::Start, p),
        (egui::TouchPhase::Move, moved),
        (egui::TouchPhase::End, moved),
    ] {
        let (ev, _) = touch.event(phase, 2, pos);
        preview_frame(&mut app, &ctx, ev);
    }
    settle(&mut app);
    assert!((app.transport.beat - 9.).abs() < 1e-9);
    assert!(app.preview.pick.is_none());
    let exit = app.fullscreen_exit_rect(rect).center();
    preview_frame(
        &mut app,
        &ctx,
        vec![pointer(exit, true), pointer(exit, false)],
    );
    assert!(!app.preview.fullscreen);
    assert!(!app.editor.is_dirty());
    app.plugins.shutdown().unwrap();
}

fn track_frame(
    app: &mut Workbench,
    ctx: &egui::Context,
    origin: &Origin,
    events: Vec<Event>,
) -> Rect {
    let mut area = Rect::NOTHING;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(960., 640.))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::Window::new("Owned clock")
                .fixed_pos([80., 80.])
                .fixed_size([320., 240.])
                .show(ctx, |ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(280., 180.), egui::Sense::hover());
                    area = rect;
                    app.plugin_track_input(
                        ui,
                        origin,
                        "clock",
                        rect,
                        Viewport {
                            size: [rect.width(), rect.height()],
                            timeline: Some(TimelineViewport {
                                beat: 8.,
                                pixels_per_beat: 96.,
                                baseline: rect.height() - 24.,
                                x_min: -675.,
                                x_max: 675.,
                                division: 4,
                                lanes: 6,
                            }),
                        },
                    );
                });
        },
    );
    area
}

#[test]
fn track_input_is_confined_to_the_owned_window_and_closing_another_track_keeps_its_capture() {
    let seen = Arc::new(Mutex::new(vec![]));
    let mut app = app(ToolGesture::Drag, None, seen.clone());
    let ctx = egui::Context::default();
    let origin = app.plugins.registry.plugins[0].origin.clone().unwrap();
    track_frame(&mut app, &ctx, &origin, vec![]);
    let area = track_frame(&mut app, &ctx, &origin, vec![]);
    let p = area.center();
    track_frame(
        &mut app,
        &ctx,
        &origin,
        vec![pointer(area.min - egui::vec2(20., 20.), true)],
    );
    assert!(app.plugins.input.capture.is_none());
    track_frame(&mut app, &ctx, &origin, vec![pointer(p, true)]);
    assert!(app.plugins.input.capture.is_some());
    app.cancel_plugin_track_input(&origin, "other");
    assert!(app.plugins.input.capture.is_some());
    track_frame(&mut app, &ctx, &origin, vec![pointer(p, false)]);
    settle(&mut app);
    assert_eq!(app.editor.document().note_count(), 1);
    assert_eq!(
        seen.lock().unwrap().last().unwrap().surface.kind,
        InputSurfaceKind::Track
    );
    assert_eq!(app.editor.document().note(0, 0).unwrap().x, 0.);
    app.plugins.shutdown().unwrap();
}

mod compiled;
