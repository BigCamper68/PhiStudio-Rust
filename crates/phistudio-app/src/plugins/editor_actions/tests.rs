use super::*;
use phistudio_core::{ChartDocument, Command, Note, NoteId};
use phistudio_plugins::{Builtin, Status};
use std::{
    sync::{Mutex, mpsc},
    time::{Duration, Instant},
};

#[derive(Clone)]
enum Decision {
    Continue,
    Cancel,
    Seek(f64),
    Forge,
    ControlFromHook,
}
struct Gate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
struct Control {
    hooks: bool,
    input: bool,
    decision: Mutex<Decision>,
    seen: Mutex<Vec<ActionIntercept>>,
    replies: Mutex<Vec<ServiceOutcome>>,
    gate: Option<Arc<Gate>>,
}
struct Extension(Arc<Control>);
fn service(method: &str, arguments: Value) -> Effect {
    Effect::Call {
        call: ServiceCall {
            request_id: "editor-action".into(),
            service: ServiceRef {
                provider: "host".into(),
                id: "editor".into(),
                version: VersionReq::parse("^1").unwrap(),
            },
            method: method.into(),
            arguments,
        },
    }
}
impl Plugin for Extension {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => {
                let mut contributions = vec![Contribution::Command {
                    id: "control".into(),
                    title: "Control the editor".into(),
                    shortcut: None,
                    snapshot: false,
                }];
                if self.0.hooks {
                    contributions.push(Contribution::Hook {
                        point: ACTION_BEFORE_HOOK.into(),
                        priority: 0,
                    });
                }
                if self.0.input {
                    contributions.push(Contribution::Tool {
                        id: "controller".into(),
                        title: "Control from a gesture".into(),
                        surfaces: vec![InputSurfaceKind::Notes],
                        buttons: default_input_buttons(),
                        gesture: ToolGesture::Drag,
                    });
                }
                Response {
                    contributions,
                    ..Default::default()
                }
            }
            Request::Command { arguments, .. } => Response {
                effects: vec![service(
                    arguments["method"].as_str().unwrap(),
                    arguments["arguments"].clone(),
                )],
                ..Default::default()
            },
            Request::Input { input, context, .. } if input.phase() != Some(InputPhase::Cancel) => {
                let action = if input.phase() == Some(InputPhase::End) {
                    EditorAction::SetTool {
                        tool: EditorTool::Tap,
                    }
                } else {
                    EditorAction::Seek { beat: 99. }
                };
                // Both end calls share one commit allowance.
                let count = if input.phase() == Some(InputPhase::End) {
                    2
                } else {
                    1
                };
                Response {
                    effects: (0..count)
                        .map(|_| {
                            service(
                                "action",
                                json!(EditorActionRequest {
                                    version: context.version,
                                    action: action.clone(),
                                }),
                            )
                        })
                        .collect(),
                    ..Default::default()
                }
            }
            Request::Hook {
                point,
                payload,
                context,
            } => {
                assert_eq!(point, ACTION_BEFORE_HOOK);
                assert!(context.chart.is_none() && context.selection.is_null());
                let mut payload: ActionIntercept = serde_json::from_value(payload.clone()).unwrap();
                self.0.seen.lock().unwrap().push(payload.clone());
                if let Some(gate) = &self.0.gate {
                    gate.entered.send(()).unwrap();
                    gate.release
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(10))
                        .unwrap();
                }
                let decision = self.0.decision.lock().unwrap().clone();
                let hook = match decision {
                    Decision::Continue => HookDecision::Continue,
                    Decision::Cancel => HookDecision::Cancel {
                        reason: "Blocked native action".into(),
                    },
                    Decision::Seek(beat) => {
                        payload.action = EditorAction::Seek { beat };
                        HookDecision::Replace {
                            payload: json!(payload),
                        }
                    }
                    Decision::Forge => {
                        payload.action = EditorAction::Cut;
                        HookDecision::Replace {
                            payload: json!(payload),
                        }
                    }
                    Decision::ControlFromHook => {
                        return Ok(Response {
                            effects: vec![service(
                                "action",
                                json!(EditorActionRequest {
                                    version: context.version,
                                    action: EditorAction::Seek { beat: 99. },
                                }),
                            )],
                            ..Default::default()
                        });
                    }
                };
                Response {
                    hook: Some(hook),
                    ..Default::default()
                }
            }
            Request::ServiceResult { reply, .. } => {
                self.0.replies.lock().unwrap().push(reply.outcome.clone());
                Response::default()
            }
            _ => Response::default(),
        })
    }
}
struct Scene {
    app: Workbench,
    ctx: egui::Context,
    control: Arc<Control>,
}
impl Scene {
    fn new(
        capabilities: &[&str],
        hooks: bool,
        decision: Decision,
        gate: Option<Arc<Gate>>,
    ) -> Self {
        let mut capabilities = capabilities.to_vec();
        if hooks && !capabilities.contains(&"hooks") {
            capabilities.push("hooks");
        }
        let manifest = serde_json::from_value(json!({
            "api":1, "id":"test.actions", "name":"Native actions", "version":"1.0.0",
            "backend":{"kind":"builtin"}, "capabilities":capabilities,
        }))
        .unwrap();
        let control = Arc::new(Control {
            hooks,
            input: capabilities.contains(&"input"),
            decision: Mutex::new(decision),
            seen: Mutex::new(vec![]),
            replies: Mutex::new(vec![]),
            gate,
        });
        let instance = control.clone();
        let mut value = ChartDocument::empty("Native action fixture", 120.)
            .unwrap()
            .as_json()
            .clone();
        value["futureData"] = json!({"keep":[null,{"x":17}]});
        let mut app = Workbench::new(ChartDocument::from_value(value).unwrap(), None);
        app.editor
            .execute(Command::AddNote {
                line: 0,
                note: Note::default(),
            })
            .unwrap();
        app.select(Some(NoteId { line: 0, index: 0 }));
        app.plugins.runtime = Some(
            Runtime::spawn(
                None,
                vec![Builtin {
                    manifest,
                    enabled: true,
                    create: Arc::new(move || Box::new(Extension(instance.clone()))),
                }],
            )
            .unwrap(),
        );
        app.plugins.pending.insert(0, None);
        let mut scene = Self {
            app,
            ctx: egui::Context::default(),
            control,
        };
        scene.settle();
        scene
    }
    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 700.),
                )),
                events,
                ..Default::default()
            },
            |ctx| self.app.ui(ctx, false),
        )
    }
    fn settle(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.frame(vec![]);
            if !self.app.plugins.working() && !self.app.plugin_editor_action_modal() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Native action stalled: {:?}",
                self.app.error
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn invoke(&mut self, action: EditorAction) {
        self.query(
            "action",
            json!(EditorActionRequest {
                version: self.app.plugin_version(),
                action
            }),
        );
    }
    fn query(&mut self, method: &str, arguments: Value) {
        self.app.plugin_command(
            "test.actions",
            "control",
            json!({"method":method,"arguments":arguments}),
        );
    }
    fn last_reply(&self) -> ServiceOutcome {
        self.control.replies.lock().unwrap().last().unwrap().clone()
    }
    fn active(&self) -> bool {
        self.app
            .plugins
            .registry
            .plugins
            .iter()
            .any(|p| p.manifest.id == "test.actions" && p.status == Status::Enabled)
    }
}
impl Drop for Scene {
    fn drop(&mut self) {
        self.app.plugins.shutdown().unwrap();
    }
}

