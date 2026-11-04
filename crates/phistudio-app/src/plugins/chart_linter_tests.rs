//! End-to-end checks of the installable external Wasm, real panel and history.
use super::*;
const ID: &str = "phistudio.chart-linter";
fn widgets(app: &Workbench) -> &[Widget] {
    &app.plugins.panels[&(ID.into(), "report".into())].widgets
}
fn labels(items: &[Widget]) -> Vec<&str> {
    let mut text = vec![];
    for widget in items {
        match widget {
            Widget::Label { text: value } => text.push(value.as_str()),
            Widget::Row { children } | Widget::Column { children } => text.extend(labels(children)),
            _ => {}
        }
    }
    text
}
fn wait_report(app: &mut Workbench, needle: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        app.poll_plugins();
        if labels(widgets(app)).iter().any(|s| s.contains(needle)) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Report did not reach {needle}: {:?}, error {:?}, status {:?}, pending {:?}, calls {}, registry {:?}",
            labels(widgets(app)),
            app.error,
            app.status,
            app.plugins.pending,
            app.plugins.calls.len(),
            app.plugins.registry
        );
        assert!(
            app.plugins
                .registry
                .plugins
                .iter()
                .all(|p| p.status == phistudio_plugins::Status::Enabled),
            "{:?}",
            app.plugins.registry
        );
        assert!(app.error.is_none(), "{:?}", app.error);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn event(app: &mut Workbench, widget: &str, value: Value) {
    app.plugin_ui_event(
        ID.into(),
        UiEvent {
            panel: "report".into(),
            widget: widget.into(),
            value,
        },
        false,
    );
}
fn fixture() -> Workbench {
    let mut source = ChartDocument::empty("Linter fixture", 120.)
        .unwrap()
        .as_json()
        .clone();
    let mut note = serde_json::to_value(phistudio_core::Note::default()).unwrap();
    note["type"] = json!(2);
    note["startTime"] = json!([2, 0, 1]);
    note["endTime"] = json!([2, 0, 1]);
    note["unknown"] = json!("preserve".repeat(50000));
    source["judgeLineList"][0]["notes"] = json!([note]);
    source["foreignBlob"] = json!("x".repeat(MAX_MESSAGE_BYTES + 1));
    Workbench::new(ChartDocument::from_value(source).unwrap(), None)
}
fn installed(app: &mut Workbench, root: &std::path::Path) {
    app.workspace.root = Some(root.into());
    app.start_plugins(Some(root.into()));
    settle(app);
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: Manifest = serde_json::from_slice(
        &std::fs::read(repository.join("plugins/chart-linter/manifest.json")).unwrap(),
    )
    .unwrap();
    let package = repository.join(format!(
        "artifacts/plugins/chart-linter-{}.zip",
        manifest.version
    ));
    app.install_plugin(MediaFile::new(package, "Chart Linter".into(), false));
    settle(app);
    assert_eq!(
        app.plugins.registry.plugins[0].status,
        phistudio_plugins::Status::Disabled
    );
    app.enable_test_plugin(ID.into());
    settle(app);
    assert_eq!(
        app.plugins.registry.plugins[0].status,
        phistudio_plugins::Status::Enabled
    );
    assert!(!app.plugins.panels[&(ID.into(), "report".into())].open);
    let ctx = egui::Context::default();
    let frame = |app: &mut Workbench, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 720.),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.ui(ctx, false),
        )
    };
    for label in ["File", "Chart Linter"] {
        frame(app, vec![]);
        let output = frame(app, vec![]);
        let point = output
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
            .unwrap_or_else(|| panic!("Missing editor menu entry: {label}"));
        for pressed in [true, false] {
            frame(
                app,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }
    assert!(app.plugins.panels[&(ID.into(), "report".into())].open);
}

#[test]
#[ignore = "requires the separately built Chart Linter Wasm package"]
fn compiled_chart_linter_scans_locates_confirms_removal_and_preserves_undo() {
    let root = tempfile::tempdir().unwrap();
    let mut app = fixture();
    installed(&mut app, root.path());
    let original = app.editor.document().as_json().clone();
    click_panel(&mut app, "Basic scan");
    wait_report(&mut app, "Scan complete");
    let text = labels(widgets(&app));
    assert!(text.iter().any(|s| s.contains("hold_zero")), "{text:?}");
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(!app.editor.can_undo());
    event(&mut app, "locate.r0", Value::Null);
    settle(&mut app);
    assert_eq!(
        app.selected,
        Some(phistudio_core::NoteId { line: 0, index: 0 })
    );
    assert_eq!(app.transport.beat, 1.5);
    assert!(app.plugins.panels[&(ID.into(), "report".into())].open);
    event(&mut app, "remove.r0", Value::Null);
    settle(&mut app);
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(
        labels(widgets(&app))
            .iter()
            .any(|s| s.starts_with("Remove /judgeLineList/0/notes/0?"))
    );
    event(&mut app, "remove_confirm", Value::Null);
    settle(&mut app);
    assert_eq!(
        app.editor.document().as_json()["judgeLineList"][0]["notes"],
        json!([])
    );
    assert_eq!(
        app.editor.history_rows(),
        vec![("Chart Linter: remove object", true)]
    );
    app.plugin_action(Action::Disable(ID.into()));
    settle(&mut app);
    assert!(app.plugins.panels.is_empty());
    assert!(app.editor.undo());
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(app.editor.redo());
    assert_eq!(
        app.editor.document().as_json()["judgeLineList"][0]["notes"],
        json!([])
    );
    app.plugins.shutdown().unwrap();
}

#[test]
#[ignore = "requires the separately built Chart Linter Wasm package"]
fn compiled_chart_linter_cancels_while_preview_waits_and_invalidates_stale_results() {
    let root = tempfile::tempdir().unwrap();
    let mut app = fixture();
    installed(&mut app, root.path());
    let original = app.editor.document().as_json().clone();
    event(&mut app, "deep", Value::Null);
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        app.poll_plugins();
        if !app.plugins.inspections.is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "{:?}", labels(widgets(&app)));
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        !app.plugins.busy(),
        "Background scan must leave Cancel available"
    );
    let call = app.plugins.inspections.pop_front().unwrap();
    event(&mut app, "cancel", Value::Null);
    wait_report(&mut app, "Scan canceled");
    app.reply_plugin_service(
        call,
        ServiceOutcome::error("test_late_reply", "A late native result"),
    );
    settle(&mut app);
    assert!(
        labels(widgets(&app))
            .iter()
            .any(|s| s.contains("Scan canceled"))
    );
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(!app.editor.can_undo());
    event(&mut app, "scan", Value::Null);
    wait_report(&mut app, "Scan complete");
    app.replace_document(
        ChartDocument::empty("Replacement", 140.).unwrap(),
        None,
        None,
    );
    settle(&mut app);
    assert!(
        labels(widgets(&app))
            .iter()
            .any(|s| s.contains("Run a new scan"))
    );
    event(&mut app, "remove.r0", Value::Null);
    event(&mut app, "remove_confirm", Value::Null);
    settle(&mut app);
    assert_eq!(app.editor.document().name(), "Replacement");
    assert!(!app.editor.can_undo());
    app.plugins.shutdown().unwrap();
}

