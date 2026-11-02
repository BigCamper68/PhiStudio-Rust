use crate::{
    grid_input::{self, Grid},
    workbench::{Tool, Workbench},
};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, pos2, vec2};
use phistudio_core::{Beat, ChartDocument, Command, EventType, LineEvent, Note, NoteId, NoteKind};

fn app() -> Workbench {
    let mut app = Workbench::new(ChartDocument::empty("Multiple", 120.).unwrap(), None);
    app.execute(Command::AddLine {
        name: "Second".into(),
    });
    app.multi_line.enabled = true;
    app.multi_line.lines = vec![0, 1];
    app.multi_line.widths = [180., 220.];
    app.set_tool(Tool::Select);
    app
}
fn grid(app: &Workbench) -> Grid {
    grid_input::geometry(Rect::from_min_size(pos2(0., 60.), vec2(1100., 650.)), app)
}
fn input(app: &mut Workbench, events: Vec<Event>) {
    let g = grid(app);
    grid_input::handle(app, &events, g, g.full_rect(), |_| true, false);
}
fn button(pos: Pos2, pressed: bool, add: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers {
            ctrl: add,
            ..Default::default()
        },
    }
}
fn click(app: &mut Workbench, p: Pos2, add: bool) {
    input(app, vec![button(p, true, add), button(p, false, add)]);
}
fn note(app: &mut Workbench, line: usize, x: f64) {
    app.execute(Command::AddNote {
        line,
        note: Note {
            start_time: Beat::new(2, 0, 1).unwrap(),
            end_time: Beat::new(2, 0, 1).unwrap(),
            x,
            ..Default::default()
        },
    });
}

#[test]
fn line_expression_supports_ranges_all_and_quoted_groups_without_chart_edits() {
    let mut app = app();
    app.execute(Command::AddLine {
        name: "Third".into(),
    });
    let raw = app.editor.document().as_json().clone();
    for (s, expected) in [
        ("0:2", vec![0, 1, 2]),
        ("ALL", vec![0, 1, 2]),
        ("2 0 2", vec![0, 2]),
        ("0-1, 2", vec![0, 1, 2]),
    ] {
        assert_eq!(
            crate::multi_line::parse_lines(s, app.editor.document()).unwrap(),
            expected
        );
    }
    for s in ["", "3", "2:0", "bogus", "\"unterminated"] {
        assert!(crate::multi_line::parse_lines(s, app.editor.document()).is_err());
    }
    assert_eq!(app.editor.document().as_json(), &raw);
}

#[test]
fn columns_place_pick_and_drag_on_the_actual_line() {
    let mut app = app();
    app.set_tool(Tool::Place(NoteKind::Tap));
    let g = grid(&app);
    let panels = app.line_panels(g, false);
    for (line, panel) in &panels {
        click(&mut app, pos2(panel.x(0.), panel.y(2.)), false);
        assert_eq!(app.selected.unwrap().line, *line);
    }
    assert_eq!(app.editor.document().notes(0).len(), 1);
    assert_eq!(app.editor.document().notes(1).len(), 1);
    app.set_tool(Tool::Select);
    let (_, p) = panels[1];
    let start = pos2(p.x(0.), p.y(2.));
    click(&mut app, start, false);
    let raw = app.editor.document().as_json().clone();
    input(
        &mut app,
        vec![
            button(start, true, false),
            Event::PointerMoved(start - vec2(0., 96.)),
            button(start - vec2(0., 96.), false, false),
        ],
    );
    assert_eq!(
        app.editor.document().note(1, 0).unwrap().start_time,
        Beat::new(3, 0, 1).unwrap()
    );
    assert_eq!(
        app.editor.document().note(0, 0).unwrap().start_time,
        Beat::new(2, 0, 1).unwrap()
    );
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &raw);
}

