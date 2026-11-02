use super::*;
use crate::workbench::Workbench;
use egui::{Pos2, Rect, vec2};
use phistudio_core::{ChartDocument, Command, Language};

fn app() -> Workbench {
    let mut raw = ChartDocument::empty("Line UI", 120.)
        .unwrap()
        .as_json()
        .clone();
    let line = raw["judgeLineList"][0].clone();
    raw["judgeLineList"] = serde_json::Value::Array(vec![line; 3]);
    for i in 0..3 {
        raw["judgeLineList"][i]["Name"] = serde_json::json!(format!("Original {i}"));
    }
    Workbench::new(ChartDocument::from_value(raw).unwrap(), None)
}
fn frame(app: &mut Workbench, ctx: &Context, size: egui::Vec2) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            ..Default::default()
        },
        |ctx| app.ui(ctx, false),
    )
}
fn tap_text(app: &mut Workbench, ctx: &Context, size: egui::Vec2, text: &str, time: f64) {
    let out = frame(app, ctx, size);
    let point = out
        .shapes
        .iter()
        .rev()
        .find_map(|shape| match &shape.1 {
            egui::Shape::Text(t) if t.galley.job.text == text => {
                let rect = Rect::from_min_size(t.pos, t.galley.size());
                shape.0.contains_rect(rect).then_some(rect.center())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("Visible control: {text}"));
    let mut router = crate::touch::TouchRouter::default();
    for (phase, time) in [
        (egui::TouchPhase::Start, time),
        (egui::TouchPhase::End, time + 0.02),
    ] {
        let (events, canceled) = router.event_with_context(ctx, phase, 1, point, time);
        assert!(!canceled);
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| app.ui(ctx, false),
        );
    }
}

#[test]
fn structural_actions_follow_line_identity_and_restore_multi_view_with_history_jump() {
    let mut app = app();
    let raw = app.editor.document().as_json().clone();
    app.options.history_jump = true;
    app.multi_line.enabled = true;
    app.multi_line.lines = vec![0, 2];
    app.line = 2;
    assert!(app.commit_authoring(Command::MoveLine { from: 2, to: 0 }));
    assert_eq!(app.line, 0);
    assert_eq!(app.multi_line.lines, vec![0, 1]);
    assert_eq!(
        app.editor.document().as_json()["judgeLineList"][0]["Name"],
        "Original 2"
    );
    app.history_step(false);
    assert_eq!(app.line, 2);
    assert_eq!(app.multi_line.lines, vec![0, 2]);
    assert_eq!(app.editor.document().as_json(), &raw);
    app.history_step(true);
    assert_eq!(app.line, 0);
    assert!(app.commit_authoring(Command::DuplicateLine(0)));
    assert_eq!(app.line, 3);
    assert_eq!(app.multi_line.lines, vec![0, 1, 3]);
    app.history_step(false);
    assert_eq!(app.line, 0);
    assert_eq!(app.multi_line.lines, vec![0, 1]);
    assert!(app.commit_authoring(Command::AddLines { count: 4 }));
    assert_eq!(app.line, 3);
    assert_eq!(app.editor.document().line_count(), 7);
    app.history_step(false);
    assert_eq!(app.line, 0);
    app.history_step(true);
    assert_eq!(app.line, 3);
}

#[test]
fn texture_selection_preserves_drafted_fields_and_only_apply_records_chart_history() {
    let mut app = app();
    app.windows.line_properties = true;
    let raw = app.editor.document().as_json().clone();
    let ctx = Context::default();
    for _ in 0..4 {
        frame(&mut app, &ctx, vec2(873., 393.));
    }
    let draft = app.line_draft.as_mut().unwrap();
    draft.properties.name = "Drafted name".into();
    draft.properties.anchor = [0., 1.];
    app.open_line_textures();
    app.stage_line_texture("pictures/图.png".into());
    let properties = app.line_draft.as_ref().unwrap().properties.clone();
    assert_eq!(properties.name, "Drafted name");
    assert_eq!(properties.anchor, [0., 1.]);
    assert_eq!(properties.texture, "pictures/图.png");
    assert_eq!(app.editor.document().as_json(), &raw);
    assert!(!app.editor.is_dirty());
    app.commit_authoring(Command::EditLine {
        index: 0,
        properties,
    });
    assert_eq!(app.editor.history_cursor(), 1);
    app.history_step(false);
    assert_eq!(app.editor.document().as_json(), &raw);
}

#[test]
fn stale_texture_selection_cannot_edit_another_revision_or_project() {
    let mut app = app();
    app.windows.line_properties = true;
    let ctx = Context::default();
    for _ in 0..4 {
        frame(&mut app, &ctx, vec2(873., 393.));
    }
    app.open_line_textures();
    app.execute(Command::AddLine {
        name: "Other".into(),
    });
    app.stage_line_texture("stale.png".into());
    assert_eq!(
        app.line_draft.as_ref().unwrap().properties.texture,
        "line.png"
    );
    assert!(!app.line_tools.show);
    app.replace_document(ChartDocument::empty("New", 120.).unwrap(), None, None);
    app.stage_line_texture("wrong-project.png".into());
    assert_eq!(
        app.editor.document().line_properties(0).unwrap().texture,
        "line.png"
    );
    assert!(!app.editor.is_dirty());
}

#[test]
fn touching_a_visual_texture_tile_stages_it_without_editing_the_chart() {
    let mut app = app();
    app.windows.line_properties = true;
    let raw = app.editor.document().as_json().clone();
    let ctx = Context::default();
    let size = vec2(873., 393.);
    for _ in 0..5 {
        frame(&mut app, &ctx, size);
    }
    app.line_draft.as_mut().unwrap().properties.texture = "prior.png".into();
    app.open_line_textures();
    for _ in 0..5 {
        frame(&mut app, &ctx, size);
    }
    let out = frame(&mut app, &ctx, size);
    let tile = out
        .shapes
        .iter()
        .find_map(|shape| match &shape.1 {
            egui::Shape::Text(t) if t.galley.job.text == "Default line" => {
                Some(Rect::from_min_size(t.pos, t.galley.size()).center())
            }
            _ => None,
        })
        .unwrap();
    let mut router = crate::touch::TouchRouter::default();
    for (phase, time) in [(egui::TouchPhase::Start, 1.), (egui::TouchPhase::End, 1.02)] {
        let (events, canceled) = router.event_with_context(&ctx, phase, 1, tile, time);
        assert!(!canceled);
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| app.ui(ctx, false),
        );
    }
    assert!(!app.line_tools.show);
    assert_eq!(
        app.line_draft.as_ref().unwrap().properties.texture,
        "line.png"
    );
    assert_eq!(app.editor.document().as_json(), &raw);
    assert!(!app.editor.is_dirty());
}

