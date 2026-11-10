//! Exercise the real GPU window and native clock, keeping all fixtures private.
use super::*;
use crate::gui::Gui;
use anyhow::{Result, ensure};
use macroquad::prelude::*;
use phistudio_core::{ChartDocument, Language, NoteKind};
use std::path::Path;
pub(crate) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut phistudio_prpr::Preview,
    out: &Path,
) -> Result<()> {
    let mut raw = app.editor.document().as_json().clone();
    raw["judgeLineList"][0]["notes"][0]["startTime"] = serde_json::json!([4, 0, 1]);
    raw["judgeLineList"][0]["notes"][0]["endTime"] = serde_json::json!([4, 0, 1]);
    raw["judgeLineList"][0]["eventLayers"][0]["speedEvents"] = serde_json::json!([{"startTime":[0,0,1],"endTime":[8,0,1],"start":10.,"end":10.,"easingType":1}]);
    let fixture = ChartDocument::from_value(raw)?;
    app.replace_document(fixture, None, None);
    app.select(Some(NoteId { line: 0, index: 0 }));
    app.project_options.use_rpe_170_speed = false;
    app.open_speed_curve();
    preview
        .replace_chart(app.editor.document(), None, &app.project_options)
        .await?;
    let mut revision = app.editor.revision();
    let original = app.editor.document().as_json().clone();
    let mut added = false;
    let mut down = None;
    let mut modal_input = crate::runtime::ModalInput::default();
    let mut input_scale = 1.;
    let mut viewport = egui::Rect::NOTHING;
    let mut plot: Option<egui::Rect> = None;
    let mut window_rect: Option<egui::Rect> = None;
    for frame in 0..100 {
        if modal_input.update(app) {
            gui.cancel_input();
        }
        gui.set_ui_scale(app.options.ui_scale);
        if app.editor.revision() != revision {
            preview
                .replace_chart(app.editor.document(), None, &app.project_options)
                .await?;
            revision = app.editor.revision();
        }
        app.speed_curve
            .record_native_clock(app.session, revision, preview.speed_curve_horizon());
        if frame > 6 && !added && !app.speed_curve.busy() {
            let profile = app.speed_curve.profile.as_ref().unwrap();
            let rect = plot;
            if let Some(rect) = rect {
                app.speed_curve.mode = Mode::Add;
                // Stay visibly away from the existing curve even on a short,
                // landscape canvas, where 100 distance units are just a few pixels.
                let distance = super::graph::distance_at(profile, 2.) - app.speed_curve.limit * 0.6;
                let pos = egui::pos2(
                    rect.left()
                        + ((2. - app.speed_curve.start) / app.speed_curve.span) as f32
                            * rect.width(),
                    rect.center().y
                        - (distance / app.speed_curve.limit) as f32 * rect.height() * 0.5,
                );
                down = Some(pos);
                touch(
                    gui,
                    macroquad::miniquad::TouchPhase::Started,
                    pos,
                    input_scale,
                );
                added = true;
            }
        } else if let Some(pos) = down.take() {
            touch(
                gui,
                macroquad::miniquad::TouchPhase::Ended,
                pos,
                input_scale,
            );
        }
        set_default_camera();
        clear_background(BLACK);
        gui.run(|ctx, canceled| {
            input_scale = ctx.pixels_per_point();
            viewport = ctx.screen_rect();
            app.sound_ui(ctx, canceled);
            plot = ctx.data_mut(|d| d.get_temp::<egui::Rect>(egui::Id::new("speed-curve-plot")));
            window_rect = crate::window::visible_rect(ctx, egui::Id::new("Note speed curve"));
        });
        preview.render(
            app.editor.document().tempo().seconds_at(app.transport.beat),
            preview.target_viewport(),
        );
        gui.draw();
        unsafe { macroquad::miniquad::gl::glFinish() };
        if app.editor.revision() > 0
            && app.speed_curve.profile.is_some()
            && !app.speed_curve.busy()
            && app.speed_curve.worker.is_none()
            && frame > 15
        {
            break;
        }
        next_frame().await;
    }
    ensure!(
        added && app.editor.document().as_json() != &original,
        "A real canvas click did not commit the speed curve"
    );
    ensure!(
        app.editor.document().as_json()["judgeLineList"][0]["eventLayers"][0]["speedEvents"]
            != original["judgeLineList"][0]["eventLayers"][0]["speedEvents"],
        "A real off-curve click did not change native speed events"
    );
    if app.editor.revision() != revision {
        preview
            .replace_chart(app.editor.document(), None, &app.project_options)
            .await?;
    }
    let edited = app.editor.document().as_json().clone();
    app.speed_curve.mode = Mode::Edit;
    if let Some(index) = app
        .speed_curve
        .points
        .iter()
        .position(|p| p.beat == Beat::new(2, 0, 1).unwrap())
    {
        app.speed_curve.selected = Some(index);
    }
    for language in [Language::English, Language::Russian] {
        app.options.language = language;
        for frame in 0..5 {
            set_default_camera();
            clear_background(BLACK);
            app.speed_curve.record_native_clock(
                app.session,
                app.editor.revision(),
                preview.speed_curve_horizon(),
            );
            gui.run(|ctx, canceled| {
                app.sound_ui(ctx, canceled);
                viewport = ctx.screen_rect();
                plot =
                    ctx.data_mut(|d| d.get_temp::<egui::Rect>(egui::Id::new("speed-curve-plot")));
                window_rect = crate::window::visible_rect(ctx, egui::Id::new("Note speed curve"));
            });
            preview.render(0., preview.target_viewport());
            gui.draw();
            unsafe { macroquad::miniquad::gl::glFinish() };
            if frame < 4 {
                next_frame().await;
            }
        }
        let rect =
            window_rect.ok_or_else(|| anyhow::anyhow!("Speed curve window was not rendered"))?;
        ensure!(
            rect.right() <= viewport.right() + 1. && rect.bottom() <= viewport.bottom() + 1.,
            "Speed curve window exceeds the viewport: {rect:?}"
        );
        get_screen_data().export_png(
            out.join(format!("speed-curve-{}.png", language.code()))
                .to_str()
                .unwrap(),
        );
        next_frame().await;
    }
    app.options.language = Language::English;
    app.transport.beat = 2.;
    app.transport.playing = true;
    app.speed_curve.mode = Mode::Edit;
    let rect = plot.ok_or_else(|| anyhow::anyhow!("Missing plot for touch navigation"))?;
    let x = |beat: f64| {
        rect.left() + ((beat - app.speed_curve.start) / app.speed_curve.span) as f32 * rect.width()
    };
    let from = egui::pos2(x(2.), rect.top() + 5.);
    let to = egui::pos2(x(3.5), rect.top() + 5.);
    touch(
        gui,
        macroquad::miniquad::TouchPhase::Started,
        from,
        input_scale,
    );
    pump(gui, app, preview, &mut modal_input).await;
    ensure!(
        app.is_scrubbing(),
        "The real platform playhead did not capture touch"
    );
    touch(gui, macroquad::miniquad::TouchPhase::Moved, to, input_scale);
    pump(gui, app, preview, &mut modal_input).await;
    touch(gui, macroquad::miniquad::TouchPhase::Ended, to, input_scale);
    pump(gui, app, preview, &mut modal_input).await;
    ensure!(
        (app.transport.beat - 3.5).abs() < 1e-4 && app.transport.playing && !app.is_scrubbing(),
        "Playhead lost the final touch position or playback intent"
    );
    let initial = app.speed_curve.limit;
    let center = rect.center();
    for (phase, id, delta) in [
        (macroquad::miniquad::TouchPhase::Started, 1, -30.),
        (macroquad::miniquad::TouchPhase::Started, 2, 30.),
        (macroquad::miniquad::TouchPhase::Moved, 1, -60.),
        (macroquad::miniquad::TouchPhase::Moved, 2, 60.),
        (macroquad::miniquad::TouchPhase::Ended, 1, -60.),
        (macroquad::miniquad::TouchPhase::Ended, 2, 60.),
    ] {
        touch_id(gui, phase, id, center + egui::vec2(delta, 0.), input_scale);
        pump(gui, app, preview, &mut modal_input).await;
    }
    ensure!(
        (app.speed_curve.limit - initial * 0.5).abs() < 0.01,
        "Actual Gui touch pinch failed"
    );
    ensure!(
        app.editor.document().as_json() == &edited,
        "Navigation authored an unintended chart edit"
    );
    app.transport.playing = false;
    app.history_step_now(false);
    ensure!(
        app.editor.document().as_json() == &original,
        "One Undo did not restore the original chart"
    );
    app.history_step_now(true);
    ensure!(
        app.editor.document().as_json() == &edited,
        "Redo did not restore the curve"
    );
    ensure!(
        app.editor.document().note(0, 0)?.kind == NoteKind::Tap,
        "Curve editing changed the note type"
    );
    std::fs::write(
        out.join("result.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"native_clock":true,"canvas_add":true,"routed_touch_add":true,"runtime_modal_input":true,"platform_touch":true,"input_scale":input_scale,"pinch":true,"playhead_during_playback":true,"exact_undo_redo":true,"viewport":[screen_width(),screen_height()],"pass":true}),
        )?,
    )?;
    Ok(())
}

