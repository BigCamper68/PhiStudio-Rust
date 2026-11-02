use super::*;

#[test]
fn overlapping_keyframes_cycle_without_history_and_drag_the_selected_lower_row() {
    let mut app = app();
    app.add_shader();
    app.shaders.draft.as_mut().unwrap().raw["vars"]["power"] = json!([
        {"startTime":[2,0,1],"endTime":[4,0,1],"start":0.1,"end":0.2,"easingType":1,"custom":0},
        {"startTime":[2,0,1],"endTime":[4,0,1],"start":0.3,"end":0.4,"easingType":1,"custom":1},
        {"startTime":[2,0,1],"endTime":[4,0,1],"start":0.5,"end":0.6,"easingType":1,"custom":2}
    ]);
    assert!(app.flush_shader_draft());
    app.shader_navigate(ShaderPage::Animation, Some("power".into()));
    app.shaders.animation = None;
    app.editor.mark_saved();
    let original = app.editor.document().as_json().clone();
    let count = app.editor.history_rows().len();
    let grid =
        crate::grid_input::geometry(Rect::from_min_max(pos2(0., 80.), pos2(900., 700.)), &app);
    let p = pos2(parameter_x(&app, grid, "power"), grid.y(3.));
    for index in [2, 1, 0, 2, 1] {
        crate::grid_input::handle(
            &mut app,
            &[pointer(p, true), pointer(p, false)],
            grid,
            grid.full_rect(),
            |_| true,
            false,
        );
        assert_eq!(app.shaders.animation.as_ref().unwrap().index, index);
    }
    assert_eq!(app.editor.document().as_json(), &original);
    assert_eq!(app.editor.history_rows().len(), count);
    assert!(!app.editor.is_dirty());
    let b = p - vec2(0., 96.);
    crate::grid_input::handle(
        &mut app,
        &[
            pointer(p, true),
            egui::Event::PointerMoved(b),
            pointer(b, false),
        ],
        grid,
        grid.full_rect(),
        |_| true,
        false,
    );
    let rows = &app.editor.document().as_json()[EXTRA_KEY]["effects"][0]["vars"]["power"];
    assert_eq!(rows[1]["startTime"], json!([3, 0, 1]));
    assert_eq!(rows[1]["custom"], 1);
    assert_eq!(
        rows[0],
        original[EXTRA_KEY]["effects"][0]["vars"]["power"][0]
    );
    assert_eq!(
        rows[2],
        original[EXTRA_KEY]["effects"][0]["vars"]["power"][2]
    );
    assert_eq!(app.editor.history_rows().len(), count + 1);
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &original);
}

#[test]
fn animation_clipboard_and_delete_only_edit_keyframes() {
    let mut app = app();
    app.shaders.preset = 9;
    app.add_shader();
    app.shader_navigate(ShaderPage::Parameters, Some("radius".into()));
    app.shader_create_animation();
    let original = app.editor.document().as_json().clone();
    let count = app.editor.history_rows().len();
    app.shader_key(egui::Key::D, egui::Modifiers::COMMAND);
    app.refresh_shaders();
    assert_eq!(app.shaders.effects.len(), 1);
    assert_eq!(
        app.shaders.effects[0].vars["radius"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(app.shaders.animation.as_ref().unwrap().index, 1);
    assert_eq!(app.editor.history_rows().len(), count + 1);
    app.shader_key(egui::Key::C, egui::Modifiers::COMMAND);
    app.shader_key(egui::Key::Delete, egui::Modifiers::NONE);
    app.refresh_shaders();
    assert_eq!(app.shaders.effects.len(), 1);
    assert_eq!(app.editor.document().as_json(), &original);
    app.shader_key(egui::Key::V, egui::Modifiers::COMMAND);
    app.refresh_shaders();
    assert_eq!(app.shaders.effects.len(), 1);
    assert_eq!(
        app.shaders.effects[0].vars["radius"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &original);
    app.shader_navigate(ShaderPage::Animation, Some("color".into()));
    let before = app.editor.document().as_json().clone();
    app.shader_key(egui::Key::V, egui::Modifiers::COMMAND);
    assert_eq!(app.editor.document().as_json(), &before);
    assert!(app.error.as_ref().unwrap().contains("parameter type"));
}

#[test]
fn first_keyframe_can_be_drawn_from_float_or_rgba_constants() {
    for (name, value) in [("radius", json!(7.)), ("color", json!([30, 40, 50, 128]))] {
        let mut app = app();
        app.shaders.preset = 9;
        app.add_shader();
        app.shaders.draft.as_mut().unwrap().raw["vars"][name] = value.clone();
        assert!(app.flush_shader_draft());
        app.refresh_shaders();
        let original = app.editor.document().as_json().clone();
        app.shader_navigate(ShaderPage::Animation, Some(name.into()));
        assert!(app.shaders.animation.is_none());
        app.set_tool(crate::workbench::Tool::Event);
        let grid =
            crate::grid_input::geometry(Rect::from_min_max(pos2(0., 80.), pos2(900., 700.)), &app);
        let a = pos2(parameter_x(&app, grid, name), grid.y(0.));
        let b = pos2(a.x, grid.y(1.));
        let count = app.editor.history_rows().len();
        crate::grid_input::handle(
            &mut app,
            &[
                pointer(a, true),
                egui::Event::PointerMoved(b),
                pointer(b, false),
            ],
            grid,
            grid.full_rect(),
            |_| true,
            false,
        );
        app.refresh_shaders();
        assert_eq!(app.shaders.effects.len(), 1);
        let rows = app.shaders.effects[0].vars[name].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["start"], value);
        assert_eq!(rows[0]["end"], value);
        assert_eq!(app.shaders.animation.as_ref().unwrap().index, 0);
        assert_eq!(app.editor.history_rows().len(), count + 1);
        app.shader_key(egui::Key::X, egui::Modifiers::COMMAND);
        app.refresh_shaders();
        assert_eq!(app.shaders.effects[0].vars[name], value);
        app.shader_key(egui::Key::V, egui::Modifiers::COMMAND);
        app.refresh_shaders();
        assert_eq!(app.shaders.effects[0].vars[name][0]["start"], value);
        app.history_step(false);
        app.history_step(false);
        app.history_step(false);
        assert_eq!(app.editor.document().as_json(), &original);
    }
}