#[test]
fn overlay_cycles_across_lines_and_additive_selection_is_preserved() {
    let mut app = app();
    app.multi_line.merged = true;
    note(&mut app, 0, 0.);
    note(&mut app, 1, 0.);
    let g = grid(&app);
    let p = pos2(g.x(0.), g.y(2.));
    let raw = app.editor.document().as_json().clone();
    for line in [1, 0, 1, 0] {
        click(&mut app, p, false);
        assert_eq!(app.selected, Some(NoteId { line, index: 0 }));
    }
    click(&mut app, p, true);
    assert_eq!(app.selection().notes.len(), 2);
    assert_eq!(app.editor.document().as_json(), &raw);
}

#[test]
fn cross_column_box_copy_paste_move_and_erase_are_sparse_single_undo_actions() {
    let mut app = app();
    note(&mut app, 0, 0.);
    note(&mut app, 1, 0.);
    let g = grid(&app);
    let panels = app.line_panels(g, false);
    app.set_tool(Tool::BoxSelect);
    let a = pos2(panels[0].1.rect.left() + 3., g.y(3.));
    let b = pos2(panels[1].1.rect.right() - 3., g.y(1.));
    input(
        &mut app,
        vec![
            button(a, true, false),
            Event::PointerMoved(b),
            button(b, false, false),
        ],
    );
    assert_eq!(app.selection().notes.len(), 2);
    app.copy_selection(false);
    app.transport.beat = 5.;
    let raw = app.editor.document().as_json().clone();
    let count = app.editor.history_cursor();
    app.paste_selection(false, false);
    assert_eq!(app.editor.history_cursor(), count + 1);
    assert_eq!(app.editor.document().notes(0).len(), 2);
    assert_eq!(app.editor.document().notes(1).len(), 2);
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &raw);
    app.transport.beat = 0.;
    app.set_tool(Tool::Erase);
    let a = pos2(panels[0].1.x(0.), g.y(2.));
    let b = pos2(panels[1].1.x(0.), g.y(2.));
    let count = app.editor.history_cursor();
    input(
        &mut app,
        vec![
            button(a, true, false),
            Event::PointerMoved(b),
            button(b, false, false),
        ],
    );
    assert!(app.editor.document().notes(0).is_empty() && app.editor.document().notes(1).is_empty());
    assert_eq!(app.editor.history_cursor(), count + 1);
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &raw);
}

#[test]
fn events_on_different_lines_select_replace_extend_and_delete_together() {
    let mut app = app();
    app.multi_line.events = true;
    for line in [0, 1] {
        app.execute(Command::AddEvent {
            line,
            layer: 0,
            kind: EventType::MoveX,
            event: LineEvent {
                start_time: Beat::new(2, 0, 1).unwrap(),
                end_time: Beat::new(4, 0, 1).unwrap(),
                ..Default::default()
            },
        });
    }
    let g = grid(&app);
    let panels = app.line_panels(g, true);
    let point = |i: usize| pos2(panels[i].1.event_rect.left() + 22., g.y(3.));
    click(&mut app, point(0), false);
    assert_eq!(app.selection().events.len(), 1);
    click(&mut app, point(1), false);
    assert_eq!(app.selection().events.len(), 1);
    assert_eq!(app.events.selected.unwrap().line, 1);
    click(&mut app, point(0), true);
    assert_eq!(app.selection().events.len(), 2);
    let raw = app.editor.document().as_json().clone();
    app.delete_selected();
    for line in [0, 1] {
        assert!(
            !app.editor
                .document()
                .events(line, 0, EventType::MoveX)
                .unwrap()
                .iter()
                .any(|(_, e)| e.start_time == Beat::new(2, 0, 1).unwrap())
        );
    }
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &raw);
}

