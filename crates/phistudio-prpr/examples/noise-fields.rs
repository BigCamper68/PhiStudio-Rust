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
    check_editor_fields(&mut preview, &base, &options, &area, &hidden, &out).await?;
    preview.replace_chart(&document, None, &options).await?;
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

async fn check_editor_fields(
    preview: &mut Preview,
    base: &ChartDocument,
    options: &ProjectOptions,
    area: &serde_json::Value,
    hidden: &Image,
    out: &std::path::Path,
) -> Result<()> {
    let vp = preview.target_viewport();
    let count = |image: &Image, axis: usize| {
        image
            .bytes
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| (0..3).all(|i| i == axis || p[axis] as u16 > p[i] as u16 + 20))
            .count()
    };
    let mut raw = base.as_json().clone();
    for (name, rgb, axis) in [("green", [0, 255, 0], 1), ("blue", [20, 40, 255], 2)] {
        let mut field = area.clone();
        field["phiStudioColor"] = json!(rgb);
        raw["blockAreaList"] = json!([field]);
        let doc = ChartDocument::from_value(raw.clone())?;
        preview.replace_chart(&doc, None, options).await?;
        for (phase, t) in [("disabled", 1.25), ("ready", 1.75), ("active", 3.)] {
            preview.render(t, vp);
            let frame = preview.snapshot();
            ensure!(
                count(&frame, axis) > 1000,
                "Colored {name} {phase} lost its palette"
            );
            frame.export_png(out.join(format!("{name}-{phase}.png")).to_str().unwrap());
            preview.render(0., vp);
            preview.render(t, vp);
            ensure!(
                frame.bytes == preview.snapshot().bytes,
                "Colored field changed after seek"
            );
        }
        let mut subtract = raw["blockAreaList"][0].clone();
        subtract["isSubtract"] = json!(true);
        raw["blockAreaList"].as_array_mut().unwrap().push(subtract);
        preview
            .replace_chart(&ChartDocument::from_value(raw.clone())?, None, options)
            .await?;
        preview.render(3., vp);
        ensure!(
            preview.snapshot().bytes == hidden.bytes,
            "Colored subtract masks left stale pixels"
        );
    }
    let mut field = area.clone();
    field["phiStudioColorEvents"] = json!([{"startTime":[2,0,1],"endTime":[6,0,1],"start":[0,255,0],"end":[20,40,255],"easingType":1}]);
    raw["blockAreaList"] = json!([field]);
    preview
        .replace_chart(&ChartDocument::from_value(raw.clone())?, None, options)
        .await?;
    preview.render(2., vp);
    let first = preview.snapshot();
    preview.render(3., vp);
    ensure!(
        first.bytes != preview.snapshot().bytes,
        "Color animation did not reach GPU"
    );
    // Exercise the emitted JSON through the real parser, without the editor's
    // authoring backup restoration. This also validates strict RGB byte types.
    let colored_doc = ChartDocument::from_value(raw.clone())?;
    let (mut exported, resources) =
        phistudio_core::noise_fields::export::prepare(&colored_doc, &[])?;
    exported
        .as_object_mut()
        .unwrap()
        .shift_remove(phistudio_core::noise_fields::export::SOURCE);
    let root = tempfile::tempdir()?;
    for (name, bytes) in resources {
        let path = root.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)?;
    }
    let exported = ChartDocument::from_value(exported)?;
    preview
        .replace_chart(&exported, Some(root.path()), options)
        .await?;
    preview.render(3., vp);
    ensure!(
        preview.note_count() == 0,
        "Generated field guides introduced notes"
    );
    let next = json!({"Name":"Next GPU","bottomLeft":{"x":-405.,"y":-270.},"topRight":{"x":405.,"y":270.},"appearTime":[2,0,1],"disappearTime":[10,0,1],"activeIntervals":[{"startTime":[4,0,1],"endTime":[8,0,1]}],"isInvert":false});
    raw["blockAreaList"] = json!([next.clone()]);
    preview
        .replace_chart(&ChartDocument::from_value(raw.clone())?, None, options)
        .await?;
    preview.render(3., vp);
    let next_active = preview.snapshot();
    ensure!(count(&next_active, 0) > 1000, "Next field is invisible");
    next_active.export_png(out.join("next-active.png").to_str().unwrap());
    preview.render(1.75, vp);
    preview
        .snapshot()
        .export_png(out.join("next-ready.png").to_str().unwrap());
    preview.render(3., vp);
    ensure!(
        next_active.bytes == preview.snapshot().bytes,
        "Next seek was stateful"
    );
    let mut sub = next.clone();
    sub["isInvert"] = json!(true);
    raw["blockAreaList"] = json!([sub.clone(), sub.clone(), sub.clone()]);
    preview
        .replace_chart(&ChartDocument::from_value(raw.clone())?, None, options)
        .await?;
    preview.render(3., vp);
    ensure!(
        count(&preview.snapshot(), 0) > 1000,
        "Three Next inversions lost XOR coverage"
    );
    raw["noiseAreaOptions"] = json!({"ignoreTripleInversion":true});
    preview
        .replace_chart(&ChartDocument::from_value(raw.clone())?, None, options)
        .await?;
    preview.render(3., vp);
    ensure!(
        preview.snapshot().bytes == hidden.bytes,
        "Single inversion option did not cancel three overlaps"
    );
    raw["noiseAreaOptions"] = json!({"ignoreTripleInversion":false});
    raw["blockAreaList"] = json!([next, sub.clone(), sub.clone(), sub]);
    preview
        .replace_chart(&ChartDocument::from_value(raw)?, None, options)
        .await?;
    preview.render(3., vp);
    ensure!(
        preview.snapshot().bytes == hidden.bytes,
        "Next normal union XOR subtract coverage did not cancel"
    );
    Ok(())
}
