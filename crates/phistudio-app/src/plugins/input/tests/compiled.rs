use super::*;

fn frame(app: &mut Workbench, ctx: &egui::Context, events: Vec<Event>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1280., 800.))),
            events,
            ..Default::default()
        },
        |ctx| app.ui(ctx, false),
    )
}
fn choose_tool(app: &mut Workbench, ctx: &egui::Context) {
    frame(app, ctx, vec![]);
    let output = frame(app, ctx, vec![]);
    let p = output
        .shapes
        .iter()
        .find_map(|s| match &s.1 {
            egui::Shape::Text(t) if t.galley.job.text == "Stamp note" => {
                Some(t.pos + t.galley.rect.center().to_vec2())
            }
            _ => None,
        })
        .expect("the external tool must appear in the real toolbar");
    for pressed in [true, false] {
        frame(app, ctx, vec![Event::PointerMoved(p), pointer(p, pressed)]);
    }
    assert!(app.plugin_tool_active());
}

#[test]
#[ignore = "requires the separately built note-stamp Wasm example"]
fn compiled_wasm_stamp_uses_the_toolbar_mouse_touch_query_and_independent_history() {
    for notes in [Some(json!([])), Some(Value::Null), None] {
        let root = tempfile::tempdir().unwrap();
        let mut source = ChartDocument::empty("External stamp", 120.)
            .unwrap()
            .as_json()
            .clone();
        source["customPluginData"] = json!({"opaque":[1, null, {"keep":true}]});
        let line = source["judgeLineList"][0].as_object_mut().unwrap();
        line.insert("futureLineData".into(), json!({"keep":"source"}));
        if let Some(notes) = notes {
            line.insert("notes".into(), notes);
        } else {
            line.shift_remove("notes");
        }
        let mut app = Workbench::new(ChartDocument::from_value(source).unwrap(), None);
        let original = app.editor.document().as_json().clone();
        app.workspace.root = Some(root.path().into());
        app.start_plugins(Some(root.path().into()));
        settle(&mut app);
        let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/plugins/note-stamp");
        app.install_plugin(MediaFile::new(package, "Note stamp".into(), false));
        settle(&mut app);
        app.enable_test_plugin("example.note-stamp".into());
        settle(&mut app);
        let ctx = egui::Context::default();
        choose_tool(&mut app, &ctx);
        let g = crate::grid_input::geometry(app.timeline.area, &app);
        let p = egui::pos2(g.x(135.), g.y(1.));
        for pressed in [true, false] {
            frame(
                &mut app,
                &ctx,
                vec![Event::PointerMoved(p), pointer(p, pressed)],
            );
        }
        settle(&mut app);
        assert_eq!(app.editor.document().note_count(), 1);
        let note = app.editor.document().note(0, 0).unwrap();
        assert_eq!(note.start_time, Beat::new(1, 0, 1).unwrap());
        assert!((note.x - 135.).abs() < 1e-6);
        let mut touch = crate::touch::TouchRouter::default();
        let p = egui::pos2(g.x(-135.), g.y(2.));
        for phase in [egui::TouchPhase::Start, egui::TouchPhase::End] {
            let (ev, _) = touch.event(phase, 1, p);
            frame(&mut app, &ctx, ev);
        }
        settle(&mut app);
        assert_eq!(app.editor.document().note_count(), 2);
        assert_eq!(
            app.editor.history_rows(),
            vec![("Stamp note", true), ("Stamp note", true)]
        );
        let after = app.editor.document().as_json().clone();
        assert_eq!(after["customPluginData"], original["customPluginData"]);
        assert_eq!(
            after["judgeLineList"][0]["futureLineData"],
            original["judgeLineList"][0]["futureLineData"]
        );
        app.plugin_action(Action::Disable("example.note-stamp".into()));
        settle(&mut app);
        assert!(app.editor.undo());
        assert!(app.editor.undo());
        assert_eq!(app.editor.document().as_json(), &original);
        assert!(app.editor.redo());
        assert!(app.editor.redo());
        assert_eq!(app.editor.document().as_json(), &after);
        app.plugins.shutdown().unwrap();
    }
}
