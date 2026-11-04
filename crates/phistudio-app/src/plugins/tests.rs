use super::*;
#[path = "chart_linter_tests.rs"]
mod chart_linter;
#[path = "expansion_tests.rs"]
mod expansion;
#[path = "project_data_tests.rs"]
mod project_data;
#[path = "rich_ui_tests.rs"]
mod rich_ui;
use phistudio_core::ChartDocument;
use phistudio_plugins::{Action, Builtin, Runtime};
use phistudio_sdk::*;
use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

#[test]
fn back_closes_the_plugin_manager_without_requesting_editor_exit() {
    let mut app = Workbench::new(ChartDocument::empty("Plugin windows", 120.).unwrap(), None);
    app.show_projects = true;
    app.plugins.open = true;
    app.back();
    assert!(!app.plugins.open);
    assert!(!app.close_requested);
    assert!(app.show_projects);
}

struct TestPlugin {
    gate: Option<Arc<Gate>>,
}
struct Gate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
impl Plugin for TestPlugin {
    fn call(&mut self, r: &Request) -> Result<Response, String> {
        Ok(match r {
            Request::Activate { .. } => Response {
                contributions: vec![
                    Contribution::Command {
                        id: "rename".into(),
                        title: "Rename with plugin".into(),
                        shortcut: Some("Ctrl+Shift+K".into()),
                        snapshot: true,
                    },
                    Contribution::Panel {
                        id: "controls".into(),
                        title: "Plugin controls".into(),
                        location: PanelLocation::Window,
                        snapshot: true,
                    },
                ],
                effects: vec![Effect::Panel {
                    id: "controls".into(),
                    widgets: vec![Widget::Button {
                        id: "apply".into(),
                        text: "Apply plugin change".into(),
                        enabled: true,
                    }],
                }],
                ..Default::default()
            },
            Request::Command { context, .. } | Request::Ui { context, .. } => {
                if let Some(gate) = &self.gate {
                    gate.entered.send(()).unwrap();
                    gate.release
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                }
                assert!(context.chart.is_some());
                Response {
                    effects: vec![Effect::Edit {
                        transaction: Transaction {
                            version: context.version,
                            label: "Plugin UI edit".into(),
                            operations: vec![PatchOperation::Replace {
                                path: "/META/name".into(),
                                value: json!("Plugin changed chart"),
                            }],
                        },
                    }],
                    ..Default::default()
                }
            }
            _ => Response::default(),
        })
    }
}
fn app(gate: Option<Arc<Gate>>) -> Workbench {
    let manifest:Manifest = serde_json::from_value(json!({"api":1,"id":"test.ui","name":"UI test","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["document_read","document_edit","ui"]})).unwrap();
    let mut app = Workbench::new(ChartDocument::empty("Original", 120.).unwrap(), None);
    app.plugins.runtime = Some(
        Runtime::spawn(
            None,
            vec![Builtin {
                manifest,
                enabled: true,
                create: Arc::new(move || Box::new(TestPlugin { gate: gate.clone() })),
            }],
        )
        .unwrap(),
    );
    app.plugins.pending.insert(0, None);
    settle(&mut app);
    app
}
fn settle(app: &mut Workbench) {
    settle_timeout(app, Duration::from_secs(10));
}
fn settle_timeout(app: &mut Workbench, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        app.poll_plugins();
        if !app.plugins.busy() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Plugin request did not finish: {:?}",
            app.plugins
                .panels
                .iter()
                .map(|(key, panel)| (key, &panel.widgets))
                .collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(app.error.is_none(), "{:?}", app.error);
}

fn click_panel(app: &mut Workbench, label: &str) {
    click_panel_timeout(app, label, Duration::from_secs(10));
}
fn click_panel_timeout(app: &mut Workbench, label: &str, timeout: Duration) {
    let views: Vec<_> = app
        .plugins
        .panels
        .iter()
        .map(|((_, id), p)| (p.origin.clone(), id.clone()))
        .collect();
    for (origin, id) in views {
        app.open_plugin_panel(&origin, &id);
    }
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    let frame = |events| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(873., 393.),
        )),
        events,
        ..Default::default()
    };
    let _ = ctx.run(frame(vec![]), |ctx| app.plugin_windows(ctx));
    let output = ctx.run(frame(vec![]), |ctx| app.plugin_windows(ctx));
    let point = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.1 {
            egui::Shape::Text(text) if text.galley.job.text == label => {
                let center = text.pos + text.galley.rect.center().to_vec2();
                shape
                    .0
                    .intersect(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(873., 393.),
                    ))
                    .contains(center)
                    .then_some(center)
            }
            _ => None,
        })
        .expect("Plugin button must be visible at phone size");
    let _ = ctx.run(
        frame(vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]),
        |ctx| app.plugin_windows(ctx),
    );
    let _ = ctx.run(
        frame(vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]),
        |ctx| app.plugin_windows(ctx),
    );
    settle_timeout(app, timeout);
}