#[test]
fn delete_line_repairs_and_undo_restores_the_line_view_and_preferences() {
    let mut app = app();
    app.execute(Command::AddLine {
        name: "Third".into(),
    });
    app.multi_line.lines = vec![0, 2];
    app.line = 2;
    app.commit_authoring(Command::DeleteLine(1));
    assert_eq!(app.multi_line.lines, vec![0, 1]);
    assert_eq!(app.line, 1);
    app.history_step(false);
    assert_eq!(app.line, 2);
    assert_eq!(app.multi_line.lines, vec![0, 2]);
    app.history_step(true);
    assert_eq!(app.multi_line.lines, vec![0, 1]);
    let root = tempfile::tempdir().unwrap();
    let prefs = app.preferences();
    phistudio_core::save_editor_preferences(root.path(), &prefs).unwrap();
    let loaded = phistudio_core::load_editor_preferences(root.path()).unwrap();
    assert_eq!(loaded.multi_line, prefs.multi_line);
}

#[test]
fn all_and_invert_respect_the_view_and_group_moves_keep_every_source_line() {
    let mut app = app();
    note(&mut app, 0, -270.);
    note(&mut app, 1, 270.);
    app.select_scope(crate::toolkit::Scope::Notes, false);
    assert_eq!(app.selection().notes.len(), 2);
    let raw = app.editor.document().as_json().clone();
    app.transform_selection(phistudio_core::tools::Transform::Shift {
        beats: Beat::new(1, 0, 1).unwrap(),
        x: 0.,
    });
    for line in [0, 1] {
        assert_eq!(
            app.editor.document().note(line, 0).unwrap().start_time,
            Beat::new(3, 0, 1).unwrap()
        );
    }
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &raw);
    app.select_scope(crate::toolkit::Scope::Notes, false);
    app.multi_line.lines = vec![0];
    app.reconcile_multi_selection();
    assert_eq!(app.selection().notes, vec![NoteId { line: 0, index: 0 }]);
    app.multi_line.lines = vec![0, 1];
    app.select_scope(crate::toolkit::Scope::Notes, false);
    app.select_scope(crate::toolkit::Scope::Notes, true);
    assert!(app.selection().is_empty());
}

#[test]
fn multi_line_window_shows_full_height_list_in_every_language_and_saved_geometry() {
    for language in phistudio_core::Language::ALL {
        for size in [vec2(873., 393.), vec2(582., 262.)] {
            for restored in [false, true] {
                let mut raw = ChartDocument::empty("Line list", 120.)
                    .unwrap()
                    .as_json()
                    .clone();
                let line = raw["judgeLineList"][0].clone();
                raw["judgeLineList"] = serde_json::Value::Array(vec![line; 16]);
                let mut app = Workbench::new(ChartDocument::from_value(raw.clone()).unwrap(), None);
                app.options.language = language;
                app.multi_line.show = true;
                let revision = app.editor.revision();
                let ctx = egui::Context::default();
                ctx.set_style(crate::theme::style());
                ctx.set_fonts(crate::fonts::definitions());
                let input = || egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    ..Default::default()
                };
                if restored {
                    let _ = ctx.run(input(), |ctx| {
                        egui::Window::new("Multiple lines")
                            .id(egui::Id::new("Multiple lines"))
                            .default_pos([48., 56.])
                            .fixed_size([410., 180.])
                            .show(ctx, |ui| {
                                ui.label("Saved geometry");
                            });
                    });
                }
                for _ in 0..6 {
                    let _ = ctx.run(input(), |ctx| app.ui(ctx, false));
                }
                let out = ctx.run(input(), |ctx| app.ui(ctx, false));
                let rect =
                    crate::window::visible_rect(&ctx, egui::Id::new("Multiple lines")).unwrap();
                assert!(
                    rect.left() >= -1.
                        && rect.right() <= size.x + 1.
                        && rect.top() >= -1.
                        && rect.bottom() <= size.y + 1.,
                    "{language:?} {size:?}, restored={restored}: {rect:?}"
                );
                let visible_rows = |out: &egui::FullOutput| {
                    out.shapes
                        .iter()
                        .filter(|shape| match &shape.1 {
                            egui::Shape::Text(t)
                                if t.galley.job.text.starts_with('L')
                                    && t.galley.job.text.contains(" · ") =>
                            {
                                let text = Rect::from_min_size(t.pos, t.galley.size());
                                let row =
                                    Rect::from_center_size(text.center(), vec2(text.width(), 32.));
                                shape.0.contains_rect(row) && rect.contains_rect(row)
                            }
                            _ => false,
                        })
                        .count()
                };
                let minimum_rows = if size.y > 300. { 7 } else { 4 };
                assert!(
                    visible_rows(&out) >= minimum_rows,
                    "{language:?} {size:?}, restored={restored}: only {} full rows visible",
                    visible_rows(&out)
                );
                // Scrolling settings must leave the line-list viewport in place.
                let first_row = out
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.1 {
                        egui::Shape::Text(t) if t.galley.job.text.starts_with("L0 · ") => {
                            Some(Rect::from_min_size(t.pos, t.galley.size()).center())
                        }
                        _ => None,
                    })
                    .unwrap();
                let list = crate::scroll::at(&ctx, first_row).unwrap();
                let controls = ctx
                    .data_mut(|d| {
                        d.get_temp::<Vec<crate::scroll::Zone>>(egui::Id::new(
                            "phistudio-scroll-zones",
                        ))
                    })
                    .unwrap()
                    .into_iter()
                    .find(|zone| zone.layer == list.layer && zone.rect.right() < list.rect.left());
                if let Some(controls) = controls {
                    assert!(!controls.rect.intersects(list.rect));
                    crate::scroll::move_by(&ctx, controls, -controls.max_offset);
                } else {
                    assert!(size.y > 300., "Compact settings must register scrolling");
                }
                let out = ctx.run(input(), |ctx| app.ui(ctx, false));
                assert!(visible_rows(&out) >= minimum_rows);
                let state = egui::scroll_area::State::load(&ctx, list.id).unwrap();
                assert_eq!(state.offset.y, 0.);
                assert_eq!(app.editor.document().as_json(), &raw);
                assert_eq!(app.editor.revision(), revision);
                assert!(!app.editor.is_dirty());
            }
        }
    }
}