async fn pump(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut phistudio_prpr::Preview,
    modal: &mut crate::runtime::ModalInput,
) {
    if modal.update(app) {
        gui.cancel_input();
    }
    app.speed_curve.record_native_clock(
        app.session,
        app.editor.revision(),
        preview.speed_curve_horizon(),
    );
    set_default_camera();
    clear_background(BLACK);
    gui.run(|ctx, canceled| app.sound_ui(ctx, canceled));
    preview.render(
        app.editor.document().tempo().seconds_at(app.transport.beat),
        preview.target_viewport(),
    );
    gui.draw();
    unsafe { macroquad::miniquad::gl::glFinish() };
    next_frame().await;
}

// Enter through Gui's real Miniquad handler with physical framebuffer coordinates.
fn touch(gui: &mut Gui, phase: macroquad::miniquad::TouchPhase, pos: egui::Pos2, scale: f32) {
    touch_id(gui, phase, 1, pos, scale);
}
fn touch_id(
    gui: &mut Gui,
    phase: macroquad::miniquad::TouchPhase,
    id: u64,
    pos: egui::Pos2,
    scale: f32,
) {
    macroquad::miniquad::EventHandler::touch_event(
        gui,
        unsafe { get_internal_gl() }.quad_context,
        phase,
        id,
        pos.x * scale,
        pos.y * scale,
        get_time(),
    );
}
