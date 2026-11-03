use phistudio_core::{ChartDocument, Editor};
use phistudio_plugins::*;
use phistudio_sdk::*;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

struct Gate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
struct Tool {
    gate: Option<Arc<Gate>>,
    service: bool,
    bad: bool,
}
fn transaction(context: &EditorContext) -> Effect {
    Effect::Edit {
        transaction: Transaction {
            version: context.version,
            label: "Gesture commit".into(),
            operations: vec![PatchOperation::Replace {
                path: "/META/name".into(),
                value: json!("Gesture result"),
            }],
        },
    }
}
impl Plugin for Tool {
    fn call(&mut self, r: &Request) -> Result<Response, String> {
        Ok(match r {
            Request::Activate { .. } => Response {
                contributions: vec![
                    Contribution::Tool {
                        id: "draw".into(),
                        title: "Draw".into(),
                        surfaces: vec![InputSurfaceKind::Notes, InputSurfaceKind::Track],
                        buttons: default_input_buttons(),
                        gesture: ToolGesture::Drag,
                    },
                    Contribution::Track {
                        id: "lane".into(),
                        title: "Lane".into(),
                    },
                    Contribution::Command {
                        id: "hold".into(),
                        title: "Hold".into(),
                        shortcut: None,
                        snapshot: false,
                    },
                ],
                ..Default::default()
            },
            Request::Input { input, context, .. } => {
                assert!(context.chart.is_none());
                if input.phase() == Some(InputPhase::End) {
                    if let Some(gate) = &self.gate {
                        gate.entered.send(()).unwrap();
                        gate.release
                            .lock()
                            .unwrap()
                            .recv_timeout(Duration::from_secs(5))
                            .unwrap();
                    }
                    if self.service {
                        Response {
                            effects: vec![Effect::Call {
                                call: ServiceCall {
                                    request_id: "inspect".into(),
                                    service: ServiceRef {
                                        provider: "host".into(),
                                        id: "document".into(),
                                        version: "^1".parse().unwrap(),
                                    },
                                    method: "read".into(),
                                    arguments: json!(DocumentQuery {
                                        version: context.version,
                                        path: "/META/name".into()
                                    }),
                                },
                            }],
                            ..Default::default()
                        }
                    } else {
                        Response {
                            effects: vec![transaction(context)],
                            ..Default::default()
                        }
                    }
                } else if self.bad {
                    Response {
                        effects: vec![transaction(context)],
                        ..Default::default()
                    }
                } else {
                    Response::default()
                }
            }
            Request::ServiceResult { context, .. } => Response {
                effects: vec![transaction(context)],
                ..Default::default()
            },
            Request::Command { arguments, .. } => {
                if arguments == "block"
                    && let Some(gate) = &self.gate
                {
                    gate.entered.send(()).unwrap();
                    gate.release
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                }
                Response::default()
            }
            _ => Response::default(),
        })
    }
}
fn spawn(gate: Option<Arc<Gate>>, service: bool, bad: bool) -> Runtime {
    let manifest:Manifest=serde_json::from_value(json!({"api":1,"id":"test.input","name":"Input test","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["input","timeline","document_read","document_edit","services"]})).unwrap();
    let runtime = Runtime::spawn(
        None,
        vec![Builtin {
            manifest,
            enabled: true,
            create: Arc::new(move || {
                Box::new(Tool {
                    gate: gate.clone(),
                    service,
                    bad,
                })
            }),
        }],
    )
    .unwrap();
    wait(&runtime, 0).unwrap();
    runtime
}
fn wait(runtime: &Runtime, id: u64) -> Result<Output, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(c) = runtime.poll()? {
            assert_eq!(c.id, id);
            return c.result;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn context() -> EditorContext {
    EditorContext {
        version: DocumentVersion {
            session: 7,
            revision: 0,
        },
        ..Default::default()
    }
}
fn surface() -> InputSurface {
    InputSurface {
        kind: InputSurfaceKind::Notes,
        id: None,
        instance: "grid-notes".into(),
        viewport: Viewport {
            size: [320., 200.],
            timeline: Some(TimelineViewport {
                beat: 0.,
                pixels_per_beat: 96.,
                baseline: 176.,
                x_min: -675.,
                x_max: 675.,
                division: 4,
                lanes: 11,
            }),
        },
        metadata: Value::Null,
    }
}
fn point() -> InputPoint {
    InputPoint {
        local: [100., 100.],
        normalized: [0.3125, 0.5],
        chart_x: Some(0.),
        raw_beat: Some(1.),
        beat: Some([1, 0, 1]),
        pressure: None,
    }
}
fn interaction(runtime: &mut Runtime) -> Interaction {
    let origin = runtime.registry().unwrap().plugins[0]
        .origin
        .clone()
        .unwrap();
    runtime
        .begin_interaction(&origin, "draw", context().version, surface())
        .unwrap()
}

#[test]
fn invalid_surfaces_and_unowned_tracks_are_rejected_before_allocating_a_gesture() {
    let mut runtime = spawn(None, false, false);
    let origin = runtime.registry().unwrap().plugins[0]
        .origin
        .clone()
        .unwrap();
    let mut invalid = surface();
    invalid.viewport.size[0] = f32::NAN;
    assert!(
        runtime
            .begin_interaction(&origin, "draw", context().version, invalid)
            .is_err()
    );
    let mut unowned = surface();
    unowned.kind = InputSurfaceKind::Track;
    unowned.id = Some("other".into());
    assert!(
        runtime
            .begin_interaction(&origin, "draw", context().version, unowned.clone())
            .is_err()
    );
    unowned.id = Some("lane".into());
    let valid = runtime
        .begin_interaction(&origin, "draw", context().version, unowned)
        .unwrap();
    assert_eq!(valid.id(), 1);
    assert!(valid.is_active());
    runtime.shutdown().unwrap();
}
fn input(scope: &Interaction, phases: &[InputPhase]) -> Action {
    Action::Input {
        interaction: scope.clone(),
        context: context(),
        input: InputBatch {
            gesture: scope.id(),
            surface: surface(),
            pointer: InputPointer {
                kind: PointerKind::Mouse,
                device: 0,
                id: 0,
                button: InputButton::Primary,
            },
            start: point(),
            events: phases
                .iter()
                .map(|phase| PointerInput {
                    phase: *phase,
                    position: point(),
                    modifiers: InputModifiers::default(),
                    moved: false,
                })
                .collect(),
        },
    }
}
fn apply(runtime: &Runtime, effect: OwnedEffect, editor: &mut Editor) -> Result<bool, String> {
    let Effect::Edit { transaction } = effect.effect else {
        panic!()
    };
    runtime.apply_scoped_transaction(
        &effect.origin,
        &effect.required,
        effect.interaction.as_ref(),
        7,
        editor,
        transaction,
    )
}

#[test]
fn one_gesture_commits_once_and_history_survives_tool_disable() {
    let mut runtime = spawn(None, false, false);
    let scope = interaction(&mut runtime);
    assert!(runtime.submit(input(&scope, &[InputPhase::End])).is_err());
    let id = runtime
        .submit(input(
            &scope,
            &[InputPhase::Begin, InputPhase::Update, InputPhase::End],
        ))
        .unwrap();
    let effect = wait(&runtime, id).unwrap().effects.remove(0);
    assert_eq!(effect.interaction, Some(scope.clone()));
    let mut editor = Editor::new(ChartDocument::empty("Original", 120.).unwrap());
    let duplicate = effect.clone();
    assert!(apply(&runtime, effect, &mut editor).unwrap());
    assert!(scope.committed());
    assert!(
        apply(&runtime, duplicate, &mut editor)
            .err()
            .unwrap()
            .contains("already committed")
    );
    let id = runtime
        .submit(Action::Disable("test.input".into()))
        .unwrap();
    wait(&runtime, id).unwrap();
    assert!(editor.undo());
    assert_eq!(editor.document().name(), "Original");
    assert!(editor.redo());
    assert_eq!(editor.document().name(), "Gesture result");
    runtime.shutdown().unwrap();
}

#[test]
fn cancellation_revokes_in_progress_and_already_delivered_results() {
    let (entered, entry) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut runtime = spawn(
        Some(Arc::new(Gate {
            entered,
            release: Mutex::new(gate),
        })),
        false,
        false,
    );
    let scope = interaction(&mut runtime);
    let id = runtime
        .submit(input(&scope, &[InputPhase::Begin, InputPhase::End]))
        .unwrap();
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    scope.cancel();
    release.send(()).unwrap();
    assert!(wait(&runtime, id).unwrap().effects.is_empty());
    let scope = interaction(&mut runtime);
    let id = runtime
        .submit(input(&scope, &[InputPhase::Begin, InputPhase::End]))
        .unwrap();
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    release.send(()).unwrap();
    let mut output = wait(&runtime, id).unwrap();
    assert!(runtime.is_output_current(&output));
    scope.cancel();
    assert!(!runtime.is_output_current(&output));
    let effect = output.effects.remove(0);
    assert!(!runtime.is_effect_current(&effect));
    let mut editor = Editor::new(ChartDocument::empty("Original", 120.).unwrap());
    assert!(apply(&runtime, effect, &mut editor).is_err());
    assert!(!editor.can_undo());
    runtime.shutdown().unwrap();
}

#[test]
fn service_continuations_retain_the_cancelled_gesture_scope() {
    let mut runtime = spawn(None, true, false);
    for cancel in [false, true] {
        let scope = interaction(&mut runtime);
        let id = runtime
            .submit(input(&scope, &[InputPhase::Begin, InputPhase::End]))
            .unwrap();
        let call = wait(&runtime, id).unwrap().calls.remove(0);
        assert_eq!(call.interaction(), Some(&scope));
        assert!(!call.is_background());
        let document = ChartDocument::empty("Original", 120.).unwrap();
        let outcome = runtime.document_service(&call, context().version, &document);
        let id = runtime
            .submit(Action::ServiceReply { call, outcome })
            .unwrap();
        let effect = wait(&runtime, id).unwrap().effects.remove(0);
        if cancel {
            scope.cancel();
        }
        let mut editor = Editor::new(document);
        assert_eq!(apply(&runtime, effect, &mut editor).is_ok(), !cancel);
    }
    runtime.shutdown().unwrap();
}

#[test]
fn updates_cannot_edit_and_bad_pointer_identity_is_rejected_before_dispatch() {
    let mut runtime = spawn(None, false, true);
    let scope = interaction(&mut runtime);
    let id = runtime.submit(input(&scope, &[InputPhase::Begin])).unwrap();
    assert!(
        wait(&runtime, id)
            .err()
            .unwrap()
            .contains("completed input")
    );
    runtime.shutdown().unwrap();
    let mut runtime = spawn(None, false, false);
    let scope = interaction(&mut runtime);
    let id = runtime.submit(input(&scope, &[InputPhase::Begin])).unwrap();
    wait(&runtime, id).unwrap();
    let mut action = input(&scope, &[InputPhase::End]);
    if let Action::Input { input, .. } = &mut action {
        input.pointer.id = 123;
    }
    assert!(runtime.submit(action).is_err());
    let id = runtime.submit(input(&scope, &[InputPhase::End])).unwrap();
    assert_eq!(wait(&runtime, id).unwrap().effects.len(), 1);
    runtime.shutdown().unwrap();
}

#[test]
fn a_full_queue_does_not_advance_the_gesture_lifecycle() {
    let (entered, entry) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut runtime = spawn(
        Some(Arc::new(Gate {
            entered,
            release: Mutex::new(gate),
        })),
        false,
        false,
    );
    let action = |block| Action::Command {
        plugin: "test.input".into(),
        id: "hold".into(),
        arguments: json!(if block { "block" } else { "noop" }),
        context: context(),
    };
    let blocked = runtime.submit(action(true)).unwrap();
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    let mut jobs = vec![];
    for _ in 0..32 {
        jobs.push(runtime.submit(action(false)).unwrap());
    }
    let scope = interaction(&mut runtime);
    assert!(runtime.submit(input(&scope, &[InputPhase::Begin])).is_err());
    release.send(()).unwrap();
    wait(&runtime, blocked).unwrap();
    for id in jobs {
        wait(&runtime, id).unwrap();
    }
    let id = runtime.submit(input(&scope, &[InputPhase::Begin])).unwrap();
    wait(&runtime, id).unwrap();
    runtime.shutdown().unwrap();
}
