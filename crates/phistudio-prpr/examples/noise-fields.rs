//! GPU regression for Preview Noise Fields. No user project is modified.
use anyhow::{Result, ensure};
use macroquad::prelude::*;
use phistudio_core::{ChartDocument, EditorOptions, ProjectOptions};
use phistudio_prpr::Preview;
use serde_json::json;
fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "Noise Fields preview regression".into(),
            window_width: 1280,
            window_height: 720,
            ..Default::default()
        },
        async {
            if let Err(e) = pollster::block_on(run()) {
                eprintln!("{e:#}");
                std::process::exit(1);
            }
            std::process::exit(0);
        },
    );
}
async fn run() -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();
    let out = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "artifacts/checks/noise-fields".into()),
    );
    std::fs::create_dir_all(&out)?;
    let base = ChartDocument::empty("Noise Fields", 120.)?;
    let mut raw = base.as_json().clone();
    let area = json!({"bottomLeftPercentage":{"x":0.2,"y":0.2},"topRightPercentage":{"x":0.8,"y":0.8},"appearTime":1.,"enableTime":2.,"disableTime":4.,"disappearTime":5.,"isSubtract":false,"moveEvents":[],"rotateEvents":[],"scaleEvents":[]});
    raw["blockAreaList"] = json!([area.clone()]);
    let document = ChartDocument::from_value(raw.clone())?;
    let options = ProjectOptions::default();
    let mut preview = Preview::new(&document, None, &options).await?;
    let mut editor_options = EditorOptions {
        preview_hud: false,
        preview_particles: false,
        mark_active_line_id: false,
        highlight_current_line: false,
        ..Default::default()
    };
    preview.set_editor_options(&editor_options, 0)?;
    let vp = preview.target_viewport();
    preview.render(0.5, vp);
    let hidden = preview.snapshot();
    for (label, t) in [
        ("disabled", 1.25),
        ("ready", 1.75),
        ("active", 3.),
        ("disabled-after", 4.5),
        ("hidden-after", 5.5),
    ] {
        preview.render(t, vp);
        let frame = preview.snapshot();
        frame.export_png(out.join(format!("{label}.png")).to_str().unwrap());
        if t < 5. {
            ensure!(frame.bytes != hidden.bytes, "{label} is invisible");
            let red_pixels = frame
                .bytes
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[0] as u16 > p[1] as u16 + 20 && p[0] as u16 > p[2] as u16 + 20)
                .count();
            ensure!(
                red_pixels > 1_000,
                "{label} lost its red base: {red_pixels} red pixels"
            );
        } else {
            ensure!(
                frame.bytes == hidden.bytes,
                "Hidden field left stale pixels"
            );
        }
    }
    preview.render(3., vp);
    let active = preview.snapshot();
    preview.render(1.25, vp);
    preview.render(3., vp);
    ensure!(
        active.bytes == preview.snapshot().bytes,
        "Scrubbing changed active field"
    );
    editor_options.preview_shaders = false;
    preview.set_editor_options(&editor_options, 0)?;
    preview.render(3., vp);
    ensure!(
        active.bytes == preview.snapshot().bytes,
        "Shader authoring switch disabled a gameplay mechanic"
    );
    let mut subtract = area.clone();
    subtract["isSubtract"] = json!(true);
    raw["blockAreaList"] = json!([area.clone(), subtract.clone()]);
    let subtract_document = ChartDocument::from_value(raw.clone())?;
    preview
        .replace_chart(&subtract_document, None, &options)
        .await?;
    preview.render(3., vp);
    ensure!(
        preview.snapshot().bytes == hidden.bytes,
        "Matching subtract area did not cancel normal area"
    );
    raw["blockAreaList"] = json!([area.clone(), subtract.clone(), subtract]);
    preview
        .replace_chart(&ChartDocument::from_value(raw.clone())?, None, &options)
        .await?;
    preview.render(3., vp);
    ensure!(
        active.bytes == preview.snapshot().bytes,
        "Subtract parity did not restore the normal field"
    );
    let mut animated = area.clone();
    animated["moveEvents"] = json!([
        {"time":1.,"endPosition":{"x":0.5,"y":0.5},"easeTypeX":1,"easeTypeY":2},
        {"time":3.,"endPosition":{"x":0.65,"y":0.65}}
    ]);
    animated["scaleEvents"] = json!([
        {"time":1.,"anchor":{"x":0.5,"y":0.5},"scale":{"x":1.,"y":1.}},
        {"time":3.,"anchor":{"x":0.5,"y":0.5},"scale":{"x":0.5,"y":0.4}}
    ]);
    animated["rotateEvents"] = json!([
        {"time":1.,"anchor":{"x":0.5,"y":0.5},"rotation":0.},
        {"time":3.,"anchor":{"x":0.5,"y":0.5},"rotation":40.}
    ]);
    raw["blockAreaList"] = json!([animated]);
    preview
        .replace_chart(&ChartDocument::from_value(raw.clone())?, None, &options)
        .await?;
    preview.render(3., vp);
    let transformed = preview.snapshot();
    ensure!(
        transformed.bytes != active.bytes,
        "Field event geometry did not reach the GPU"
    );
    transformed.export_png(out.join("animated.png").to_str().unwrap());
    preview.render(1.75, vp);
    preview.render(3., vp);
    ensure!(
        transformed.bytes == preview.snapshot().bytes,
        "Animated field changed after scrubbing"
    );
    preview.replace_chart(&document, None, &options).await?;
    preview.render(3., vp);
    ensure!(
        active.bytes == preview.snapshot().bytes,
        "Replacing chart changed same frame"
    );
    for (name, aspect) in [("square", 1.), ("portrait", 0.75), ("wide", 2.4)] {
        let mut options = options.clone();
        options.aspect_ratio = Some(aspect);
        preview.replace_chart(&document, None, &options).await?;
        preview.render(3., vp);
        preview
            .snapshot()
            .export_png(out.join(format!("{name}.png")).to_str().unwrap());
    }
    preview.fit_display([1600., 900.], 16. / 9.);
    preview.render(3., preview.target_viewport());
    ensure!(preview.snapshot().bytes.iter().any(|v| *v > 20));
    preview.replace_chart(&base, None, &options).await?;
    preview.render(3., preview.target_viewport());
    ensure!(preview.note_count() == 0);
    ensure!(
        document.as_json() == &raw_with_area(base.as_json(), area),
        "Preview mutated chart source"
    );
    std::fs::write(
        out.join("result.txt"),
        "PASS: five phases, exact repeated seek, subtract cancellation, replacement/removal, shader toggle, aspect and resize\n",
    )?;
    if let Some(chart_path) = std::env::args().nth(2) {
        let path = std::path::Path::new(&chart_path);
        let document = ChartDocument::from_value(serde_json::from_slice(&std::fs::read(path)?)?)?;
        let before = document.as_json().clone();
        let options = ProjectOptions::default();
        preview
            .replace_chart(&document, path.parent(), &options)
            .await?;
        preview.fit_display([1280., 720.], 16. / 9.);
        for beat in [322., 331.5, 332., 333., 333.49, 333.5, 334.] {
            let time = beat * 60. / 202.;
            preview.render(time, preview.target_viewport());
            preview.snapshot().export_png(
                out.join(format!("desultory-beat-{beat}.png"))
                    .to_str()
                    .unwrap(),
            );
        }
        ensure!(document.as_json() == &before, "User chart changed");
    }
    println!("PASS: Noise Fields Preview GPU regression");
    Ok(())
}
fn raw_with_area(base: &serde_json::Value, area: serde_json::Value) -> serde_json::Value {
    let mut raw = base.clone();
    raw["blockAreaList"] = json!([area]);
    raw
}
