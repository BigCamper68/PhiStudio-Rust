//! Exercise persistent creation settings through the production touch route.
use super::*;
use phistudio_core::{NoteCreationDefaults, NoteKind};

#[derive(Clone, Copy)]
enum Aim {
    Header,
    Field,
    Reset,
    Grid,
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
            Aim::Grid => app.multi_line.base.map_or(Rect::NOTHING, |g| g.rect),
            Aim::Header => d
                .get_temp(Id::new("new-note-defaults-header"))
                .unwrap_or(Rect::NOTHING),
            Aim::Reset => d
                .get_temp(Id::new("reset-new-note-defaults"))
                .unwrap_or(Rect::NOTHING),
            Aim::Field => d
                .get_temp::<Vec<Rect>>(Id::new("new-note-default-fields"))
                .and_then(|v| v.first().copied())
                .unwrap_or(Rect::NOTHING),
        });
        let zones = ctx
            .data_mut(|d| d.get_temp::<Vec<crate::scroll::Zone>>(Id::new("phistudio-scroll-zones")))
            .unwrap_or_default();
        if let Some(zone) = zones
            .into_iter()
            .find(|z| z.layer != egui::LayerId::background() && z.axis == 1)
            && !matches!(aim, Aim::Grid)
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
    let image = super::read_framebuffer();
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
    let root = tempfile::tempdir()?;
    let chart = root.path().join("chart.json");
    let document = ChartDocument::empty("Note defaults touch", 120.)?;
    phistudio_core::save_chart(&document, &chart)?;
    *app = Workbench::new(document, Some(chart));
    app.skin = Some(preview.editor_skin());
    preview
        .replace_chart(
            app.editor.document(),
            Some(root.path()),
            &app.project_options,
        )
        .await?;
    let mut files = crate::files::FileManager::new(None);
    files.update(app);
    app.select(None);
    app.tools.show = false;
    app.windows.settings = true;
    let defaults = NoteCreationDefaults {
        size: 1.5,
        speed: 1.2,
        above: 0,
        is_fake: 1,
        alpha: 180,
        y_offset: 32.,
        visible_time: 8.,
        judge_area: Some(1.75),
        tint_hit_effects: Some([12, 34, 56]),
        volume: 0.6,
        ..Default::default()
    };
    app.options.new_note_defaults = defaults.clone();
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
    let before = app.options.new_note_defaults.size;
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
        app.options.new_note_defaults.size != before,
        "Actual numeric touch did not change note defaults"
    );
    for language in Language::ALL {
        app.options.language = language;
        for _ in 0..6 {
            current = frame(gui, app, preview, &mut modal, Aim::Reset).await?;
        }
        if language == Language::Russian {
            current
                .image
                .export_png(out.join("note-defaults-ru.png").to_str().unwrap());
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
        app.options.new_note_defaults == NoteCreationDefaults::default(),
        "Actual touch did not reset note defaults"
    );
    ensure!(
        app.editor.revision() == revision && app.editor.document().as_json() == &original,
        "Preference controls modified chart/history"
    );
    app.options.new_note_defaults = defaults.clone();
    app.windows.settings = false;
    app.windows.sidebar = false;
    app.set_tool(crate::workbench::Tool::Place(NoteKind::Tap));
    for _ in 0..6 {
        current = frame(gui, app, preview, &mut modal, Aim::Grid).await?;
    }
    let tap_position = current.target.center();
    tap(
        gui,
        app,
        preview,
        &mut modal,
        tap_position,
        current.scale,
        Aim::Grid,
    )
    .await?;
    ensure!(
        app.editor.document().note_count() == 0 && app.files.request.is_some(),
        "Touch did not stage native sound preparation"
    );
    let deadline = get_time() + 20.;
    while app.editor.document().note_count() == 0 {
        files.update(app);
        current = frame(gui, app, preview, &mut modal, Aim::Grid).await?;
        ensure!(
            app.error.is_none(),
            "Creation worker failed: {:?}",
            app.error
        );
        ensure!(get_time() < deadline, "Creation worker timed out");
    }
    let after_tap = app.editor.document().as_json().clone();
    let note = app.editor.document().note(0, 0)?;
    ensure!(
        note.size == defaults.size
            && note.speed == defaults.speed
            && note.alpha == defaults.alpha
            && note.above == defaults.above
            && note.is_fake == defaults.is_fake
            && note.visible_time == defaults.visible_time
            && note.y_offset == defaults.y_offset
            && note.judge_area == defaults.judge_area
            && note.tint_hit_effects == defaults.tint_hit_effects,
        "Touch placement lost note defaults"
    );
    let gain = app
        .editor
        .document()
        .note_sound_gain(NoteId { line: 0, index: 0 })
        .unwrap();
    ensure!(
        gain.volume == defaults.volume && root.path().join(&gain.rendered).is_file(),
        "Native gain was not prepared"
    );
    app.select(None);
    app.set_tool(crate::workbench::Tool::Place(NoteKind::Hold));
    let start = current.target.center() + vec2(current.target.width() * 0.25, 0.);
    let end = start - vec2(0., 48.);
    touch(
        gui,
        macroquad::miniquad::TouchPhase::Started,
        start,
        current.scale,
    );
    frame(gui, app, preview, &mut modal, Aim::Grid).await?;
    touch(
        gui,
        macroquad::miniquad::TouchPhase::Moved,
        end,
        current.scale,
    );
    frame(gui, app, preview, &mut modal, Aim::Grid).await?;
    touch(
        gui,
        macroquad::miniquad::TouchPhase::Ended,
        end,
        current.scale,
    );
    frame(gui, app, preview, &mut modal, Aim::Grid).await?;
    ensure!(
        app.editor.document().note_count() == 2,
        "Cached hold placement was not immediate: {:?}",
        app.error
    );
    let hold = app.editor.document().note(0, 1)?;
    ensure!(
        hold.kind == NoteKind::Hold
            && hold.end_time > hold.start_time
            && hold.size == defaults.size
            && hold.above == 0,
        "Hold gesture lost duration or defaults"
    );
    let after_hold = app.editor.document().as_json().clone();
    preview
        .replace_chart(
            app.editor.document(),
            Some(root.path()),
            &app.project_options,
        )
        .await?;
    ensure!(
        preview.note_count() == 2,
        "Created defaults did not reach the original prpr parser"
    );
    app.history_step_now(false);
    ensure!(
        app.editor.document().as_json() == &after_tap,
        "Hold was not one Undo"
    );
    app.history_step_now(false);
    ensure!(
        app.editor.document().as_json() == &original,
        "Tap was not one Undo"
    );
    app.history_step_now(true);
    app.history_step_now(true);
    ensure!(
        app.editor.document().as_json() == &after_hold,
        "Redo changed note data"
    );
    std::fs::write(
        out.join("result.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "pass": true, "real_touch_numeric": true, "real_touch_reset": true, "all_locales": true,
            "preferences_do_not_edit": true, "real_touch_tap_hold": true, "native_gain_worker": true,
            "original_prpr_notes": preview.note_count(), "exact_undo_redo": true,
            "viewport": [screen_width(), screen_height()]
        }))?,
    )?;
    Ok(())
}