#[test]
fn event_creation_hold_resize_and_horizontal_navigation_use_the_owning_panel() {
    let mut app = app();
    app.multi_line.events = true;
    app.set_tool(Tool::Event);
    let g = grid(&app);
    let (_, panel) = app.line_panels(g, true)[1];
    let a = pos2(panel.event_rect.left() + 22., g.y(5.));
    let b = pos2(a.x, g.y(6.));
    input(
        &mut app,
        vec![
            button(a, true, false),
            Event::PointerMoved(b),
            button(b, false, false),
        ],
    );
    assert!(
        app.editor
            .document()
            .events(1, 0, EventType::MoveX)
            .unwrap()
            .iter()
            .any(|(_, e)| e.start_time == Beat::new(5, 0, 1).unwrap())
    );
    assert!(
        !app.editor
            .document()
            .events(0, 0, EventType::MoveX)
            .unwrap()
            .iter()
            .any(|(_, e)| e.start_time == Beat::new(5, 0, 1).unwrap())
    );
    app.multi_line.events = false;
    app.set_tool(Tool::Place(NoteKind::Hold));
    let (_, panel) = app.line_panels(g, false)[1];
    let a = pos2(panel.x(0.), g.y(2.));
    let b = pos2(a.x, g.y(4.));
    input(
        &mut app,
        vec![
            button(a, true, false),
            Event::PointerMoved(b),
            button(b, false, false),
        ],
    );
    assert_eq!(
        app.editor.document().note(1, 0).unwrap().end_time,
        Beat::new(4, 0, 1).unwrap()
    );
    app.set_tool(Tool::Select);
    let c = pos2(a.x, g.y(5.));
    input(
        &mut app,
        vec![
            button(b, true, false),
            Event::PointerMoved(c),
            button(c, false, false),
        ],
    );
    assert_eq!(
        app.editor.document().note(1, 0).unwrap().end_time,
        Beat::new(5, 0, 1).unwrap()
    );
    app.multi_line.events = true;
    app.select(None);
    let start = grid(&app).rect.center();
    let camera = app.timeline.camera_x;
    input(
        &mut app,
        vec![
            button(start, true, false),
            Event::PointerMoved(start - vec2(80., 0.)),
            button(start - vec2(80., 0.), false, false),
        ],
    );
    assert_ne!(
        app.timeline.camera_x, camera,
        "Normal notes camera must still pan while events use columns"
    );
    assert_eq!(app.multi_line.offsets[1], 0.);
}

