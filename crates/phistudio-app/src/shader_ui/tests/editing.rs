use super::*;

#[test]
fn selection_and_inspection_do_not_materialize_override_or_dirty_history() {
    let root = tempfile::tempdir().unwrap();
    let extra = json!({"bpm":120,"effects":[{"shader":"grayscale","start":[0,0,4],"end":[4,0,7],"unknown":"retained"}]});
    std::fs::write(root.path().join("extra.json"), extra.to_string()).unwrap();
    let mut app = Workbench::new(
        ChartDocument::empty("Imported", 120.).unwrap(),
        Some(root.path().join("chart.json")),
    );
    app.advanced.right_mode = RightMode::Shaders;
    app.set_tool(crate::workbench::Tool::Select);
    app.refresh_shaders();
    app.select_shader(0);
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&mut app, &ctx, vec2(873., 393.), vec![]);
    }
    assert!(app.flush_shader_draft());
    assert!(!app.editor.is_dirty());
    assert!(app.editor.document().as_json().get(EXTRA_KEY).is_none());
}

#[test]
fn source_edit_is_atomic_and_hidden_note_shortcuts_cannot_edit_chart() {
    let mut app = app();
    let notes = app.editor.document().as_json()["judgeLineList"].clone();
    app.new_shader_code(TEMPLATE.into());
    assert!(app.flush_shader_draft());
    app.refresh_shaders();
    assert_eq!(app.shaders.effects.len(), 1);
    assert_eq!(app.editor.history_rows().len(), 1);
    app.select_shader(0);
    assert!(app.shaders.parameters.iter().any(|p| p.name == "amount"));
    app.shader_key(egui::Key::D, egui::Modifiers::COMMAND);
    app.refresh_shaders();
    assert_eq!(app.shaders.effects.len(), 2);
    app.shader_key(egui::Key::Delete, egui::Modifiers::NONE);
    app.refresh_shaders();
    assert_eq!(app.shaders.effects.len(), 1);
    assert_eq!(app.editor.document().as_json()["judgeLineList"], notes);
    app.history_step(false);
    app.history_step(false);
    app.history_step(false);
    assert!(!app.editor.is_dirty());
    assert!(app.editor.document().as_json().get(SOURCES_KEY).is_none());
}

#[test]
fn pending_effect_values_save_and_invalid_interval_is_not_committed() {
    let mut app = app();
    app.add_shader();
    let saved = app.editor.document().as_json().clone();
    app.shaders.draft.as_mut().unwrap().end = "-1".into();
    assert!(!app.flush_shader_draft());
    assert_eq!(app.editor.document().as_json(), &saved);
    app.shaders.draft.as_mut().unwrap().end = "8".into();
    assert!(app.flush_shader_draft());
    assert_eq!(
        app.editor.document().as_json()[EXTRA_KEY]["effects"][0]["end"],
        json!([8, 0, 1])
    );
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &saved);
}

#[test]
fn interval_drag_commits_once_and_keeps_pending_uniform_changes() {
    let mut app = app();
    let ctx = egui::Context::default();
    let size = vec2(1194., 834.);
    app.add_shader();
    for _ in 0..3 {
        frame(&mut app, &ctx, size, vec![]);
    }
    app.shaders.draft.as_mut().unwrap().raw["vars"]["power"] = json!(0.25);
    frame(&mut app, &ctx, size, vec![]);
    let grid = crate::grid_input::geometry(app.timeline.area, &app);
    let pos = pos2(effect_x(&app, grid, 0), grid.y(2.));
    frame(
        &mut app,
        &ctx,
        size,
        vec![egui::Event::PointerMoved(pos), pointer(pos, true)],
    );
    frame(
        &mut app,
        &ctx,
        size,
        vec![egui::Event::PointerMoved(pos - vec2(0., 48.))],
    );
    let count = app.editor.history_rows().len();
    frame(
        &mut app,
        &ctx,
        size,
        vec![pointer(pos - vec2(0., 96.), false)],
    );
    assert_eq!(app.editor.history_rows().len(), count + 1);
    assert_eq!(app.shaders.effects[0].start, Beat::ONE);
    assert_eq!(app.shaders.effects[0].vars["power"], json!(0.25));
    app.history_step(false);
    app.refresh_shaders();
    assert_eq!(app.shaders.effects[0].start, Beat::ZERO);
    assert_eq!(app.shaders.effects[0].vars["power"], json!(0.25));
}

#[test]
fn shader_vertical_resize_and_erase_are_single_reversible_gestures() {
    let mut app = app();
    app.add_shader();
    let original = app.editor.document().as_json().clone();
    let grid =
        crate::grid_input::geometry(Rect::from_min_max(pos2(0., 80.), pos2(900., 700.)), &app);
    let x = effect_x(&app, grid, 0);
    let start = pos2(x, grid.y(4.));
    let end = pos2(x, grid.y(5.));
    let count = app.editor.history_rows().len();
    crate::grid_input::handle(
        &mut app,
        &[
            pointer(start, true),
            egui::Event::PointerMoved(end),
            pointer(end, false),
        ],
        grid,
        grid.full_rect(),
        |_| true,
        false,
    );
    app.refresh_shaders();
    assert_eq!(app.shaders.effects[0].end, Beat::new(5, 0, 1).unwrap());
    assert_eq!(app.editor.history_rows().len(), count + 1);
    app.history_step(false);
    app.refresh_shaders();
    assert_eq!(app.editor.document().as_json(), &original);
    app.add_shader();
    app.shaders.draft.as_mut().unwrap().raw["phiStudioEditorX"] = json!(0.);
    assert!(app.flush_shader_draft());
    let erase_original = app.editor.document().as_json().clone();
    let count = app.editor.history_rows().len();
    app.set_tool(crate::workbench::Tool::Erase);
    let a = pos2(x, grid.y(2.));
    crate::grid_input::handle(
        &mut app,
        &[
            pointer(a, true),
            egui::Event::PointerMoved(a + vec2(0., 60.)),
            pointer(a, false),
        ],
        grid,
        grid.full_rect(),
        |_| true,
        false,
    );
    app.refresh_shaders();
    assert!(app.shaders.effects.is_empty());
    assert_eq!(app.editor.history_rows().len(), count + 1);
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &erase_original);
}

#[test]
fn shader_rgba_uses_shared_palette_with_apply_cancel_and_undo() {
    let mut app = app();
    app.new_shader_code(TEMPLATE.replace(
        "uniform float amount;",
        "uniform vec4 tint; // %1,1,1,1%\nuniform float amount;",
    ));
    assert!(app.flush_shader_draft());
    app.refresh_shaders();
    let before = app.editor.document().as_json().clone();
    let ctx = egui::Context::default();
    let size = vec2(873., 393.);
    app.open_shader_color("tint".into(), None, [30, 40, 50, 128]);
    for _ in 0..3 {
        frame(&mut app, &ctx, size, vec![]);
    }
    let out = frame(&mut app, &ctx, size, vec![]);
    let apply = text_position(&out, "Apply color");
    click(&mut app, &ctx, size, apply);
    assert!(app.palette.is_none());
    assert_eq!(
        app.shaders.draft.as_ref().unwrap().raw["vars"]["tint"],
        json!([30, 40, 50, 128])
    );
    app.open_shader_color("tint".into(), None, [255, 0, 0, 255]);
    assert!(app.close_color_picker());
    assert_eq!(
        app.shaders.draft.as_ref().unwrap().raw["vars"]["tint"],
        json!([30, 40, 50, 128])
    );
    assert!(app.flush_shader_draft());
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &before);
}
