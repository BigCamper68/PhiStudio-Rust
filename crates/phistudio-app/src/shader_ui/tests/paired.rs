use super::*;
use crate::{
    grid_input::{self, Grid},
    workbench::Tool,
};

fn canvas(app: &Workbench) -> Grid {
    grid_input::geometry(Rect::from_min_max(pos2(0., 80.), pos2(1000., 900.)), app)
}
fn route(app: &mut Workbench, grid: Grid, events: &[egui::Event], canceled: bool) {
    grid_input::handle(app, events, grid, grid.full_rect(), |_| true, canceled);
}
fn drag(app: &mut Workbench, grid: Grid, from: egui::Pos2, to: egui::Pos2) {
    route(
        app,
        grid,
        &[
            pointer(from, true),
            egui::Event::PointerMoved(to),
            pointer(to, false),
        ],
        false,
    );
    app.refresh_shaders();
}
fn animated(app: &mut Workbench) {
    app.shaders.preset = 9;
    app.add_shader();
    let draft = &mut app.shaders.draft.as_mut().unwrap().raw;
    for (name, start, end) in [
        ("radius", json!(1.), json!(5.)),
        ("color", json!([10, 20, 30, 255]), json!([40, 50, 60, 128])),
        ("position", json!([0., 0.]), json!([1., 1.])),
    ] {
        draft["vars"][name] = json!([{"startTime":[0,0,1],"endTime":[4,0,1],"start":start,"end":end,"easingType":2,"custom":"keep"}]);
    }
    assert!(app.flush_shader_draft());
    app.refresh_shaders();
}
#[test]
fn left_columns_keep_their_width_and_move_on_the_configurable_grid() {
    let mut app = app();
    app.add_shader();
    app.shaders.draft.as_mut().unwrap().raw["unknown"] = json!({"retained":true});
    assert!(app.flush_shader_draft());
    app.refresh_shaders();
    let original = app.editor.document().as_json().clone();
    let grid = canvas(&app);
    let col = surface::column(&app, grid, false, 0.);
    app.lanes = 5;
    assert_eq!(surface::column(&app, canvas(&app), false, 0.), col);
    let grid = canvas(&app);
    let from = pos2(col.center().x, grid.y(2.));
    drag(&mut app, grid, from, from + vec2(77., -96.));
    let raw = &app.shaders.source["effects"][0];
    assert_eq!(raw["unknown"], json!({"retained":true}));
    assert_eq!(raw["start"], json!([1, 0, 1]));
    assert!((surface::lane(raw, 0) - surface::snap(grid, 77. / 52.)).abs() < 0.001);
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &original);
}
#[test]
fn both_panes_resize_the_upper_edge_and_keyframe_bodies_move_whole_intervals() {
    let mut app = app();
    animated(&mut app);
    let original = app.editor.document().as_json().clone();
    let grid = canvas(&app);
    let x = effect_x(&app, grid, 0);
    drag(&mut app, grid, pos2(x, grid.y(4.)), pos2(x, grid.y(5.)));
    assert_eq!(app.shaders.effects[0].end, Beat::new(5, 0, 1).unwrap());
    app.history_step(false);
    app.refresh_shaders();
    let x = parameter_x(&app, grid, "radius");
    route(
        &mut app,
        grid,
        &[
            pointer(pos2(x, grid.y(2.)), true),
            pointer(pos2(x, grid.y(2.)), false),
        ],
        false,
    );
    let count = app.editor.revision();
    drag(&mut app, grid, pos2(x, grid.y(4.)), pos2(x, grid.y(5.)));
    assert_eq!(
        app.shaders.source["effects"][0]["vars"]["radius"][0]["endTime"],
        json!([5, 0, 1])
    );
    assert_eq!(app.editor.revision(), count + 1);
    app.history_step(false);
    app.refresh_shaders();
    drag(&mut app, grid, pos2(x, grid.y(2.)), pos2(x, grid.y(3.)));
    let raw = &app.shaders.source["effects"][0]["vars"]["radius"][0];
    assert_eq!(raw["startTime"], json!([1, 0, 1]));
    assert_eq!(raw["endTime"], json!([5, 0, 1]));
    assert_eq!(raw["custom"], "keep");
    assert_eq!(
        app.shaders.source["effects"][0]["vars"]["color"],
        original[EXTRA_KEY]["effects"][0]["vars"]["color"]
    );
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &original);
}
#[test]
fn placement_uses_the_touched_parameter_and_one_grid_tick_without_a_second_press() {
    for name in ["radius", "color"] {
        let mut app = app();
        app.shaders.preset = 9;
        app.add_shader();
        let original = app.editor.document().as_json().clone();
        let grid = canvas(&app);
        let x = parameter_x(&app, grid, name);
        app.set_tool(Tool::Event);
        let start = pos2(x, grid.y(1.));
        route(
            &mut app,
            grid,
            &[pointer(start, true), pointer(start, false)],
            false,
        );
        app.refresh_shaders();
        let row = &app.shaders.source["effects"][0]["vars"][name][0];
        let end: Beat = serde_json::from_value(row["endTime"].clone()).unwrap();
        assert_eq!(
            end,
            Beat::ONE
                .checked_add(Beat::new(0, 1, grid.division).unwrap())
                .unwrap()
        );
        assert_eq!(app.shaders.animation_track(), Some(name));
        let start = pos2(x, grid.y(2.));
        drag(&mut app, grid, start, pos2(x, grid.y(3.)));
        assert_eq!(
            app.shaders.source["effects"][0]["vars"][name][1]["endTime"],
            json!([3, 0, 1])
        );
        app.history_step(false);
        app.history_step(false);
        assert_eq!(app.editor.document().as_json(), &original);
    }
}
#[test]
fn boxes_across_uniforms_move_copy_and_delete_as_atomic_actions() {
    let mut app = app();
    animated(&mut app);
    let original = app.editor.document().as_json().clone();
    let grid = canvas(&app);
    app.set_tool(Tool::BoxSelect);
    drag(
        &mut app,
        grid,
        pos2(grid.event_rect.left() + 1., grid.y(4.2)),
        pos2(grid.event_rect.right() - 1., grid.y(0.1)),
    );
    assert_eq!(app.shaders.selection.len(), 3);
    let count = app.editor.history_rows().len();
    let x = parameter_x(&app, grid, "radius");
    drag(&mut app, grid, pos2(x, grid.y(2.)), pos2(x, grid.y(3.)));
    for name in ["radius", "color", "position"] {
        assert_eq!(
            app.shaders.source["effects"][0]["vars"][name][0]["startTime"],
            json!([1, 0, 1])
        );
    }
    assert_eq!(app.editor.history_rows().len(), count + 1);
    app.shader_key(egui::Key::C, egui::Modifiers::COMMAND);
    app.shader_key(egui::Key::Delete, egui::Modifiers::NONE);
    app.refresh_shaders();
    for name in ["radius", "color", "position"] {
        assert!(!is_animated(
            &app.shaders.source["effects"][0]["vars"][name]
        ));
    }
    app.shader_key(egui::Key::V, egui::Modifiers::COMMAND);
    app.refresh_shaders();
    for name in ["radius", "color", "position"] {
        assert!(is_animated(&app.shaders.source["effects"][0]["vars"][name]));
    }
    app.history_step(false);
    app.history_step(false);
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &original);
}
#[test]
fn canceled_gestures_leave_both_shader_and_uniform_data_unchanged() {
    let mut app = app();
    animated(&mut app);
    let grid = canvas(&app);
    let original = app.editor.document().as_json().clone();
    let count = app.editor.history_rows().len();
    for x in [effect_x(&app, grid, 0), parameter_x(&app, grid, "radius")] {
        let start = pos2(x, grid.y(2.));
        route(
            &mut app,
            grid,
            &[
                pointer(start, true),
                egui::Event::PointerMoved(start - vec2(0., 96.)),
            ],
            false,
        );
        route(&mut app, grid, &[], true);
        assert!(app.drag.is_none());
        assert_eq!(app.editor.document().as_json(), &original);
        assert_eq!(app.editor.history_rows().len(), count);
    }
}
#[test]
fn independent_horizontal_cameras_and_effect_jump_do_not_change_the_document() {
    let mut app = app();
    animated(&mut app);
    for n in 0..12 {
        app.shaders.draft.as_mut().unwrap().raw["vars"][format!("custom{n}")] = json!(0.);
    }
    app.shaders.draft.as_mut().unwrap().raw["phiStudioEditorX"] = json!(100.);
    assert!(app.flush_shader_draft());
    app.refresh_shaders();
    let original = app.editor.document().as_json().clone();
    let grid = canvas(&app);
    surface::pan(&mut app, grid, true, 152.);
    assert_eq!(app.shaders.offsets[0], 0.);
    assert!((app.shaders.offsets[1] - 2.).abs() < 0.001);
    surface::pan(&mut app, grid, false, -104.);
    assert_eq!(app.shaders.offsets[0], -2.);
    assert!((app.shaders.offsets[1] - 2.).abs() < 0.001);
    app.jump_shader(0, grid);
    let col = surface::column(&app, grid, false, 100.);
    assert!((col.center().x - grid.rect.center().x).abs() < 0.001);
    assert_eq!(app.shaders.offsets[1], 0.);
    assert_eq!(app.editor.document().as_json(), &original);
}