#[test]
fn line_properties_and_texture_picker_fit_compact_screens_in_every_language() {
    for language in Language::ALL {
        let _scope = crate::i18n::Scope::enter(language);
        for size in [vec2(873., 393.), vec2(582., 262.)] {
            let mut app = app();
            app.options.language = language;
            app.windows.line_properties = true;
            let raw = app.editor.document().as_json().clone();
            let ctx = Context::default();
            ctx.set_style(crate::theme::style());
            ctx.set_fonts(crate::fonts::definitions());
            for _ in 0..6 {
                frame(&mut app, &ctx, size);
            }
            tap_text(&mut app, &ctx, size, &crate::i18n::tr("Manage lines"), 1.);
            for _ in 0..6 {
                frame(&mut app, &ctx, size);
            }
            app.line_tools.count = 3;
            tap_text(&mut app, &ctx, size, &crate::i18n::tr("Create lines"), 2.);
            assert_eq!(app.editor.document().line_count(), 6);
            assert_eq!(app.line, 3);
            app.history_step(false);
            assert_eq!(app.editor.document().as_json(), &raw);
            for _ in 0..4 {
                frame(&mut app, &ctx, size);
            }
            let bounds =
                crate::window::visible_rect(&ctx, egui::Id::new("Judge line properties")).unwrap();
            assert!(
                bounds.left() >= -1.
                    && bounds.right() <= size.x + 1.
                    && bounds.top() >= -1.
                    && bounds.bottom() <= size.y + 1.,
                "{language:?}/{size:?}: {bounds:?}"
            );
            app.open_line_textures();
            for _ in 0..6 {
                frame(&mut app, &ctx, size);
            }
            let bounds =
                crate::window::visible_rect(&ctx, egui::Id::new("Choose line texture")).unwrap();
            assert!(
                bounds.left() >= -1.
                    && bounds.right() <= size.x + 1.
                    && bounds.top() >= -1.
                    && bounds.bottom() <= size.y + 1.,
                "{language:?}/{size:?}: {bounds:?}"
            );
            let zones = ctx
                .data_mut(|d| {
                    d.get_temp::<Vec<crate::scroll::Zone>>(egui::Id::new("phistudio-scroll-zones"))
                })
                .unwrap();
            for zone in zones.iter().filter(|z| bounds.contains_rect(z.rect)) {
                let state = egui::scroll_area::State::load(&ctx, zone.id).unwrap();
                assert_eq!(state.offset.x, 0.);
            }
            app.back();
            assert!(!app.line_tools.show && app.windows.line_properties);
            assert_eq!(app.editor.document().as_json(), &raw);
            assert!(!app.editor.is_dirty());
        }
    }
}

#[test]
fn thumbnail_worker_limits_pending_jobs_and_discards_previous_project_results() {
    use std::time::{Duration, Instant};
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let bytes = include_bytes!("../../../../assets/skins/phigros/click.png");
    for i in 0..48 {
        std::fs::write(first.path().join(format!("image-{i:02}.png")), bytes).unwrap();
    }
    std::fs::write(second.path().join("fresh.png"), bytes).unwrap();
    let ctx = Context::default();
    let mut state = State::default();
    state.sync(&ctx, 1, 0, Some(first.path().to_owned()));
    let start = Instant::now();
    while state.names.is_none() {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(2));
        state.sync(&ctx, 1, 0, Some(first.path().to_owned()));
    }
    let names = state.names.clone().unwrap();
    assert_eq!(names.len(), 48);
    for name in names {
        state.image(&ctx, &name);
    }
    assert_eq!(state.pending, 24);
    assert_eq!(state.entries.len(), 24);
    state.sync(&ctx, 2, 0, Some(second.path().to_owned()));
    assert_eq!(state.pending, 0);
    assert!(state.entries.is_empty());
    let start = Instant::now();
    while state.names.is_none() {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(2));
        state.sync(&ctx, 2, 0, Some(second.path().to_owned()));
    }
    assert_eq!(state.names.as_ref().unwrap(), &vec!["fresh.png".to_owned()]);
    state.image(&ctx, "fresh.png");
    let start = Instant::now();
    while state.pending > 0 {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(2));
        state.sync(&ctx, 2, 0, Some(second.path().to_owned()));
    }
    assert_eq!(state.entries.len(), 1);
    assert!(state.entries["fresh.png"].image.is_some());
}