#[test]
fn touch_taps_create_in_two_columns_and_vertical_swipes_do_not_place_notes() {
    let mut app = app();
    app.set_tool(Tool::Place(NoteKind::Tap));
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    let size = vec2(1194., 834.);
    let run = |app: &mut Workbench, events, canceled| {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| app.ui(ctx, canceled),
        );
    };
    for _ in 0..4 {
        run(&mut app, vec![], false);
    }
    let g = grid_input::geometry(app.timeline.area, &app);
    let panels = app.line_panels(g, false);
    let mut router = crate::touch::TouchRouter::default();
    for (i, (_, panel)) in panels.iter().enumerate() {
        let p = pos2(panel.x(0.), panel.y(2.));
        for (phase, t) in [(egui::TouchPhase::Start, 1.), (egui::TouchPhase::End, 1.05)] {
            let (events, canceled) = router.event_with_context(&ctx, phase, 1, p, t + i as f64);
            run(&mut app, events, canceled);
        }
    }
    assert_eq!(app.editor.document().notes(0).len(), 1);
    assert_eq!(app.editor.document().notes(1).len(), 1);
    let p = pos2(panels[0].1.x(0.), panels[0].1.y(5.));
    for (phase, delta, t) in [
        (egui::TouchPhase::Start, 0., 4.),
        (egui::TouchPhase::Move, 40., 4.05),
        (egui::TouchPhase::End, 70., 4.1),
    ] {
        let (events, canceled) = router.event_with_context(&ctx, phase, 1, p + vec2(0., delta), t);
        run(&mut app, events, canceled);
    }
    assert_eq!(app.editor.document().note_count(), 2);
}

