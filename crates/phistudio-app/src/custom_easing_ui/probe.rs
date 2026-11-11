//! Actual Miniquad/Gui input, modal preflight and framebuffer evidence.
use super::*;
use crate::{gui::Gui, runtime::ModalInput};
use anyhow::{Result, ensure};
use egui::{Id, Pos2, Rect};
use macroquad::{
    miniquad::{EventHandler, TouchPhase},
    prelude::*,
};
use phistudio_core::{ChartDocument, EventType, Language};
use phistudio_prpr::Preview;
use std::path::Path;

struct Frame {
    scale: f32,
    screen: Rect,
    window: Rect,
    plot: Rect,
    use_curve: Rect,
    add: Rect,
    apply: Rect,
    image: Image,
}
async fn frame(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    modal: &mut ModalInput,
    reveal_apply: bool,
) -> Result<Frame> {
    if modal.update(app) {
        gui.cancel_input();
    }
    gui.set_ui_scale(app.options.ui_scale);
    set_default_camera();
    clear_background(BLACK);
    let mut info = (
        1.,
        Rect::NOTHING,
        Rect::NOTHING,
        Rect::NOTHING,
        Rect::NOTHING,
        Rect::NOTHING,
    );
    let mut add = Rect::NOTHING;
    gui.run(|ctx, canceled| {
        app.sound_ui(ctx, canceled);
        info.0 = ctx.pixels_per_point();
        info.1 = ctx.screen_rect();
        info.2 = crate::window::visible_rect(ctx, Id::new("Custom easing editor"))
            .unwrap_or(Rect::NOTHING);
        ctx.data_mut(|d| {
            info.3 = d
                .get_temp(Id::new("custom-easing-plot"))
                .unwrap_or(Rect::NOTHING);
            info.4 = d
                .get_temp(Id::new("custom-easing-use"))
                .unwrap_or(Rect::NOTHING);
            info.5 = d.get_temp(Id::new("event-apply")).unwrap_or(Rect::NOTHING);
            add = d
                .get_temp(Id::new(("custom-easing-tool", "Add point")))
                .unwrap_or(Rect::NOTHING);
        });
        if reveal_apply && info.5.is_positive() {
            let zones = ctx
                .data_mut(|d| {
                    d.get_temp::<Vec<crate::scroll::Zone>>(Id::new("phistudio-scroll-zones"))
                })
                .unwrap_or_default();
            if let Some(zone) = zones
                .into_iter()
                .find(|z| z.layer == egui::LayerId::background() && z.axis == 1)
                && let Some(mut state) = egui::scroll_area::State::load(ctx, zone.id)
            {
                state.offset.y = (state.offset.y + info.5.center().y - zone.rect.center().y)
                    .clamp(0., zone.max_offset);
                state.store(ctx, zone.id);
            }
        }
    });
    preview.render(0., preview.target_viewport());
    gui.draw();
    unsafe { macroquad::miniquad::gl::glFinish() };
    let image = crate::ui_layout_probe::read_framebuffer();
    next_frame().await;
    Ok(Frame {
        scale: info.0,
        screen: info.1,
        window: info.2,
        plot: info.3,
        use_curve: info.4,
        add,
        apply: info.5,
        image,
    })
}
fn touch(gui: &mut Gui, phase: TouchPhase, id: u64, pos: Pos2, scale: f32) {
    EventHandler::touch_event(
        gui,
        unsafe { get_internal_gl() }.quad_context,
        phase,
        id,
        pos.x * scale,
        pos.y * scale,
        get_time(),
    );
}
async fn tap(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    modal: &mut ModalInput,
    pos: Pos2,
    scale: f32,
) -> Result<Frame> {
    touch(gui, TouchPhase::Started, 1, pos, scale);
    frame(gui, app, preview, modal, false).await?;
    touch(gui, TouchPhase::Ended, 1, pos, scale);
    frame(gui, app, preview, modal, false).await
}
pub(crate) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &Path,
) -> Result<()> {
    let mut raw = app.editor.document().as_json().clone();
    raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"] = serde_json::json!([{"startTime":[0,0,1],"endTime":[8,0,1],"start":10.,"end":100.,"easingType":1,"unknownVendorData":{"keep":true}}]);
    let fixture = ChartDocument::from_value(raw)?;
    app.replace_document(fixture, None, None);
    preview
        .replace_chart(app.editor.document(), None, &app.project_options)
        .await?;
    let id = EventId {
        line: 0,
        layer: 0,
        kind: EventType::MoveX,
        index: 0,
    };
    app.select_event(Some(id));
    let event = app.editor.document().event(id)?;
    let key = DraftKey {
        session: app.session,
        revision: app.editor.revision(),
        id,
        times: [event.start_time.to_string(), event.end_time.to_string()],
        event,
    };
    let original = app.editor.document().as_json().clone();
    app.open_custom_easing(key);
    let mut modal = ModalInput::default();
    let mut f = frame(gui, app, preview, &mut modal, false).await?;
    for _ in 0..4 {
        f = frame(gui, app, preview, &mut modal, false).await?;
    }
    f.image
        .export_png(&out.join("initial.png").to_string_lossy());
    ensure!(
        f.screen.expand(2.).contains_rect(f.window),
        "Curve workspace leaves viewport: {:?}, screen {:?}",
        f.window,
        f.screen
    );
    ensure!(
        f.screen.contains_rect(f.use_curve),
        "Use curve is outside the viewport"
    );
    ensure!(
        f.plot.width() > 80. && f.plot.height() > 60.,
        "Curve graph is too small"
    );
    let scale = f.scale;
    f = tap(gui, app, preview, &mut modal, f.add.center(), scale).await?;
    let view = graph::View {
        rect: f.plot,
        bounds: app.custom_easing.view,
    };
    f = tap(
        gui,
        app,
        preview,
        &mut modal,
        view.point([0.4, 0.75]),
        scale,
    )
    .await?;
    ensure!(
        app.custom_easing.draft.curve.nodes.len() == 3,
        "A real touch did not add a point"
    );
    let added = app.custom_easing.draft.curve.clone();
    ensure!(
        app.editor.document().as_json() == &original,
        "Adding a draft point changed the chart"
    );
    app.custom_easing.tool = Tool::Edit;
    let view = graph::View {
        rect: f.plot,
        bounds: app.custom_easing.view,
    };
    let p = view.point(added.nodes[1].point());
    let q = view.point([0.55, 0.8]);
    touch(gui, TouchPhase::Started, 1, p, scale);
    frame(gui, app, preview, &mut modal, false).await?;
    touch(gui, TouchPhase::Moved, 1, q, scale);
    frame(gui, app, preview, &mut modal, false).await?;
    touch(gui, TouchPhase::Ended, 1, q, scale);
    f = frame(gui, app, preview, &mut modal, false).await?;
    let mut moved = app.custom_easing.draft.curve.clone();
    ensure!(
        (moved.nodes[1].x - 0.55).abs() < 0.005 && (moved.nodes[1].y - 0.8).abs() < 0.005,
        "A real touch did not move the point"
    );
    ensure!(
        app.custom_easing.undo.len() == 2,
        "A touch drag split local history"
    );
    let view = graph::View {
        rect: f.plot,
        bounds: app.custom_easing.view,
    };
    let handle = moved.materialized()?.nodes[1]
        .incoming
        .ok_or_else(|| anyhow::anyhow!("Missing incoming handle"))?;
    let h = view.point(handle);
    let target = view.point([handle[0] + 0.025, handle[1] + 0.12]);
    touch(gui, TouchPhase::Started, 1, h, scale);
    frame(gui, app, preview, &mut modal, false).await?;
    touch(gui, TouchPhase::Moved, 1, target, scale);
    frame(gui, app, preview, &mut modal, false).await?;
    touch(gui, TouchPhase::Ended, 1, target, scale);
    f = frame(gui, app, preview, &mut modal, false).await?;
    ensure!(
        app.custom_easing.draft.curve.nodes[1].incoming != moved.nodes[1].incoming,
        "A real touch did not move a curve handle"
    );
    ensure!(
        app.custom_easing.draft.curve.nodes[1].point() == moved.nodes[1].point(),
        "Moving a handle moved its node"
    );
    moved = app.custom_easing.draft.curve.clone();
    ensure!(
        app.custom_easing.undo.len() == 3,
        "A handle drag split local history"
    );
    let view = graph::View {
        rect: f.plot,
        bounds: app.custom_easing.view,
    };
    let p = view.point(moved.nodes[1].point());
    touch(gui, TouchPhase::Started, 1, p, scale);
    frame(gui, app, preview, &mut modal, false).await?;
    touch(gui, TouchPhase::Moved, 1, p + egui::vec2(12., -12.), scale);
    frame(gui, app, preview, &mut modal, false).await?;
    touch(gui, TouchPhase::Cancelled, 1, p, scale);
    f = frame(gui, app, preview, &mut modal, false).await?;
    ensure!(
        app.custom_easing.draft.curve == moved && app.custom_easing.undo.len() == 3,
        "A canceled native touch committed a curve edit"
    );
    let view = graph::View {
        rect: f.plot,
        bounds: app.custom_easing.view,
    };
    let p = view.point(moved.nodes[1].point());
    let other = view.point([0.9, 0.7]);
    touch(gui, TouchPhase::Started, 1, p, scale);
    frame(gui, app, preview, &mut modal, false).await?;
    touch(gui, TouchPhase::Moved, 1, p + egui::vec2(8., -8.), scale);
    frame(gui, app, preview, &mut modal, false).await?;
    touch(gui, TouchPhase::Started, 2, other, scale);
    frame(gui, app, preview, &mut modal, false).await?;
    ensure!(
        app.custom_easing.draft.curve == moved,
        "Pinch promotion committed the interrupted point drag"
    );
    let bounds = app.custom_easing.view;
    touch(
        gui,
        TouchPhase::Moved,
        2,
        other + egui::vec2(24., -10.),
        scale,
    );
    frame(gui, app, preview, &mut modal, false).await?;
    ensure!(
        app.custom_easing.view != bounds,
        "A real pinch did not change the view"
    );
    touch(gui, TouchPhase::Ended, 2, other, scale);
    frame(gui, app, preview, &mut modal, false).await?;
    touch(gui, TouchPhase::Ended, 1, p, scale);
    frame(gui, app, preview, &mut modal, false).await?;
    ensure!(
        app.custom_easing.undo.len() == 3 && app.custom_easing.draft.curve == moved,
        "Pinch changed curve history"
    );
    for language in Language::ALL {
        app.options.language = language;
        frame(gui, app, preview, &mut modal, false).await?;
        f = frame(gui, app, preview, &mut modal, false).await?;
        let code = language.code();
        f.image
            .export_png(&out.join(format!("locale-{code}.png")).to_string_lossy());
        ensure!(
            f.screen.expand(2.).contains_rect(f.window) && f.screen.contains_rect(f.use_curve),
            "Workspace overflow in {code}"
        );
    }
    app.options.language = Language::English;
    f = frame(gui, app, preview, &mut modal, false).await?;
    f = tap(gui, app, preview, &mut modal, f.use_curve.center(), scale).await?;
    for _ in 0..180 {
        if !app.custom_easing.open {
            break;
        }
        f = frame(gui, app, preview, &mut modal, false).await?;
    }
    ensure!(
        !app.custom_easing.open && app.custom_easing.error.is_none(),
        "Prepared curve did not return to the inspector: {:?}",
        app.custom_easing.error
    );
    ensure!(
        app.editor.document().as_json() == &original,
        "Use curve bypassed the inspector draft"
    );
    for _ in 0..5 {
        f = frame(gui, app, preview, &mut modal, true).await?;
    }
    ensure!(
        f.screen.contains_rect(f.apply),
        "Apply event is unreachable"
    );
    tap(gui, app, preview, &mut modal, f.apply.center(), scale).await?;
    let edited = app.editor.document().as_json().clone();
    ensure!(
        edited != original,
        "Apply event did not commit the prepared curve"
    );
    let event = app.editor.document().event(id)?;
    ensure!(
        event
            .custom_easing
            .as_ref()
            .is_some_and(|b| b.curve == moved),
        "The inspector lost the edited curve"
    );
    preview
        .replace_chart(app.editor.document(), None, &app.project_options)
        .await?;
    ensure!(
        app.editor.undo() && app.editor.document().as_json() == &original,
        "Workspace apply Undo was not exact"
    );
    ensure!(
        app.editor.redo() && app.editor.document().as_json() == &edited,
        "Workspace apply Redo was not exact"
    );
    std::fs::write(
        out.join("result.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"pass":true,"real_gui_touch":true,"logical_event":true,"draft_ownership":true,"local_history":true,"handle_drag":true,"touch_cancellation":true,"pinch_cancellation":true,"native_load":true,"exact_undo_redo":true,"locales":7,"physical_android":false,"scale":scale}),
        )?,
    )?;
    Ok(())
}
