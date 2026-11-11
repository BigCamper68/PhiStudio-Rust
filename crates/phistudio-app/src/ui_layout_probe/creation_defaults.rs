//! Exercise persistent creation settings through the production touch route.
use super::*;
use phistudio_core::{Command, EventType, LineCreationDefaults};

#[derive(Clone, Copy)]
enum Aim {
    Header,
    Field,
    Reset,
}
struct Frame {
    target: Rect,
    scale: f32,
    image: Image,
}
async fn frame(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    modal: &mut crate::runtime::ModalInput,
    aim: Aim,
) -> Result<Frame> {
    if modal.update(app) {
        gui.cancel_input();
    }
    gui.set_ui_scale(app.options.ui_scale);
    let mut target = Rect::NOTHING;
    let mut scale = 1.;
    set_default_camera();
    clear_background(BLACK);
    gui.run(|ctx, canceled| {
        app.sound_ui(ctx, canceled);
        scale = ctx.pixels_per_point();
        target = ctx.data_mut(|d| match aim {
            Aim::Header => d
                .get_temp(Id::new("new-line-defaults-header"))
                .unwrap_or(Rect::NOTHING),
            Aim::Reset => d
                .get_temp(Id::new("reset-new-line-defaults"))
                .unwrap_or(Rect::NOTHING),
            Aim::Field => d
                .get_temp::<Vec<Rect>>(Id::new("new-line-default-fields"))
                .and_then(|v| v.first().copied())
                .unwrap_or(Rect::NOTHING),
        });
        let zones = ctx
            .data_mut(|d| d.get_temp::<Vec<crate::scroll::Zone>>(Id::new("phistudio-scroll-zones")))
            .unwrap_or_default();
        if let Some(zone) = zones
            .into_iter()
            .find(|z| z.layer != egui::LayerId::background() && z.axis == 1)
            && target.is_positive()
            && let Some(mut state) = egui::scroll_area::State::load(ctx, zone.id)
        {
            state.offset.y = (state.offset.y + target.center().y - zone.rect.center().y)
                .clamp(0., zone.max_offset);
            state.store(ctx, zone.id);
        }
    });
    preview.render(0., preview.target_viewport());
    gui.draw();
    unsafe { macroquad::miniquad::gl::glFinish() };
    let image = get_screen_data();
    next_frame().await;
    Ok(Frame {
        target,
        scale,
        image,
    })
}
fn touch(gui: &mut Gui, phase: macroquad::miniquad::TouchPhase, position: egui::Pos2, scale: f32) {
    macroquad::miniquad::EventHandler::touch_event(
        gui,
        unsafe { get_internal_gl() }.quad_context,
        phase,
        1,
        position.x * scale,
        position.y * scale,
        get_time(),
    );
}
async fn tap(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    modal: &mut crate::runtime::ModalInput,
    position: egui::Pos2,
    scale: f32,
    aim: Aim,
) -> Result<()> {
    touch(
        gui,
        macroquad::miniquad::TouchPhase::Started,
        position,
        scale,
    );
    frame(gui, app, preview, modal, aim).await?;
    touch(gui, macroquad::miniquad::TouchPhase::Ended, position, scale);
    frame(gui, app, preview, modal, aim).await?;
    Ok(())
}
pub(super) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &std::path::Path,
) -> Result<()> {
    app.select(None);
    app.tools.show = false;
    app.windows.settings = true;
    let defaults = LineCreationDefaults {
        x: 125.,
        y: -75.,
        rotation: 34.,
        alpha: 128.,
        speed: 12.,
    };
    app.options.new_line_defaults = defaults;
    let original = app.editor.document().as_json().clone();
    let revision = app.editor.revision();
    let mut modal = crate::runtime::ModalInput::default();
    let mut current = frame(gui, app, preview, &mut modal, Aim::Header).await?;
    for _ in 0..8 {
        current = frame(gui, app, preview, &mut modal, Aim::Header).await?;
    }
    tap(
        gui,
        app,
        preview,
        &mut modal,
        current.target.center(),
        current.scale,
        Aim::Header,
    )
    .await?;
    for _ in 0..10 {
        current = frame(gui, app, preview, &mut modal, Aim::Field).await?;
    }
    let before = app.options.new_line_defaults.x;
    let position = current.target.center();
    let scale = current.scale;
    touch(
        gui,
        macroquad::miniquad::TouchPhase::Started,
        position,
        scale,
    );
    frame(gui, app, preview, &mut modal, Aim::Field).await?;
    for delta in [12., 24., 36.] {
        touch(
            gui,
            macroquad::miniquad::TouchPhase::Moved,
            position + vec2(delta, 0.),
            scale,
        );
        frame(gui, app, preview, &mut modal, Aim::Field).await?;
    }
    touch(
        gui,
        macroquad::miniquad::TouchPhase::Ended,
        position + vec2(36., 0.),
        scale,
    );
    frame(gui, app, preview, &mut modal, Aim::Field).await?;
    ensure!(
        app.options.new_line_defaults.x != before,
        "Actual numeric touch did not change line defaults"
    );
    for language in Language::ALL {
        app.options.language = language;
        for _ in 0..6 {
            current = frame(gui, app, preview, &mut modal, Aim::Reset).await?;
        }
        if language == Language::Russian {
            current
                .image
                .export_png(out.join("line-defaults-ru.png").to_str().unwrap());
        }
    }
    tap(
        gui,
        app,
        preview,
        &mut modal,
        current.target.center(),
        current.scale,
        Aim::Reset,
    )
    .await?;
    ensure!(
        app.options.new_line_defaults == LineCreationDefaults::default(),
        "Actual touch did not reset line defaults"
    );
    ensure!(
        app.editor.revision() == revision && app.editor.document().as_json() == &original,
        "Preference controls modified chart/history"
    );
    app.options.new_line_defaults = defaults;
    app.windows.settings = false;
    app.execute(Command::AddLines { count: 3 });
    ensure!(
        app.error.is_none()
            && app.editor.document().line_count()
                == original["judgeLineList"].as_array().unwrap().len() + 3,
        "Batch did not use creation settings"
    );
    let first = app.editor.document().line_count() - 3;
    for line in first..first + 3 {
        ensure!(
            app.editor
                .document()
                .event_values(line, 0, EventType::MoveX)?[0]["start"]
                == 125.
        );
        ensure!(
            app.editor
                .document()
                .event_values(line, 0, EventType::Alpha)?[0]["start"]
                == 128.
        );
    }
    app.history_step_now(false);
    ensure!(
        app.editor.document().as_json() == &original,
        "One Undo did not restore the original chart"
    );
    std::fs::write(
        out.join("result.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"pass":true,"real_touch_numeric":true,"real_touch_reset":true,"all_locales":true,"preferences_do_not_edit":true,"batch_defaults":true,"one_undo":true,"viewport":[screen_width(),screen_height()]}),
        )?,
    )?;
    Ok(())
}
