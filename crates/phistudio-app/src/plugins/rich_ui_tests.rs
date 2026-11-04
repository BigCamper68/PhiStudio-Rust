use super::*;
use crate::touch::TouchRouter;

fn raw(events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(900., 700.),
        )),
        events,
        ..Default::default()
    }
}
fn text_at(out: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
    out.shapes.iter().rev().find_map(|item| match &item.1 {
        egui::Shape::Text(t) if t.galley.job.text == label => {
            let point = t.pos + t.galley.rect.center().to_vec2();
            item.0.contains(point).then_some(point)
        }
        _ => None,
    })
}
fn frame(app: &mut Workbench, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(raw(events), |ctx| app.ui(ctx, false))
}
fn tap(app: &mut Workbench, ctx: &egui::Context, touch: &mut TouchRouter, label: &str) {
    for _ in 0..3 {
        frame(app, ctx, vec![]);
    }
    let out = frame(app, ctx, vec![]);
    let point = text_at(&out, label).unwrap_or_else(|| panic!("Missing control: {label}"));
    for (phase, time) in [(egui::TouchPhase::Start, 0.1), (egui::TouchPhase::End, 0.2)] {
        frame(
            app,
            ctx,
            touch.event_with_context(ctx, phase, 1, point, time).0,
        );
    }
}
fn settle(app: &mut Workbench) {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        app.poll_plugins();
        if !app.plugins.working() {
            break;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(app.error.is_none(), "{:?}", app.error);
}

#[test]
fn tables_trees_tabs_and_masked_inputs_use_real_touch_routing() {
    let ctx = egui::Context::default();
    let mut touch = TouchRouter::default();
    let mut widgets = vec![Widget::Tabs {
        id: "pages".into(),
        selected: "table".into(),
        tabs: vec![
            UiTab {
                id: "table".into(),
                title: "Results".into(),
                children: vec![Widget::Table {
                    id: "results".into(),
                    columns: vec!["Name".into(), "Value".into()],
                    rows: vec![TableRow {
                        id: "r1".into(),
                        cells: vec!["First result".into(), "42".into()],
                    }],
                    selected: None,
                    height: 180.,
                }],
            },
            UiTab {
                id: "tree".into(),
                title: "Resources".into(),
                children: vec![
                    Widget::Tree {
                        id: "files".into(),
                        nodes: vec![TreeNode {
                            id: "root".into(),
                            text: "Project files".into(),
                            expanded: true,
                            children: vec![TreeNode {
                                id: "music".into(),
                                text: "Music file".into(),
                                children: vec![],
                                expanded: false,
                            }],
                        }],
                        selected: None,
                    },
                    Widget::Password {
                        id: "token".into(),
                        text: "Token".into(),
                        value: "canary-private-value".into(),
                    },
                ],
            },
        ],
    }];
    let mut events = vec![];
    let mut paint = |input| {
        ctx.run(raw(input), |ctx| {
            crate::scroll::begin_frame(ctx);
            egui::CentralPanel::default().show(ctx, |ui| {
                super::super::widgets::show(
                    ui,
                    "panel",
                    &mut widgets,
                    &mut events,
                    &Default::default(),
                )
            });
        })
    };
    for label in ["First result", "Resources", "Music file"] {
        for _ in 0..3 {
            paint(vec![]);
        }
        let point = text_at(&paint(vec![]), label).unwrap();
        for (phase, time) in [(egui::TouchPhase::Start, 0.1), (egui::TouchPhase::End, 0.2)] {
            paint(touch.event_with_context(&ctx, phase, 1, point, time).0);
        }
    }
    let out = paint(vec![]);
    assert!(out.shapes.iter().all(|s|!matches!(&s.1,egui::Shape::Text(t) if t.galley.job.text.contains("canary-private-value"))));
    assert!(
        events
            .iter()
            .any(|(e, _)| e.widget == "results" && e.value == json!("r1"))
    );
    assert!(
        events
            .iter()
            .any(|(e, _)| e.widget == "pages" && e.value == json!("tree"))
    );
    assert!(
        events
            .iter()
            .any(|(e, _)| e.widget == "files" && e.value == json!("music"))
    );
    assert!(!format!("{widgets:?}").contains("canary-private-value"));
}

struct DialogPeer;
impl Plugin for DialogPeer {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        let version = request
            .context()
            .map_or(DocumentVersion::default(), |c| c.version);
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![
                    Contribution::Panel {
                        id: "confirm".into(),
                        title: "Confirm changes".into(),
                        location: PanelLocation::Window,
                        snapshot: false,
                    },
                    Contribution::Command {
                        id: "open".into(),
                        title: "Open dialog".into(),
                        shortcut: None,
                        snapshot: false,
                    },
                ],
                effects: vec![
                    Effect::Panel {
                        id: "confirm".into(),
                        widgets: vec![Widget::Label {
                            text: "Dialog body".into(),
                        }],
                    },
                    Effect::Dialog {
                        panel: "confirm".into(),
                        open: true,
                        version,
                    },
                ],
                ..Default::default()
            },
            Request::Command { .. } | Request::Ui { .. } => Response {
                effects: vec![Effect::Dialog {
                    panel: "confirm".into(),
                    open: true,
                    version,
                }],
                ..Default::default()
            },
            _ => Response::default(),
        })
    }
}
#[test]
fn dialogs_require_user_actions_close_on_back_and_reject_stale_document_contexts() {
    let manifest:Manifest=serde_json::from_value(json!({"api":1,"id":"test.dialog","name":"Dialog","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["ui"]})).unwrap();
    let mut app = Workbench::new(ChartDocument::empty("Dialog", 120.).unwrap(), None);
    app.plugins.runtime = Some(
        Runtime::spawn(
            None,
            vec![Builtin {
                manifest,
                enabled: true,
                create: Arc::new(|| Box::new(DialogPeer)),
            }],
        )
        .unwrap(),
    );
    app.plugins.pending.insert(0, None);
    settle(&mut app);
    assert!(!app.plugin_dialog_modal());
    app.plugin_command("test.dialog", "open", Value::Null);
    settle(&mut app);
    assert!(app.plugin_dialog_modal());
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    assert!(text_at(&frame(&mut app, &ctx, vec![]), "Dialog body").is_some());
    app.back();
    settle(&mut app);
    assert!(
        !app.plugin_dialog_modal(),
        "Close acknowledgement cannot reopen the dialog"
    );
    app.plugin_command("test.dialog", "open", Value::Null);
    settle(&mut app);
    let version = app.plugin_version();
    app.editor
        .apply_patch(
            version.revision,
            "Change document".into(),
            vec![PatchOperation::Replace {
                path: "/META/name".into(),
                value: json!("Changed"),
            }],
        )
        .unwrap();
    settle(&mut app);
    assert!(!app.plugin_dialog_modal());
    app.plugin_command("test.dialog", "open", Value::Null);
    settle(&mut app);
    assert!(app.plugin_dialog_modal());
    app.plugin_action(Action::Reload);
    settle(&mut app);
    assert!(!app.plugin_dialog_modal());
    assert_eq!(
        app.plugins.registry.plugins[0].status,
        phistudio_plugins::Status::Enabled
    );
    app.plugins.shutdown().unwrap();
}

#[test]
fn manager_touch_consent_can_deny_access_and_revoke_an_enabled_external_package() {
    let source = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("manifest.json"),serde_json::to_vec(&json!({"api":1,"id":"test.review","name":"Review","version":"1.0.0","backend":{"kind":"wasm","entry":"plugin.wasm"},"capabilities":["document_read","services"]})).unwrap()).unwrap();
    let module = wat::parse_str(
        r#"(module (memory (export "memory") 1) (data (i32.const 1024) "{}")
        (func (export "phistudio_api_version") (result i32) i32.const 1)
        (func (export "phistudio_alloc") (param i32) (result i32) i32.const 8192)
        (func (export "phistudio_free") (param i32 i32))
        (func (export "phistudio_call") (param i32 i32) (result i64) i64.const 8589935616))"#,
    )
    .unwrap();
    std::fs::write(source.path().join("plugin.wasm"), module).unwrap();
    let mut app = Workbench::new(ChartDocument::empty("Permissions", 120.).unwrap(), None);
    app.workspace.root = Some(root.path().into());
    app.start_plugins(Some(root.path().into()));
    settle(&mut app);
    app.install_plugin(MediaFile::new(source.path().into(), "Review".into(), false));
    settle(&mut app);
    app.show_projects = true;
    app.plugins.open = true;
    let ctx = egui::Context::default();
    let mut touch = TouchRouter::default();
    tap(&mut app, &ctx, &mut touch, "Enable plugin");
    assert!(app.plugins.permissions_open());
    assert!(app.plugins.registry.plugins[0].granted.is_empty());
    tap(&mut app, &ctx, &mut touch, "Read chart");
    tap(&mut app, &ctx, &mut touch, "Apply and enable");
    settle(&mut app);
    let info = &app.plugins.registry.plugins[0];
    assert_eq!(info.status, phistudio_plugins::Status::Enabled);
    assert!(!info.granted.contains(&Capability::DocumentRead));
    assert!(info.granted.contains(&Capability::Services));
    tap(&mut app, &ctx, &mut touch, "Plugin permissions");
    tap(&mut app, &ctx, &mut touch, "Revoke permissions");
    settle(&mut app);
    assert_eq!(
        app.plugins.registry.plugins[0].status,
        phistudio_plugins::Status::Disabled
    );
    assert!(app.plugins.registry.plugins[0].granted.is_empty());
    tap(&mut app, &ctx, &mut touch, "Enable plugin");
    assert!(app.plugins.permissions_open());
    app.back();
    assert!(!app.plugins.permissions_open() && app.plugins.open);
    app.plugins.shutdown().unwrap();
}
