//! Paired shader timelines rendered with the real editor and shared GL context.
use super::*;
use phistudio_core::{Command, shaders::*};
use serde_json::json;
use std::path::Path;

pub(super) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &Path,
) -> Result<()> {
    let baseline = default_shader_extra(app.editor.document());
    for index in 0..5 {
        let mut effect = json!({"start":[0,0,1],"end":[6,0,1],"shader":"vignette","vars":{
            "radius":[{"startTime":[0,0,1],"endTime":[2,0,1],"start":1.,"end":15.,"easingType":2},
                      {"startTime":[2,0,1],"endTime":[4,0,1],"start":15.,"end":7.,"easingType":5}],
            "color":[{"startTime":[0,0,1],"endTime":[4,0,1],"start":[0,0,0,255],"end":[20,40,80,200],"easingType":1}]
        },"custom":"retained"});
        // Extra uniforms exercise horizontal navigation without changing the preset shader.
        for n in 0..6 {
            effect["vars"][format!("custom{n}")] =
                json!([{"startTime":[0,0,1],"endTime":[4,0,1],"start":n,"end":n+1,"easingType":2}]);
        }
        app.editor.execute(Command::EditShader {
            baseline: baseline.clone(),
            action: ShaderAction::Insert { index, effect },
        })?;
    }
    app.advanced.right_mode = RightMode::Shaders;
    app.shaders.selected = Some(0);
    app.refresh_shaders();
    app.set_tool(crate::workbench::Tool::Select);
    app.windows.sidebar = true;
    let original = app.editor.document().as_json().clone();
    preview
        .replace_chart(app.editor.document(), None, &app.project_options)
        .await?;
    let mut report = vec![];
    for language in [
        Language::English,
        Language::Russian,
        Language::SimplifiedChinese,
    ] {
        app.options.language = language;
        for scale in [1., 1.25, 1.5] {
            app.options.ui_scale = scale;
            for width in [220., 330.] {
                for _ in 0..5 {
                    paint(gui, app, preview, width)?;
                    next_frame().await;
                }
                paint(gui, app, preview, width)?;
                let grid = crate::grid_input::geometry(app.timeline.area, app);
                ensure!(
                    grid.rect.right() < grid.event_rect.left(),
                    "Shader grids overlap"
                );
                ensure!(
                    grid.rect.width() > 40. && grid.event_rect.width() > 40.,
                    "Shader grid collapsed"
                );
                let name = format!("{}-{scale}-{width}", language.code());
                get_screen_data().export_png(out.join(format!("{name}.png")).to_str().unwrap());
                report.push(name);
            }
        }
    }
    app.options.ui_scale = 1.;
    app.options.language = Language::English;
    for _ in 0..5 {
        paint(gui, app, preview, 220.)?;
        next_frame().await;
    }
    let grid = crate::grid_input::geometry(app.timeline.area, app);
    let left = pos2(grid.rect.left() + 26., grid.y(1.));
    gui.probe_input(vec![button(left, true), button(left, false)], false);
    paint(gui, app, preview, 220.)?;
    next_frame().await;
    let right = pos2(grid.event_rect.left() + 38., grid.y(1.));
    gui.probe_input(vec![button(right, true), button(right, false)], false);
    for _ in 0..5 {
        paint(gui, app, preview, 220.)?;
        next_frame().await;
    }
    paint(gui, app, preview, 220.)?;
    get_screen_data().export_png(out.join("animation-inspector.png").to_str().unwrap());
    gui.probe_input(
        vec![
            egui::Event::PointerMoved(right),
            egui::Event::Scroll(vec2(-1000., 0.)),
        ],
        false,
    );
    for _ in 0..5 {
        paint(gui, app, preview, 220.)?;
        next_frame().await;
    }
    paint(gui, app, preview, 220.)?;
    get_screen_data().export_png(out.join("uniforms-scrolled.png").to_str().unwrap());
    ensure!(
        app.editor.document().as_json() == &original,
        "Shader navigation changed chart data"
    );
    std::fs::write(out.join("cases.json"), serde_json::to_vec_pretty(&report)?)?;
    println!(
        "PASS: {} paired shader layouts, uniform selection and horizontal scroll; chart unchanged",
        report.len()
    );
    Ok(())
}
fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}