#[test]
fn native_delete_can_be_canceled_then_undone_through_existing_history() {
    let mut scene = Scene::new(&[], true, Decision::Cancel, None);
    let original = scene.app.editor.document().as_json().clone();
    let version = scene.app.plugin_version();
    scene.app.transport.playing = true;
    scene.frame(vec![egui::Event::Key {
        key: egui::Key::Delete,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    scene.settle();
    assert_eq!(scene.app.editor.document().as_json(), &original);
    assert_eq!(scene.app.plugin_version(), version);
    assert!(scene.app.transport.playing && !scene.app.is_scrubbing());
    assert_eq!(scene.app.selected, Some(NoteId { line: 0, index: 0 }));
    assert_eq!(
        scene.control.seen.lock().unwrap()[0].action,
        EditorAction::DeleteSelection
    );
    *scene.control.decision.lock().unwrap() = Decision::Continue;
    scene.app.delete_selected();
    scene.settle();
    assert_ne!(scene.app.plugin_version(), version);
    assert!(!scene.app.transport.playing);
    scene.app.history_step(false);
    scene.settle();
    assert_eq!(scene.app.editor.document().as_json(), &original);
    scene.app.history_step(true);
    scene.settle();
    assert_eq!(
        scene.app.editor.document().as_json()["futureData"],
        original["futureData"]
    );
}

#[test]
fn automatic_clipboard_and_escape_cleanup_do_not_wait_for_canceling_action_hooks() {
    let mut scene = Scene::new(&[], true, Decision::Cancel, None);
    scene
        .app
        .editor
        .execute(Command::AddNote {
            line: 0,
            note: Note::default(),
        })
        .unwrap();
    scene.app.options.auto_copy_selection = true;
    scene.app.set_object_selection(
        Selection {
            notes: vec![NoteId { line: 0, index: 0 }, NoteId { line: 0, index: 1 }],
            events: vec![],
        },
        vec![],
    );
    assert!(scene.app.tools.clipboard.is_some());
    assert!(!scene.app.plugin_editor_action_modal());
    scene.app.set_tool_now(Tool::Place(NoteKind::Tap));
    assert!(
        scene
            .app
            .default_tool_key(egui::Key::Escape, egui::Modifiers::NONE)
    );
    assert!(scene.app.timeline.tool == Tool::Select);
    assert!(scene.app.selection().is_empty());
    assert!(scene.control.seen.lock().unwrap().is_empty());
    assert!(!scene.app.plugin_editor_action_modal());
    scene.app.select(Some(NoteId { line: 0, index: 0 }));
    scene
        .app
        .request_editor_action(EditorAction::ClearSelection);
    scene.settle();
    assert_eq!(scene.app.selected, Some(NoteId { line: 0, index: 0 }));
    assert_eq!(
        scene.control.seen.lock().unwrap()[0].action,
        EditorAction::ClearSelection
    );
}

#[test]
fn same_kind_replacement_works_and_copy_cannot_become_cut() {
    let mut scene = Scene::new(&[], true, Decision::Seek(8.), None);
    scene
        .app
        .request_editor_action(EditorAction::Seek { beat: 7.25 });
    scene.settle();
    assert_eq!(scene.app.transport.beat, 8.);
    *scene.control.decision.lock().unwrap() = Decision::Forge;
    let original = scene.app.editor.document().as_json().clone();
    scene.app.copy_selection(false);
    scene.settle();
    assert_eq!(scene.app.editor.document().as_json(), &original);
    assert!(scene.app.tools.clipboard.is_none());
    assert!(!scene.active());
    assert!(scene.app.error.is_some());
}

#[test]
fn services_enforce_capabilities_and_version_without_disabling_the_caller() {
    for (capabilities, action, allowed) in [
        (vec!["services"], EditorAction::Seek { beat: 4. }, false),
        (
            vec!["services", "editor_control"],
            EditorAction::Seek { beat: 4. },
            true,
        ),
        (
            vec!["services", "editor_control"],
            EditorAction::DeleteSelection,
            false,
        ),
        (
            vec!["services", "editor_control", "document_edit"],
            EditorAction::DeleteSelection,
            true,
        ),
        (
            vec!["services", "editor_control"],
            EditorAction::Copy,
            false,
        ),
        (
            vec!["services", "editor_control", "document_read"],
            EditorAction::Copy,
            true,
        ),
    ] {
        let mut scene = Scene::new(&capabilities, false, Decision::Continue, None);
        let version = scene.app.plugin_version();
        scene.query("status", json!(EditorStatusQuery { version }));
        scene.settle();
        assert!(matches!(scene.last_reply(), ServiceOutcome::Success { .. }));
        scene.invoke(action);
        scene.settle();
        assert_eq!(
            matches!(scene.last_reply(), ServiceOutcome::Success { .. }),
            allowed,
            "{capabilities:?}"
        );
        assert!(scene.active());
        assert!(scene.app.error.is_none());
        let current = scene.app.plugin_version();
        scene.query(
            "action",
            json!(EditorActionRequest {
                version: DocumentVersion {
                    revision: current.revision + 1,
                    ..current
                },
                action: EditorAction::Seek { beat: 99. },
            }),
        );
        let beat = scene.app.transport.beat;
        scene.settle();
        assert!(matches!(scene.last_reply(), ServiceOutcome::Error { .. }));
        assert_eq!(scene.app.transport.beat, beat);
        assert!(scene.active());
    }
}

#[test]
fn action_hooks_are_readonly_even_with_editor_control_and_service_permissions() {
    let mut scene = Scene::new(
        &["services", "editor_control", "document_edit"],
        true,
        Decision::ControlFromHook,
        None,
    );
    scene.app.request_editor_playback(true, false);
    scene.settle();
    assert!(scene.app.transport.playing);
    assert_eq!(scene.app.transport.beat, 0.);
    assert!(matches!(scene.last_reply(), ServiceOutcome::Error { .. }));
    assert!(scene.active());
}

#[test]
fn back_cancels_waiting_action_and_late_completion_cannot_change_the_editor() {
    let (entered, arrived) = mpsc::channel();
    let (release, input) = mpsc::channel();
    let mut scene = Scene::new(
        &[],
        true,
        Decision::Continue,
        Some(Arc::new(Gate {
            entered,
            release: Mutex::new(input),
        })),
    );
    scene.app.transport.playing = true;
    let original = scene.app.editor.document().as_json().clone();
    scene.app.delete_selected();
    arrived.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(scene.app.modal_open() && scene.app.is_scrubbing());
    assert!(scene.app.transport.playing);
    scene.app.back();
    assert!(!scene.app.plugin_editor_action_modal() && scene.app.transport.playing);
    release.send(()).unwrap();
    scene.settle();
    assert_eq!(scene.app.editor.document().as_json(), &original);
    assert!(!scene.app.close_requested);
}

#[test]
fn selection_change_revokes_waiting_action_and_service_origin_is_preserved() {
    let (entered, arrived) = mpsc::channel();
    let (release, input) = mpsc::channel();
    let mut scene = Scene::new(
        &["services", "editor_control", "document_edit"],
        true,
        Decision::Continue,
        Some(Arc::new(Gate {
            entered,
            release: Mutex::new(input),
        })),
    );
    let original = scene.app.editor.document().as_json().clone();
    scene.invoke(EditorAction::DeleteSelection);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !scene.app.plugin_editor_action_modal() {
        scene.frame(vec![]);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    arrived.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        scene.control.seen.lock().unwrap()[0].source.as_deref(),
        Some("test.actions")
    );
    scene.app.select(None);
    scene.app.plugin_editor_action_preflight();
    release.send(()).unwrap();
    scene.settle();
    assert_eq!(scene.app.editor.document().as_json(), &original);
    assert!(matches!(scene.last_reply(), ServiceOutcome::Error { .. }));
    assert!(scene.active());
}

#[test]
fn gesture_control_waits_for_end_commits_once_and_acknowledges_a_tool_change() {
    let (entered, arrived) = mpsc::channel();
    let (release, input) = mpsc::channel();
    let mut scene = Scene::new(
        &["input", "services", "editor_control"],
        true,
        Decision::Continue,
        Some(Arc::new(Gate {
            entered,
            release: Mutex::new(input),
        })),
    );
    let origin = scene.app.plugins.registry.plugins[0]
        .origin
        .clone()
        .unwrap();
    scene.app.select_plugin_tool(&origin, Some("controller"));
    let grid = crate::grid_input::geometry(scene.app.timeline.area, &scene.app);
    let pos = grid.rect.center();
    let pointer = |pressed| {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    };
    let version = scene.app.plugin_version();
    scene.frame(pointer(true));
    scene.settle();
    assert_eq!(scene.app.transport.beat, 0.);
    assert!(matches!(scene.last_reply(), ServiceOutcome::Error { .. }));
    scene.frame(pointer(false));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !scene.app.plugin_editor_action_modal() {
        scene.frame(vec![]);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    arrived.recv_timeout(Duration::from_secs(5)).unwrap();
    scene.frame(vec![]);
    assert!(scene.app.plugin_editor_action_modal());
    release.send(()).unwrap();
    scene.settle();
    assert_eq!(scene.app.plugin_version(), version);
    assert!(scene.app.timeline.tool == Tool::Place(NoteKind::Tap));
    assert!(!scene.app.plugin_tool_active());
    assert_eq!(scene.control.seen.lock().unwrap().len(), 1);
    assert_eq!(
        scene
            .control
            .replies
            .lock()
            .unwrap()
            .iter()
            .filter(|r| matches!(r, ServiceOutcome::Success { .. }))
            .count(),
        1
    );
    assert!(scene.active());
}

#[test]
fn document_replacement_disable_and_focus_loss_cancel_an_action_before_handoff() {
    for change in ["document", "disable", "focus"] {
        let (entered, arrived) = mpsc::channel();
        let (release, input) = mpsc::channel();
        let mut scene = Scene::new(
            &[],
            true,
            Decision::Continue,
            Some(Arc::new(Gate {
                entered,
                release: Mutex::new(input),
            })),
        );
        let version = scene.app.plugin_version();
        scene.app.delete_selected();
        arrived.recv_timeout(Duration::from_secs(5)).unwrap();
        match change {
            "document" => scene.app.replace_document(
                ChartDocument::empty("New project", 120.).unwrap(),
                None,
                None,
            ),
            "disable" => scene
                .app
                .plugin_action(Action::Disable("test.actions".into())),
            "focus" => {
                let _ = scene.ctx.run(
                    egui::RawInput {
                        has_focus: false,
                        ..Default::default()
                    },
                    |ctx| scene.app.ui(ctx, true),
                );
            }
            _ => unreachable!(),
        }
        scene.app.plugin_editor_action_preflight();
        assert!(!scene.app.plugin_editor_action_modal(), "{change}");
        release.send(()).unwrap();
        scene.settle();
        if change == "document" {
            assert_eq!(scene.app.editor.document().name(), "New project");
            assert!(!scene.app.editor.can_undo());
        } else {
            assert_eq!(scene.app.plugin_version(), version);
            assert_eq!(scene.app.editor.document().note_count(), 1);
        }
    }
}

fn click(scene: &mut Scene, label: &str) {
    let views: Vec<_> = scene
        .app
        .plugins
        .panels
        .iter()
        .map(|((_, id), p)| (p.origin.clone(), id.clone()))
        .collect();
    for (origin, id) in views {
        scene.app.open_plugin_panel(&origin, &id);
    }
    scene.frame(vec![]);
    let output = scene.frame(vec![]);
    let point = output
        .shapes
        .iter()
        .rev()
        .find_map(|s| match &s.1 {
            egui::Shape::Text(t) if t.galley.job.text == label => {
                let point = t.pos + t.galley.rect.center().to_vec2();
                s.0.contains(point).then_some(point)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing editor action control: {label}"));
    for pressed in [true, false] {
        scene.frame(vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    scene.settle();
}

#[test]
fn revealing_a_source_object_obeys_native_hooks_permissions_and_versions() {
    for capabilities in [
        vec!["services", "editor_control"],
        vec!["services", "document_read"],
        vec!["services", "editor_control", "document_read"],
    ] {
        let mut scene = Scene::new(&capabilities, true, Decision::Cancel, None);
        scene.app.select(None);
        let original = scene.app.editor.document().as_json().clone();
        let request = EditorReveal {
            version: scene.app.plugin_version(),
            path: "/judgeLineList/0/notes/0".into(),
            beat: Some(8.),
        };
        scene.query("reveal", json!(request));
        scene.settle();
        assert!(matches!(scene.last_reply(), ServiceOutcome::Error { .. }));
        assert!(scene.app.selected.is_none());
        assert_eq!(scene.app.transport.beat, 0.);
        *scene.control.decision.lock().unwrap() = Decision::Seek(4.);
        scene.query("reveal", json!(request));
        scene.settle();
        if capabilities.contains(&"document_read") && capabilities.contains(&"editor_control") {
            assert!(matches!(scene.last_reply(), ServiceOutcome::Success { .. }));
            assert_eq!(scene.app.selected, Some(NoteId { line: 0, index: 0 }));
            assert_eq!(scene.app.transport.beat, 4.);
            scene.app.select(None);
            let mut stale = request.clone();
            stale.version.revision += 1;
            scene.query("reveal", json!(stale));
            scene.settle();
            assert!(scene.app.selected.is_none());
            assert!(matches!(scene.last_reply(), ServiceOutcome::Error { .. }));
        } else {
            assert!(matches!(scene.last_reply(), ServiceOutcome::Error { .. }));
            assert!(scene.app.selected.is_none())
        }
        assert_eq!(scene.app.editor.document().as_json(), &original);
    }
}

#[test]
#[ignore = "requires the separately built action-lab Wasm example"]
fn compiled_wasm_action_lab_controls_native_history_playback_and_seek_through_real_ui() {
    let root = tempfile::tempdir().unwrap();
    let mut scene = Scene::new(&[], false, Decision::Continue, None);
    scene.app.plugins.shutdown().unwrap();
    scene.app.plugins = State::default();
    scene.app.workspace.root = Some(root.path().into());
    scene.app.start_plugins(Some(root.path().into()));
    scene.settle();
    let package =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/action-lab");
    scene
        .app
        .install_plugin(MediaFile::new(package, "Editor action lab".into(), false));
    scene.settle();
    scene.app.enable_test_plugin("example.action-lab".into());
    scene.settle();
    assert!(scene.app.error.is_none(), "{:?}", scene.app.error);
    let original = scene.app.editor.document().as_json().clone();
    click(&mut scene, "Block native deletion");
    scene.app.delete_selected();
    scene.settle();
    assert_eq!(scene.app.editor.document().as_json(), &original);
    click(&mut scene, "Block native deletion");
    scene.app.delete_selected();
    scene.settle();
    assert_eq!(scene.app.editor.document().note_count(), 0);
    click(&mut scene, "Block Undo");
    click(&mut scene, "Undo through API");
    assert_eq!(scene.app.editor.document().note_count(), 0);
    click(&mut scene, "Block Undo");
    click(&mut scene, "Undo through API");
    assert_eq!(scene.app.editor.document().as_json(), &original);
    click(&mut scene, "Play / pause through API");
    assert!(scene.app.transport.playing);
    click(&mut scene, "Seek forward through API");
    assert_eq!(scene.app.transport.beat, 1.5);
    click(&mut scene, "Snap API seeks to bars");
    click(&mut scene, "Seek forward through API");
    assert_eq!(scene.app.transport.beat, 4.);
    click(&mut scene, "Read editor status");
    assert!(scene.app.error.is_none(), "{:?}", scene.app.error);
    scene
        .app
        .plugin_action(Action::Disable("example.action-lab".into()));
    scene.settle();
    scene.app.history_step(true);
    assert_eq!(scene.app.editor.document().note_count(), 0);
    scene.app.history_step(false);
    assert_eq!(scene.app.editor.document().as_json(), &original);
}