fn storyboard_app() -> Workbench {
    let mut app = app();
    app.open_storyboard();
    app.multi_line.events = true;
    app
}
fn story(app: &mut Workbench, line: usize) {
    use phistudio_core::{StoryEvent, StoryKind};
    app.execute(Command::EditStoryboard {
        line,
        kind: StoryKind::ScaleX,
        index: None,
        event: Some(StoryEvent::new(
            StoryKind::ScaleX,
            Beat::ONE,
            Beat::new(3, 0, 1).unwrap(),
        )),
    });
}
#[test]
fn storyboard_columns_create_on_the_pressed_line_and_cancel_without_committing() {
    use phistudio_core::StoryKind;
    let mut app = storyboard_app();
    let g = grid(&app);
    let panels = app.line_panels(g, true);
    assert_eq!(panels.len(), 2);
    app.set_tool(Tool::Event);
    for (line, panel) in panels {
        let x = panel.event_rect.left() + panel.event_rect.width() / 12.;
        let a = pos2(x, g.y(1.));
        let b = pos2(x, g.y(3.));
        let before = app.editor.document().as_json().clone();
        input(
            &mut app,
            vec![button(a, true, false), Event::PointerMoved(b)],
        );
        assert_eq!(app.editor.document().as_json(), &before);
        grid_input::handle(&mut app, &[], g, g.full_rect(), |_| true, true);
        assert_eq!(app.editor.document().as_json(), &before);
        input(
            &mut app,
            vec![
                button(a, true, false),
                Event::PointerMoved(b),
                button(b, false, false),
            ],
        );
        let rows = app
            .editor
            .document()
            .story_events(line, StoryKind::ScaleX)
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].start_time, Beat::ONE);
        assert_eq!(rows[0].end_time, Beat::new(3, 0, 1).unwrap());
    }
}
#[test]
fn storyboard_multi_box_group_move_copy_delete_and_undo_keep_source_lines() {
    use phistudio_core::StoryKind;
    let mut app = storyboard_app();
    for line in [0, 1] {
        story(&mut app, line);
    }
    let source = app.editor.document().as_json().clone();
    let g = grid(&app);
    let panels = app.line_panels(g, true);
    app.set_tool(Tool::BoxSelect);
    let a = pos2(panels[0].1.event_rect.left() + 2., g.y(0.5));
    let b = pos2(panels[1].1.event_rect.right() - 2., g.y(4.));
    input(
        &mut app,
        vec![
            button(a, true, false),
            Event::PointerMoved(b),
            button(b, false, false),
        ],
    );
    assert_eq!(
        app.story_selection()
            .iter()
            .map(|id| id.line)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    let a = pos2(
        panels[0].1.event_rect.left() + panels[0].1.event_rect.width() / 12.,
        g.y(2.),
    );
    let b = pos2(a.x, g.y(4.));
    input(
        &mut app,
        vec![
            button(a, true, false),
            Event::PointerMoved(b),
            button(b, false, false),
        ],
    );
    for line in [0, 1] {
        assert_eq!(
            app.editor
                .document()
                .story_events(line, StoryKind::ScaleX)
                .unwrap()[0]
                .start_time,
            Beat::new(3, 0, 1).unwrap()
        );
    }
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &source);
    app.select_all_stories(false);
    assert_eq!(app.story_selection().len(), 2);
    app.copy_selection(false);
    assert_eq!(app.tools.clipboard.as_ref().unwrap().len(), 2);
    app.transport.beat = 8.;
    app.paste_selection(false, false);
    for line in [0, 1] {
        assert_eq!(
            app.editor
                .document()
                .story_events(line, StoryKind::ScaleX)
                .unwrap()
                .len(),
            2
        );
    }
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &source);
    app.select_all_stories(false);
    app.delete_selected();
    for line in [0, 1] {
        assert!(
            app.editor
                .document()
                .story_events(line, StoryKind::ScaleX)
                .unwrap()
                .is_empty()
        );
    }
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &source);
    app.select_all_stories(false);
    app.reconcile_multi_selection();
    assert_eq!(app.story_selection().len(), 2);
    app.multi_line.lines = vec![1];
    app.reconcile_multi_selection();
    assert_eq!(
        app.story_selection()
            .iter()
            .map(|id| id.line)
            .collect::<Vec<_>>(),
        vec![1]
    );
}
#[test]
fn storyboard_erase_across_columns_and_horizontal_navigation_use_the_shared_view() {
    use phistudio_core::StoryKind;
    let mut app = storyboard_app();
    for line in [0, 1] {
        story(&mut app, line);
    }
    let source = app.editor.document().as_json().clone();
    let g = grid(&app);
    let panels = app.line_panels(g, true);
    let points: Vec<_> = panels
        .iter()
        .map(|(_, p)| pos2(p.event_rect.left() + p.event_rect.width() / 12., g.y(2.)))
        .collect();
    app.set_tool(Tool::Erase);
    input(
        &mut app,
        vec![
            button(points[0], true, false),
            Event::PointerMoved(points[1]),
            button(points[1], false, false),
        ],
    );
    for line in [0, 1] {
        assert!(
            app.editor
                .document()
                .story_events(line, StoryKind::ScaleX)
                .unwrap()
                .is_empty()
        );
    }
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &source);
    app.set_tool(Tool::Select);
    app.options.lock_horizontal = true;
    app.multi_line.widths[1] = 500.;
    let panels = app.line_panels(g, true);
    let a = pos2(panels[0].1.event_rect.center().x, g.y(5.));
    let b = a - vec2(70., 0.);
    let beat = app.transport.beat;
    input(
        &mut app,
        vec![
            button(a, true, false),
            Event::PointerMoved(b),
            button(b, false, false),
        ],
    );
    assert!(app.multi_line.offsets[1] > 0.);
    assert_eq!(app.transport.beat, beat);
    assert_eq!(app.editor.document().as_json(), &source);
}