#[test]
#[ignore = "requires the separately built Chart Linter Wasm package"]
fn compiled_chart_linter_inspects_real_project_assets_and_indirect_shader_links() {
    let root = tempfile::tempdir().unwrap();
    let assets = root.path().join("assets");
    for directory in ["skin", "a", "b"] {
        std::fs::create_dir_all(assets.join(directory)).unwrap()
    }
    std::fs::write(
        assets.join("skin/config.json"),
        br#"{"texture":"missing.png","name":"display.png"}"#,
    )
    .unwrap();
    std::fs::write(
        assets.join("skin/main.glsl"),
        b"#include \"common.glsl\"\n// @texture glow.png",
    )
    .unwrap();
    for path in ["a/hit.wav", "b/hit.wav"] {
        std::fs::write(assets.join(path), b"placeholder").unwrap()
    }
    let mut source = ChartDocument::empty("Project assets", 120.)
        .unwrap()
        .as_json()
        .clone();
    source["noiseAreaLayers"] = json!([{"skin":"skin"}]);
    let mut note = serde_json::to_value(phistudio_core::Note::default()).unwrap();
    note["hitsound"] = json!("hit.wav");
    source["judgeLineList"][0]["notes"] = json!([note]);
    std::fs::write(
        assets.join("chart.json"),
        serde_json::to_vec(&source).unwrap(),
    )
    .unwrap();
    let mut app = Workbench::new(
        ChartDocument::from_value(source).unwrap(),
        Some(assets.join("chart.json")),
    );
    installed(&mut app, root.path());
    let original = app.editor.document().as_json().clone();
    event(&mut app, "scan", Value::Null);
    wait_report(&mut app, "Scan complete");
    assert!(
        labels(widgets(&app))
            .iter()
            .any(|s| s.contains("ambiguous")),
        "{:?}",
        labels(widgets(&app))
    );
    event(&mut app, "deep", Value::Null);
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        app.poll_plugins();
        if let Some(call) = app.plugins.inspections.pop_front() {
            app.reply_plugin_service(
                call,
                ServiceOutcome::error(
                    "no_graphics",
                    "Native graphics are checked by chart-linter-probe",
                ),
            );
            break;
        }
        assert!(Instant::now() < deadline, "{:?}", labels(widgets(&app)));
        std::thread::sleep(Duration::from_millis(1));
    }
    wait_report(&mut app, "Scan incomplete");
    let text = labels(widgets(&app));
    for name in ["missing.png", "common.glsl", "glow.png"] {
        assert!(
            text.iter().any(|s| s.contains(name)),
            "Missing {name}: {text:?}"
        )
    }
    assert!(!text.iter().any(|s| s.contains("display.png")));
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(!app.editor.can_undo());
    app.plugins.shutdown().unwrap();
}