#[test]
fn a_contributed_panel_button_edits_the_real_document_and_history_survives_disable() {
    let mut app = app(None);
    click_panel(&mut app, "Apply plugin change");
    assert_eq!(app.editor.document().name(), "Plugin changed chart");
    assert_eq!(app.editor.history_rows(), vec![("Plugin UI edit", true)]);
    app.plugin_action(Action::Disable("test.ui".into()));
    settle(&mut app);
    assert!(app.plugins.panels.is_empty());
    assert!(app.editor.undo());
    assert_eq!(app.editor.document().name(), "Original");
    assert!(app.editor.redo());
    assert_eq!(app.editor.document().name(), "Plugin changed chart");
    app.plugins.shutdown().unwrap();
}

#[test]
fn in_progress_results_are_discarded_after_project_replacement_and_host_survives() {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut app = app(Some(Arc::new(Gate {
        entered: entered_tx,
        release: Mutex::new(release_rx),
    })));
    app.plugin_command("test.ui", "rename", Value::Null);
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    app.replace_document(
        ChartDocument::empty("Another project", 160.).unwrap(),
        None,
        None,
    );
    release_tx.send(()).unwrap();
    settle(&mut app);
    assert_eq!(app.editor.document().name(), "Another project");
    assert!(!app.editor.can_undo());
    assert!(app.plugins.runtime.is_some());
    assert_eq!(app.plugins.registry.plugins.len(), 1);
    app.plugins.shutdown().unwrap();
}

#[test]
fn contributed_shortcut_uses_the_same_command_and_disabled_plugin_loses_it() {
    let mut app = app(None);
    assert!(app.plugin_key(
        egui::Key::K,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT
    ));
    settle(&mut app);
    assert_eq!(app.editor.document().name(), "Plugin changed chart");
    app.plugin_action(Action::Disable("test.ui".into()));
    settle(&mut app);
    assert!(!app.plugin_key(
        egui::Key::K,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT
    ));
    app.plugins.shutdown().unwrap();
}

struct QueryPlugin;
impl Plugin for QueryPlugin {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![Contribution::Command {
                    id: "rename".into(),
                    title: "Rename through query".into(),
                    shortcut: None,
                    snapshot: false,
                }],
                ..Default::default()
            },
            Request::Command { context, .. } => {
                assert!(context.chart.is_none());
                Response {
                    effects: vec![Effect::Call {
                        call: ServiceCall {
                            request_id: "name".into(),
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
            }
            Request::ServiceResult { reply, context } => {
                assert!(context.chart.is_none());
                match &reply.outcome {
                    ServiceOutcome::Success { value } => Response {
                        effects: vec![Effect::Edit {
                            transaction: Transaction {
                                version: context.version,
                                label: "Query rename".into(),
                                operations: vec![PatchOperation::Replace {
                                    path: "/META/name".into(),
                                    value: json!(format!(
                                        "{} via query",
                                        value["value"].as_str().unwrap()
                                    )),
                                }],
                            },
                        }],
                        ..Default::default()
                    },
                    ServiceOutcome::Error { code, .. } => {
                        assert_eq!(code, "stale_document");
                        Response::default()
                    }
                }
            }
            _ => Response::default(),
        })
    }
}

