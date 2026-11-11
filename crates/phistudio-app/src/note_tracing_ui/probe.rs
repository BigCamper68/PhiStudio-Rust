//! Native geometry and actual Miniquad touch checks for Motion Generator tracing.
use super::*;
use crate::{gui::Gui, runtime::ModalInput};
use anyhow::{Result, ensure};
use macroquad::prelude::*;
use phistudio_core::{ChartDocument, Language, Note, NoteKind};
use serde_json::{Value, json};
use std::path::Path;

fn event(a: f64, b: f64) -> Value {
    json!({"startTime":[0,0,1],"endTime":[4,0,1],"start":a,"end":b,"easingType":1})
}
fn fixture() -> Result<ChartDocument> {
    let mut raw = ChartDocument::empty("Tracing native fixture", 120.)?
        .as_json()
        .clone();
    let base = raw["judgeLineList"][0].clone();
    raw["judgeLineList"]
        .as_array_mut()
        .unwrap()
        .extend([base.clone(), base]);
    raw["BPMList"] = json!([{"startTime":[0,0,1],"bpm":120.},{"startTime":[2,0,1],"bpm":180.}]);
    let source = &mut raw["judgeLineList"][0];
    source["father"] = json!(1);
    source["rotateWithFather"] = json!(true);
    source["notes"] = json!([Note {
        start_time: "4".parse()?,
        end_time: "4".parse()?,
        x: 130.,
        y_offset: 80.,
        above: 0,
        speed: 1.3,
        ..Default::default()
    }]);
    source["notes"][0]["unknown"] = json!({"keep":true});
    source["eventLayers"][0]["moveXEvents"] = json!([event(-120., 200.)]);
    source["eventLayers"][0]["moveYEvents"] = json!([event(30., 100.)]);
    source["eventLayers"][0]["rotateEvents"] = json!([event(0., 360.)]);
    source["eventLayers"][0]["speedEvents"] = json!([event(10., 20.)]);
    source["posControl"] = json!([{"x":0.,"pos":1.,"easing":1},{"x":250.,"pos":1.,"easing":1},{"x":250.,"pos":1.5,"easing":1},{"x":10000.,"pos":1.5,"easing":1}]);
    source["extended"]["inclineEvents"] = json!([event(0., 20.)]);
    let parent = &mut raw["judgeLineList"][1];
    parent["eventLayers"][0]["moveXEvents"] = json!([event(50., 100.)]);
    parent["eventLayers"][0]["moveYEvents"] = json!([event(-50., 30.)]);
    parent["eventLayers"][0]["rotateEvents"] = json!([event(0., 90.)]);
    let destination = &mut raw["judgeLineList"][2];
    destination["father"] = json!(1);
    destination["rotateWithFather"] = json!(true);
    destination["eventLayers"][0]["moveXEvents"] = json!([event(25., -35.)]);
    destination["eventLayers"][0]["moveYEvents"] = json!([event(-40., 20.)]);
    destination["eventLayers"][0]["rotateEvents"] = json!([event(10., -20.)]);
    destination["eventLayers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"moveXEvents":[event(15.,15.)],"rotateEvents":[event(0.,0.)]}));
    Ok(ChartDocument::from_value(raw)?)
}
struct Frame {
    mode: Option<egui::Rect>,
    preview: Option<egui::Rect>,
    apply: Option<egui::Rect>,
    scale: f32,
    image: Option<Image>,
}
async fn pump(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    modal: &mut ModalInput,
    bottom: bool,
) -> Result<Frame> {
    if modal.update(app) {
        gui.cancel_input();
    }
    gui.set_ui_scale(app.options.ui_scale);
    app.update_note_tracing(preview, true);
    preview.set_editor_options(&app.options, app.line)?;
    if app.preview.rect.is_positive() {
        let size = app.preview.rect.size() * app.pixels_per_point;
        preview.fit_display([size.x, size.y], app.preview_aspect());
    }
    app.preview.texture = Some(egui::TextureId::User(preview.texture_id()));
    app.preview.size = preview.render_size();
    let mut frame = Frame {
        mode: None,
        preview: None,
        apply: None,
        scale: 1.,
        image: None,
    };
    set_default_camera();
    clear_background(BLACK);
    gui.run(|ctx, canceled| {
        app.sound_ui(ctx, canceled);
        frame.scale = ctx.pixels_per_point();
        ctx.data_mut(|d| {
            frame.mode = d.get_temp(egui::Id::new("note-tracing-mode"));
            frame.preview = d.get_temp(egui::Id::new("note-tracing-preview"));
            frame.apply = d.get_temp(egui::Id::new("note-tracing-apply"));
        });
        {
            let zones = ctx
                .data_mut(|d| {
                    d.get_temp::<Vec<crate::scroll::Zone>>(egui::Id::new("phistudio-scroll-zones"))
                })
                .unwrap_or_default();
            for zone in zones
                .into_iter()
                .filter(|z| z.layer != egui::LayerId::background() && z.axis == 1)
            {
                if let Some(mut state) = egui::scroll_area::State::load(ctx, zone.id) {
                    state.offset.y = if bottom { zone.max_offset } else { 0. };
                    state.store(ctx, zone.id);
                }
            }
        }
    });
    preview.render(0.35, preview.target_viewport());
    gui.draw();
    unsafe { macroquad::miniquad::gl::glFinish() };
    if app.note_tracing.prepared.is_some() || app.note_tracing.mode == MotionMode::Trajectory {
        frame.image = Some(get_screen_data());
    }
    next_frame().await;
    Ok(frame)
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
    modal: &mut ModalInput,
    rect: egui::Rect,
    scale: f32,
    bottom: bool,
) -> Result<()> {
    touch(
        gui,
        macroquad::miniquad::TouchPhase::Started,
        rect.center(),
        scale,
    );
    pump(gui, app, preview, modal, bottom).await?;
    touch(
        gui,
        macroquad::miniquad::TouchPhase::Ended,
        rect.center(),
        scale,
    );
    pump(gui, app, preview, modal, bottom).await?;
    Ok(())
}
fn native_samples(
    preview: &mut Preview,
    source: NoteId,
    times: &[f64],
) -> Result<Vec<NoteTraceSample>> {
    let mut result = vec![];
    for chunk in times.chunks(16) {
        result.extend(preview.sample_note_trace(source, None, chunk)?);
    }
    Ok(result)
}
pub(crate) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &Path,
) -> Result<()> {
    let mut modal = ModalInput::default();
    let mut geometry = vec![];
    for (case, destination, layer) in [
        ("new-line", None, 0),
        ("parent-and-layers", Some(2), 1),
        ("hold-new-line", None, 3),
    ] {
        let mut document = fixture()?.as_json().clone();
        if case == "hold-new-line" {
            document["judgeLineList"][0]["notes"][0]["type"] = json!(2);
            document["judgeLineList"][0]["notes"][0]["startTime"] = json!([2, 0, 1]);
            document["judgeLineList"][0]["notes"][0]["size"] = json!(1.5);
            document["judgeLineList"][0]["notes"][0]["above"] = json!(1);
        }
        app.replace_document(ChartDocument::from_value(document)?, None, None);
        app.project_options.note_uniform_scale = case == "hold-new-line";
        app.options.new_line_defaults = if case == "hold-new-line" {
            phistudio_core::LineCreationDefaults {
                x: 123.,
                y: -64.,
                rotation: 35.,
                alpha: 172.,
                speed: 12.,
            }
        } else {
            Default::default()
        };
        app.select(Some(NoteId { line: 0, index: 0 }));
        app.tools.show = true;
        app.tools.page = crate::toolkit::Page::Motion;
        app.note_tracing = State::default();
        app.note_tracing.rate = 240;
        app.note_tracing.destination = destination;
        app.note_tracing.layer = layer;
        app.note_tracing.replace = destination.is_some();
        preview
            .replace_chart(app.editor.document(), None, &app.project_options)
            .await?;
        let original = app.editor.document().as_json().clone();
        let mut frame = pump(gui, app, preview, &mut modal, false).await?;
        for _ in 0..5 {
            frame = pump(gui, app, preview, &mut modal, false).await?;
        }
        let mode = frame
            .mode
            .ok_or_else(|| anyhow::anyhow!("Missing Note Tracing mode"))?;
        frame.image.as_ref().unwrap().export_png(
            out.join(format!("{case}-before-mode.png"))
                .to_str()
                .unwrap(),
        );
        tap(gui, app, preview, &mut modal, mode, frame.scale, false).await?;
        ensure!(
            app.note_tracing.mode == MotionMode::NoteTracing,
            "Actual touch did not select Note Tracing"
        );
        for _ in 0..5 {
            frame = pump(gui, app, preview, &mut modal, true).await?;
        }
        let button = frame
            .preview
            .ok_or_else(|| anyhow::anyhow!("Missing tracing preview button"))?;
        tap(gui, app, preview, &mut modal, button, frame.scale, true).await?;
        ensure!(
            app.note_tracing.pending.is_some(),
            "Actual touch did not start tracing: {:?}",
            app.note_tracing.error
        );
        let deadline = get_time() + 30.;
        while app.note_tracing.pending.is_some() && get_time() < deadline {
            frame = pump(gui, app, preview, &mut modal, true).await?;
        }
        ensure!(app.note_tracing.pending.is_none(), "Tracing timed out");
        ensure!(
            app.editor.document().as_json() == &original,
            "Preview mutated source JSON"
        );
        let prepared = &app
            .note_tracing
            .prepared
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No preview: {:?}", app.note_tracing.error))?
            .1;
        ensure!(
            prepared.edit.is_ok(),
            "Preparation failed: {:?}",
            prepared.edit.as_ref().err()
        );
        let count = prepared.edit.as_ref().unwrap().event_count();
        let end = app.editor.document().tempo().seconds_at(4.);
        let times: Vec<_> = (0..=500).map(|i| end * f64::from(i) / 500.).collect();
        let reference = native_samples(preview, NoteId { line: 0, index: 0 }, &times)?;
        for language in Language::ALL {
            app.options.language = language;
            for _ in 0..3 {
                frame = pump(gui, app, preview, &mut modal, true).await?;
            }
            if language == Language::Russian {
                frame
                    .image
                    .as_ref()
                    .unwrap()
                    .export_png(out.join(format!("{case}-ru-preview.png")).to_str().unwrap());
            }
        }
        frame
            .image
            .as_ref()
            .unwrap()
            .export_png(out.join(format!("{case}-preview.png")).to_str().unwrap());
        let apply = frame
            .apply
            .ok_or_else(|| anyhow::anyhow!("Missing tracing apply button"))?;
        tap(gui, app, preview, &mut modal, apply, frame.scale, true).await?;
        ensure!(
            app.editor.document().as_json() != &original,
            "Actual touch did not apply tracing"
        );
        ensure!(
            app.editor.document().as_json()["judgeLineList"][0] == original["judgeLineList"][0],
            "Tracing changed the source line"
        );
        let applied = app.editor.document().as_json().clone();
        app.editor.undo();
        ensure!(
            app.editor.document().as_json() == &original,
            "Undo was not exact"
        );
        app.editor.redo();
        ensure!(
            app.editor.document().as_json() == &applied,
            "Redo was not exact"
        );
        let target = destination.unwrap_or(3);
        let mut sampled = applied;
        sampled["judgeLineList"][target]["notes"] = json!([Note {
            kind: NoteKind::Hold,
            start_time: Beat::ZERO,
            end_time: "100".parse()?,
            is_fake: 1,
            ..Default::default()
        }]);
        let sampled = ChartDocument::from_value(sampled)?;
        preview
            .replace_chart(&sampled, None, &app.project_options)
            .await?;
        let actual = native_samples(
            preview,
            NoteId {
                line: target,
                index: 0,
            },
            &times,
        )?;
        let max_position = reference
            .iter()
            .zip(&actual)
            .map(|(a, b)| (a.position[0] - b.position[0]).hypot(a.position[1] - b.position[1]))
            .fold(0., f64::max);
        let max_rotation = reference
            .iter()
            .zip(&actual)
            .map(|(a, b)| (a.rotation - b.rotation).abs())
            .fold(0., f64::max);
        ensure!(max_position < 0.45, "{case} position error {max_position}");
        ensure!(max_rotation < 0.035, "{case} rotation error {max_rotation}");
        geometry.push(json!({"case":case,"events":count,"max_position_error":max_position,"max_rotation_error":max_rotation}));
        preview
            .replace_chart(app.editor.document(), None, &app.project_options)
            .await?;
    }
    std::fs::write(
        out.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"pass":true,"geometry":geometry,"routed_touch":true,"runtime_modal_input":true,"exact_undo_redo":true,"viewport":[screen_width(),screen_height()]}),
        )?,
    )?;
    Ok(())
}