#[test]
fn shader_boxes_move_and_delete_effects_without_touching_notes_or_unknown_data() {
    let mut app = app();
    for _ in 0..2 {
        app.add_shader();
    }
    let original = app.editor.document().as_json().clone();
    let grid = canvas(&app);
    app.set_tool(Tool::BoxSelect);
    drag(
        &mut app,
        grid,
        pos2(grid.rect.left() + 1., grid.y(4.2)),
        pos2(grid.rect.left() + 103., grid.y(0.1)),
    );
    assert_eq!(app.shaders.selection.len(), 2);
    let count = app.editor.history_rows().len();
    drag(
        &mut app,
        grid,
        pos2(grid.rect.left() + 26., grid.y(2.)),
        pos2(grid.rect.left() + 26., grid.y(3.)),
    );
    assert_eq!(app.editor.history_rows().len(), count + 1);
    assert!(app.shaders.effects.iter().all(|e| e.start == Beat::ONE));
    app.shader_key(egui::Key::Delete, egui::Modifiers::NONE);
    app.refresh_shaders();
    assert!(app.shaders.effects.is_empty());
    assert_eq!(
        app.editor.document().as_json()["judgeLineList"],
        original["judgeLineList"]
    );
    app.history_step(false);
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &original);
}
#[test]
fn erasing_across_parameter_columns_and_tapping_the_divider_never_edit_notes() {
    let mut app = app();
    animated(&mut app);
    let original = app.editor.document().as_json().clone();
    let grid = canvas(&app);
    app.set_tool(Tool::Event);
    let divider = pos2(
        (grid.rect.right() + grid.event_rect.left()) / 2.,
        grid.y(2.),
    );
    route(
        &mut app,
        grid,
        &[pointer(divider, true), pointer(divider, false)],
        false,
    );
    assert_eq!(app.editor.document().as_json(), &original);
    app.set_tool(Tool::Erase);
    let count = app.editor.history_rows().len();
    drag(
        &mut app,
        grid,
        pos2(grid.event_rect.left() + 1., grid.y(2.)),
        pos2(grid.event_rect.right() - 1., grid.y(2.)),
    );
    for name in ["color", "radius", "position"] {
        assert!(!is_animated(
            &app.shaders.source["effects"][0]["vars"][name]
        ));
    }
    assert_eq!(app.editor.history_rows().len(), count + 1);
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &original);
}

#[test]
fn horizontal_shader_navigation_keeps_the_note_camera_and_empty_selection_clears_parameters() {
    let mut app = app();
    animated(&mut app);
    app.camera_x = 17.25;
    let grid = canvas(&app);
    let from = pos2(grid.rect.right() - 20., grid.y(2.));
    drag(&mut app, grid, from, from - vec2(104., 0.));
    assert_eq!(app.camera_x, 17.25);
    assert!((app.shaders.offsets[0] - 2.).abs() < 0.001);
    app.set_tool(Tool::BoxSelect);
    drag(
        &mut app,
        grid,
        pos2(grid.rect.left() + 1., grid.y(5.)),
        pos2(grid.rect.left() + 20., grid.y(4.5)),
    );
    assert!(app.shaders.selected.is_none());
    assert!(app.shaders.parameters.is_empty());
}