#[test]
fn document_service_round_trips_apply_edits_and_reject_project_replacement() {
    for replace in [false, true] {
        let mut app = Workbench::new(ChartDocument::empty("Original", 120.).unwrap(), None);
        let manifest:Manifest = serde_json::from_value(json!({"api":1,"id":"test.query","name":"Query test","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["document_read","document_edit","services"]})).unwrap();
        app.plugins.runtime = Some(
            Runtime::spawn(
                None,
                vec![Builtin {
                    manifest,
                    enabled: true,
                    create: Arc::new(|| Box::new(QueryPlugin)),
                }],
            )
            .unwrap(),
        );
        app.plugins.pending.insert(0, None);
        settle(&mut app);
        app.plugin_command("test.query", "rename", Value::Null);
        if replace {
            // Hold the outgoing query before the document-owning thread reads.
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(completion) = app.plugins.runtime.as_ref().unwrap().poll().unwrap() {
                    app.plugins.pending.remove(&completion.id);
                    app.plugins.calls.extend(completion.result.unwrap().calls);
                    break;
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
            app.replace_document(
                ChartDocument::empty("Replacement", 160.).unwrap(),
                None,
                None,
            );
        }
        settle(&mut app);
        if replace {
            assert_eq!(app.editor.document().name(), "Replacement");
            assert!(!app.editor.can_undo());
        } else {
            assert_eq!(app.editor.document().name(), "Original via query");
            assert_eq!(app.editor.history_rows(), vec![("Query rename", true)]);
        }
        app.plugins.shutdown().unwrap();
    }
}

#[test]
#[ignore = "requires the separately built note-shift Wasm example"]
fn compiled_wasm_package_installs_enables_and_edits_through_its_actual_panel() {
    installed_note_shift_panel("../../examples/plugins/note-shift", "example.note-shift");
}

#[test]
#[ignore = "requires the separately built and packaged native note-shift example"]
fn compiled_native_package_installs_enables_and_edits_through_its_actual_panel() {
    installed_note_shift_panel(
        "../../examples/plugins/note-shift/dist/native",
        "example.note-shift-native",
    );
}

fn installed_note_shift_panel(package: &str, plugin: &str) {
    let root = tempfile::tempdir().unwrap();
    let mut app = Workbench::new(ChartDocument::parse(crate::DEMO).unwrap(), None);
    let original = app.editor.document().as_json().clone();
    app.workspace.root = Some(root.path().into());
    app.start_plugins(Some(root.path().into()));
    settle(&mut app);
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(package);
    app.install_plugin(MediaFile::new(source, "Note shift example".into(), false));
    settle(&mut app);
    assert_eq!(
        app.plugins.registry.plugins[0].status,
        phistudio_plugins::Status::Disabled
    );
    app.enable_test_plugin(plugin.into());
    settle(&mut app);
    click_panel(&mut app, "Move notes on active line");
    let after = app.editor.document().as_json().clone();
    for (before, after) in original["judgeLineList"][0]["notes"]
        .as_array()
        .unwrap()
        .iter()
        .zip(after["judgeLineList"][0]["notes"].as_array().unwrap())
    {
        assert_eq!(
            after["positionX"].as_f64().unwrap(),
            before["positionX"].as_f64().unwrap() + 25.
        );
    }
    app.plugin_action(Action::Disable(plugin.into()));
    settle(&mut app);
    assert!(app.editor.undo());
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(app.editor.redo());
    assert_eq!(app.editor.document().as_json(), &after);
    app.plugins.shutdown().unwrap();
}

#[test]
#[ignore = "requires the separately built service-lab Wasm example"]
fn compiled_wasm_service_lab_replaces_cancels_and_restores_calls_through_its_real_panel() {
    let root = tempfile::tempdir().unwrap();
    let mut app = Workbench::new(ChartDocument::empty("Service lab", 120.).unwrap(), None);
    let original = app.editor.document().as_json().clone();
    app.workspace.root = Some(root.path().into());
    app.start_plugins(Some(root.path().into()));
    settle(&mut app);
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/service-lab");
    app.install_plugin(MediaFile::new(source, "Service lab".into(), false));
    settle(&mut app);
    app.enable_test_plugin("example.service-lab".into());
    settle(&mut app);
    let plugin = &app.plugins.registry.plugins[0];
    assert!(!plugin.granted.contains(&Capability::DocumentRead));
    assert!(!plugin.granted.contains(&Capability::DocumentEdit));
    click_panel(&mut app, "Call virtual greeting");
    assert_eq!(app.status, "host/greeting: Hello, PhiStudio");
    click_panel(&mut app, "Block the virtual service");
    click_panel(&mut app, "Call virtual greeting");
    assert_eq!(app.status, "hook_cancelled: The virtual service is blocked");
    click_panel(&mut app, "Block the virtual service");
    click_panel(&mut app, "Call virtual greeting");
    assert_eq!(app.status, "host/greeting: Hello, PhiStudio");
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(!app.editor.is_dirty());
    assert!(!app.editor.can_undo());
    app.plugin_action(Action::Disable("example.service-lab".into()));
    settle(&mut app);
    assert!(app.plugins.panels.is_empty());
    assert!(app.plugins.registry.plugins[0].contributions.is_empty());
    app.plugins.shutdown().unwrap();
}

#[test]
#[ignore = "requires the separately built note-statistics Wasm example"]
fn compiled_wasm_statistics_reads_pages_from_a_chart_larger_than_the_snapshot_limit() {
    let root = tempfile::tempdir().unwrap();
    let mut source = ChartDocument::empty("Paged plugin", 120.)
        .unwrap()
        .as_json()
        .clone();
    source["unrelated_plugin_data"] = json!("x".repeat(MAX_MESSAGE_BYTES + 1));
    let mut notes = vec![];
    for i in 0..4000 {
        let mut note = serde_json::to_value(phistudio_core::Note {
            kind: [
                phistudio_core::NoteKind::Tap,
                phistudio_core::NoteKind::Hold,
                phistudio_core::NoteKind::Flick,
                phistudio_core::NoteKind::Drag,
            ][i % 4],
            ..Default::default()
        })
        .unwrap();
        note["unknown"] = json!({"retain":[1,null,3]});
        notes.push(note);
    }
    source["judgeLineList"][0]["notes"] = json!(notes);
    source["judgeLineList"][0]["numOfNotes"] = json!(4000);
    let mut app = Workbench::new(ChartDocument::from_value(source).unwrap(), None);
    let original = app.editor.document().as_json().clone();
    app.workspace.root = Some(root.path().into());
    app.start_plugins(Some(root.path().into()));
    settle(&mut app);
    let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/plugins/note-statistics");
    app.install_plugin(MediaFile::new(
        package,
        "Note statistics example".into(),
        false,
    ));
    settle(&mut app);
    app.enable_test_plugin("example.note-statistics".into());
    settle(&mut app);
    let started = Instant::now();
    click_panel_timeout(
        &mut app,
        "Count notes on active line",
        Duration::from_secs(60),
    );
    eprintln!(
        "Compiled guest paged scan (4000 notes): {:?}",
        started.elapsed()
    );
    let panel = &app.plugins.panels[&("example.note-statistics".into(), "statistics".into())];
    assert!(panel.widgets.iter().any(|w|matches!(w,Widget::Label {text} if text == "Line 0: 4000 notes · Tap 1000 · Hold 1000 · Flick 1000 · Drag 1000 · Other 0")),"{:?}",panel.widgets);
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(!app.editor.is_dirty());
    assert!(!app.editor.can_undo());
    app.plugins.shutdown().unwrap();
}

// Integration fixtures explicitly accept their test package's requested access.
impl Workbench {
    pub(crate) fn enable_test_plugin(&mut self, id: String) {
        if let Some(info) = self
            .plugins
            .registry
            .plugins
            .iter()
            .find(|p| p.manifest.id == id)
            && !matches!(info.manifest.backend, Backend::Builtin)
        {
            let requested = PermissionRequest::from_manifest(&info.manifest);
            self.plugin_action(Action::Permissions {
                plugin: id.clone(),
                decision: Some(PermissionDecision {
                    requested: requested.clone(),
                    granted: requested,
                }),
            });
        }
        self.plugin_action(Action::Enable(id));
    }
}
