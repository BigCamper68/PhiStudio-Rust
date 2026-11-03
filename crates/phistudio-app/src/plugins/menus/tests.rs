use super::*;
use phistudio_core::ChartDocument;
use phistudio_plugins::{Builtin, Runtime};
use phistudio_sdk::*;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

struct Extension {
    explicit_menu: bool,
}
impl Plugin for Extension {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => {
                let mut response = Response {
                    contributions: vec![
                        Contribution::Panel {
                            id: "view".into(),
                            title: "Menu fixture panel".into(),
                            location: PanelLocation::Window,
                            snapshot: false,
                        },
                        Contribution::Menu {
                            id: "open".into(),
                            title: "Menu fixture panel".into(),
                            location: EditorMenu::File,
                            panel: "view".into(),
                        },
                        Contribution::Track {
                            id: "track".into(),
                            title: "Menu fixture track".into(),
                        },
                        Contribution::Command {
                            id: "run".into(),
                            title: "Hidden loader command".into(),
                            shortcut: None,
                            snapshot: false,
                        },
                    ],
                    effects: vec![Effect::Panel {
                        id: "view".into(),
                        widgets: vec![Widget::Label {
                            text: "Panel payload".into(),
                        }],
                    }],
                    ..Default::default()
                };
                if !self.explicit_menu {
                    response
                        .contributions
                        .retain(|c| !matches!(c, Contribution::Menu { .. }));
                }
                response
            }
            Request::Command { .. } => Response {
                effects: vec![Effect::Panel {
                    id: "view".into(),
                    widgets: vec![Widget::Label {
                        text: "Updated payload".into(),
                    }],
                }],
                ..Default::default()
            },
            _ => Response::default(),
        })
    }
}
fn fixture() -> Workbench {
    fixture_with_menu(true)
}
fn fixture_with_menu(explicit_menu: bool) -> Workbench {
    let manifest=serde_json::from_value(json!({"api":1,"id":"test.menu","name":"Menu fixture","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["ui","timeline"],"settings":{"flag":{"kind":"bool","title":"Hidden loader setting","default":false}}})).unwrap();
    let mut manifest: Manifest = manifest;
    if !explicit_menu {
        manifest.name = "Menu fixture panel".into();
    }
    let mut app = Workbench::new(ChartDocument::empty("Menus", 120.).unwrap(), None);
    app.plugins.runtime = Some(
        Runtime::spawn(
            None,
            vec![Builtin {
                manifest,
                enabled: true,
                create: Arc::new(move || Box::new(Extension { explicit_menu })),
            }],
        )
        .unwrap(),
    );
    app.plugins.pending.insert(0, None);
    settle(&mut app);
    app
}

#[test]
fn legacy_file_actions_are_distinct_from_the_window_entry() {
    let mut app = fixture_with_menu(false);
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, "Windows");
    assert!(visible(
        &frame(&mut app, &ctx, vec![]),
        "Menu fixture panel"
    ));
    click(&mut app, &ctx, "Windows");
    click(&mut app, &ctx, "File");
    let output = frame(&mut app, &ctx, vec![]);
    assert!(!visible(&output, "Menu fixture panel"));
    assert!(visible(&output, "Menu fixture panel · More…"));
    click(&mut app, &ctx, "Menu fixture panel · More…");
    assert!(visible(
        &frame(&mut app, &ctx, vec![]),
        "Hidden loader command"
    ));
    assert!(!app.plugins.panels[&("test.menu".into(), "view".into())].open);
    app.plugins.shutdown().unwrap();
}

#[test]
fn legacy_file_actions_remain_open_after_a_touch_and_accept_commands() {
    for batched in [false, true] {
        let mut app = fixture_with_menu(false);
        let ctx = egui::Context::default();
        let mut touch = crate::touch::TouchRouter::default();
        tap(&mut app, &ctx, &mut touch, "File", batched);
        tap(
            &mut app,
            &ctx,
            &mut touch,
            "Menu fixture panel · More…",
            batched,
        );
        for _ in 0..12 {
            frame(&mut app, &ctx, vec![]);
        }
        assert!(
            visible(&frame(&mut app, &ctx, vec![]), "Hidden loader command"),
            "The actions disappeared after lifting the finger (batched: {batched})"
        );
        tap(&mut app, &ctx, &mut touch, "Hidden loader command", batched);
        settle(&mut app);
        let panel = &app.plugins.panels[&("test.menu".into(), "view".into())];
        assert!(matches!(&panel.widgets[0], Widget::Label { text } if text == "Updated payload"));
        assert!(!panel.open);
        assert!(!visible(
            &frame(&mut app, &ctx, vec![]),
            "Menu fixture panel · More…"
        ));
        app.plugins.shutdown().unwrap();
    }
}

#[test]
#[ignore = "requires the archived Chart Linter 0.1.0 ZIP"]
fn archived_chart_linter_010_exposes_its_file_actions_and_windows_panel() {
    let root = tempfile::tempdir().unwrap();
    let mut app = Workbench::new(ChartDocument::empty("Legacy linter", 120.).unwrap(), None);
    app.start_plugins(Some(root.path().into()));
    settle(&mut app);
    let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts/plugins/chart-linter-0.1.0.zip");
    app.install_plugin(MediaFile::new(package, "Chart Linter".into(), false));
    settle(&mut app);
    app.plugin_action(Action::Enable("phistudio.chart-linter".into()));
    settle(&mut app);
    let info = &app.plugins.registry.plugins[0];
    assert_eq!(info.manifest.version.to_string(), "0.1.0");
    assert_eq!(info.status, phistudio_plugins::Status::Enabled);
    assert!(
        !info
            .contributions
            .iter()
            .any(|c| matches!(c, Contribution::Menu { .. }))
    );
    let ctx = egui::Context::default();
    click(&mut app, &ctx, "File");
    assert!(visible(
        &frame(&mut app, &ctx, vec![]),
        "Chart Linter · More…"
    ));
    click(&mut app, &ctx, "Chart Linter · More…");
    assert!(visible(
        &frame(&mut app, &ctx, vec![]),
        "Chart Linter: scan chart"
    ));
    click(&mut app, &ctx, "File");
    click(&mut app, &ctx, "Windows");
    click(&mut app, &ctx, "Chart Linter");
    assert!(app.plugins.panels[&("phistudio.chart-linter".into(), "report".into())].open);
    app.plugins
        .panels
        .get_mut(&("phistudio.chart-linter".into(), "report".into()))
        .unwrap()
        .open = false;
    let ctx = egui::Context::default();
    let size = egui::vec2(800., 360.);
    let mut touch = crate::touch::TouchRouter::default();
    tap_at_size(&mut app, &ctx, &mut touch, "File", true, size);
    assert!(crate::scroll::at(&ctx, egui::pos2(100., 260.)).is_some());
    for (phase, y, time) in [
        (egui::TouchPhase::Start, 260., 0.3),
        (egui::TouchPhase::Move, 70., 0.5),
        (egui::TouchPhase::End, 70., 0.6),
    ] {
        let events = touch
            .event_with_context(&ctx, phase, 1, egui::pos2(100., y), time)
            .0;
        frame_at_size(&mut app, &ctx, events, size);
    }
    tap_at_size(
        &mut app,
        &ctx,
        &mut touch,
        "Chart Linter · More…",
        true,
        size,
    );
    for _ in 0..12 {
        frame_at_size(&mut app, &ctx, vec![], size);
    }
    assert!(visible(
        &frame_at_size(&mut app, &ctx, vec![], size),
        "Chart Linter: scan chart"
    ));
    tap_at_size(
        &mut app,
        &ctx,
        &mut touch,
        "Chart Linter: scan chart",
        true,
        size,
    );
    settle(&mut app);
    assert!(!app.plugins.panels[&("phistudio.chart-linter".into(), "report".into())].open);
    app.plugins.shutdown().unwrap();
}

#[test]
fn opening_a_cached_window_does_not_wait_for_worker_commands() {
    let mut app = fixture();
    let ctx = egui::Context::default();
    app.plugins.pending.insert(u64::MAX, None);
    frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, "File");
    click(&mut app, &ctx, "Menu fixture panel");
    assert!(app.plugins.panels[&("test.menu".into(), "view".into())].open);
    app.plugins.pending.remove(&u64::MAX);
    app.plugins.shutdown().unwrap();
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
fn frame(app: &mut Workbench, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    frame_at_size(app, ctx, events, egui::vec2(1000., 720.))
}
fn frame_at_size(
    app: &mut Workbench,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    size: egui::Vec2,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ctx| app.ui(ctx, false),
    )
}
fn visible(output: &egui::FullOutput, label: &str) -> bool {
    output.shapes.iter().any(|s|matches!(&s.1,egui::Shape::Text(t) if t.galley.job.text==label && s.0.intersects(egui::Rect::from_min_size(t.pos,t.galley.size()))))
}
fn click(app: &mut Workbench, ctx: &egui::Context, label: &str) {
    click_at_size(app, ctx, label, egui::vec2(1000., 720.));
}
fn click_at_size(app: &mut Workbench, ctx: &egui::Context, label: &str, size: egui::Vec2) {
    let point = control_at_size(app, ctx, label, size);
    for pressed in [true, false] {
        frame_at_size(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            size,
        );
    }
}
fn control_at_size(
    app: &mut Workbench,
    ctx: &egui::Context,
    label: &str,
    size: egui::Vec2,
) -> egui::Pos2 {
    frame_at_size(app, ctx, vec![], size);
    let output = frame_at_size(app, ctx, vec![], size);
    output
        .shapes
        .iter()
        .rev()
        .find_map(|s| match &s.1 {
            egui::Shape::Text(t) if t.galley.job.text == label => {
                let p = t.pos + t.galley.rect.center().to_vec2();
                s.0.contains(p).then_some(p)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing menu control: {label}"))
}
fn tap(
    app: &mut Workbench,
    ctx: &egui::Context,
    touch: &mut crate::touch::TouchRouter,
    label: &str,
    batched: bool,
) {
    tap_at_size(app, ctx, touch, label, batched, egui::vec2(1000., 720.));
}
fn tap_at_size(
    app: &mut Workbench,
    ctx: &egui::Context,
    touch: &mut crate::touch::TouchRouter,
    label: &str,
    batched: bool,
    size: egui::Vec2,
) {
    let point = control_at_size(app, ctx, label, size);
    let mut events = touch
        .event_with_context(ctx, egui::TouchPhase::Start, 1, point, 0.1)
        .0;
    if !batched {
        frame_at_size(app, ctx, std::mem::take(&mut events), size);
    }
    events.extend(
        touch
            .event_with_context(ctx, egui::TouchPhase::End, 1, point, 0.2)
            .0,
    );
    frame_at_size(app, ctx, events, size);
}

#[test]
fn windows_stay_closed_after_enable_reload_and_background_panel_updates() {
    let mut app = fixture();
    let key = ("test.menu".into(), "view".into());
    assert!(!app.plugins.panels[&key].open);
    let origin = app.plugins.registry.plugins[0].origin.clone().unwrap();
    app.open_plugin_panel(&origin, "view");
    assert!(app.plugins.panels[&key].open);
    app.plugins.panels.get_mut(&key).unwrap().open = false;
    app.plugin_command("test.menu", "run", Value::Null);
    settle(&mut app);
    assert!(!app.plugins.panels[&key].open);
    app.open_plugin_panel(&origin, "view");
    app.plugin_action(Action::Reload);
    settle(&mut app);
    assert!(!app.plugins.panels[&key].open);
    app.open_plugin_panel(&origin, "view");
    assert!(
        !app.plugins.panels[&key].open,
        "Old menu owners cannot open reloaded views"
    );
    app.plugin_action(Action::Disable("test.menu".into()));
    settle(&mut app);
    app.plugin_action(Action::Enable("test.menu".into()));
    settle(&mut app);
    assert!(!app.plugins.panels[&key].open);
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    assert!(app.plugins.rendering.tracks.values().all(|v| !v.open));
    app.plugins.shutdown().unwrap();
}

#[test]
fn editor_file_opens_the_panel_and_loader_only_appears_in_projects() {
    let mut app = fixture();
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, "Windows");
    let output = frame(&mut app, &ctx, vec![]);
    assert!(!visible(&output, "Plugins"));
    assert!(!visible(&output, "Menu fixture panel"));
    click(&mut app, &ctx, "Windows");
    click(&mut app, &ctx, "File");
    assert!(visible(
        &frame(&mut app, &ctx, vec![]),
        "Menu fixture panel"
    ));
    click(&mut app, &ctx, "Menu fixture panel");
    assert!(app.plugins.panels[&("test.menu".into(), "view".into())].open);
    frame(&mut app, &ctx, vec![]);
    assert!(visible(&frame(&mut app, &ctx, vec![]), "Panel payload"));
    app.show_projects = true;
    frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, "Plugins");
    let output = frame(&mut app, &ctx, vec![]);
    assert!(visible(&output, "Disable plugin"));
    assert!(!visible(&output, "Menu fixture panel"));
    assert!(!visible(&output, "Hidden loader command"));
    assert!(!visible(&output, "Hidden loader setting"));
    assert!(!visible(&output, "Menu fixture track"));
    app.back();
    assert!(app.show_projects && !app.plugins.open);
    app.back();
    assert!(!app.show_projects);
    assert!(app.plugins.panels[&("test.menu".into(), "view".into())].open);
    frame(&mut app, &ctx, vec![]);
    assert!(!app.plugins.open);
    app.plugins.shutdown().unwrap();
}
