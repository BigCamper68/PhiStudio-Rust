//! Actual note pixels are sampled from native preview, then picked by the native ID pass.
use anyhow::{Result, ensure};
use macroquad::prelude::*;
use phistudio_core::{
    Beat, ChartDocument, Command, Editor, Note, NoteId, NoteKind, ProjectOptions,
};
use phistudio_prpr::Preview;
fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "Native preview picking".into(),
            window_width: 1280,
            window_height: 720,
            sample_count: 1,
            ..Default::default()
        },
        async {
            if let Err(e) = pollster::block_on(run()) {
                eprintln!("{e:#}");
                std::process::exit(1);
            }
        },
    );
}
async fn run() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let _guard = rt.enter();
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    std::fs::create_dir_all(&out)?;
    let options = ProjectOptions {
        force_aspect_ratio: true,
        ..Default::default()
    };
    let empty = ChartDocument::empty("Pick", 120.)?;
    let mut preview = Preview::new(&empty, None, &options).await?;
    if let Some(size) = std::env::args().nth(2) {
        let (w, h) = size
            .split_once('x')
            .ok_or_else(|| anyhow::anyhow!("Expected WIDTHxHEIGHT"))?;
        preview.fit_display([w.parse()?, h.parse()?], 16. / 9.);
    }
    let mut last_shader_point = None;
    let mut reports = Vec::new();
    for (case, (kind, above, rotation, aspect)) in [
        (NoteKind::Tap, 1, 0., 16. / 9.),
        (NoteKind::Hold, 1, 37., 16. / 9.),
        (NoteKind::Tap, 2, -30., 16. / 9.),
        (NoteKind::Flick, 1, -90., 16. / 9.),
        (NoteKind::Drag, 2, 60., 16. / 9.),
        (NoteKind::Tap, 1, 12., 4. / 3.),
        (NoteKind::Hold, 1, 15., 16. / 9.),
    ]
    .into_iter()
    .enumerate()
    {
        let mut e = Editor::new(empty.clone());
        for (kind, beat, x) in [(NoteKind::Tap, 100, 300.), (kind, 2, -200.)] {
            e.execute(Command::AddNote {
                line: 0,
                note: Note {
                    kind,
                    start_time: Beat::new(beat, 0, 1)?,
                    end_time: Beat::new(beat + 1, 0, 1)?,
                    x,
                    above,
                    ..Default::default()
                },
            })?;
        }
        let mut raw = e.document().as_json().clone();
        raw["judgeLineList"][0]["eventLayers"][0]["rotateEvents"][0]["start"] =
            serde_json::json!(rotation);
        raw["judgeLineList"][0]["eventLayers"][0]["rotateEvents"][0]["end"] =
            serde_json::json!(rotation);
        if case == 6 {
            let mut parent = empty.as_json()["judgeLineList"][0].clone();
            parent["eventLayers"][0]["rotateEvents"][0]["start"] = serde_json::json!(30.);
            parent["eventLayers"][0]["rotateEvents"][0]["end"] = serde_json::json!(30.);
            raw["judgeLineList"].as_array_mut().unwrap().push(parent);
            let line = &mut raw["judgeLineList"][0];
            line["father"] = serde_json::json!(1);
            line["parent"] = serde_json::json!(1);
            line["extended"]["inclineEvents"] = serde_json::json!([{"startTime":[0,0,1],"endTime":[10,0,1],"start":20.,"end":20.,"easingType":1}]);
            for (track, field, value) in [
                ("posControl", "pos", 1.2),
                ("sizeControl", "size", 1.3),
                ("yControl", "y", 0.9),
            ] {
                line[track] = serde_json::json!([{"x":0.,field:value,"easing":1},{"x":500.,field:value,"easing":1},{"x":10000.,field:value,"easing":1}]);
            }
        }
        let doc = ChartDocument::from_value(raw.clone())?;
        let options = ProjectOptions {
            aspect_ratio: Some(aspect),
            ..options.clone()
        };
        preview.replace_chart(&doc, None, &options).await?;
        preview.render(0.9, preview.target_viewport());
        let image = preview.snapshot();
        let width = image.width as usize;
        let height = image.height as usize;
        image.export_png(out.join(format!("native-{case}.png")).to_str().unwrap());
        let mut pixels = Vec::new();
        for (i, p) in image.bytes.as_chunks::<4>().0.iter().enumerate() {
            let colored = match kind {
                NoteKind::Tap | NoteKind::Hold => {
                    p[2] > 150 && p[1] > 100 && i32::from(p[2]) - i32::from(p[0]) > 40
                }
                NoteKind::Flick => p[0] > 150 && p[1] < 140 && p[2] < 150,
                NoteKind::Drag => p[0] > 160 && p[1] > 130 && p[2] < 130,
            };
            if colored {
                pixels.push((i % width, i / width));
            }
        }
        ensure!(!pixels.is_empty(), "No visible target note in case {case}");
        let (x, y) = pixels[pixels.len() / 2];
        let w = (height as f32 * aspect).min(width as f32);
        let h = (width as f32 / aspect).min(height as f32);
        let left = ((width as f32 - w) / 2.).round();
        let bottom = ((height as f32 - h) / 2.).round();
        let point = [(x as f32 - left) / w, 1. - (y as f32 - bottom) / h];
        let id = preview.pick_note(point, [0.002, 0.002])?;
        ensure!(
            id == Some(NoteId { line: 0, index: 1 }),
            "Case {case}, point {point:?} picked {id:?}"
        );
        preview.render(0.9, preview.target_viewport());
        ensure!(
            preview.snapshot().bytes == image.bytes,
            "Picking changed visible rendering in case {case}"
        );
        ensure!(doc.as_json() == &raw, "Picking changed source document");
        if case == 0 {
            let mut faint = raw.clone();
            faint["judgeLineList"][0]["notes"][1]["alpha"] = serde_json::json!(8);
            let faint = ChartDocument::from_value(faint)?;
            preview.replace_chart(&faint, None, &options).await?;
            preview.render(0.9, preview.target_viewport());
            ensure!(
                preview.pick_note(point, [0.002, 0.002])? == Some(NoteId { line: 0, index: 1 }),
                "A fading but visible note could not be picked"
            );
        }
        reports.push(serde_json::json!({"case":case,"note":format!("{kind:?}"),"above":above,"rotation":rotation,"aspect":aspect,"point":point,"picked_index":1}));
    }
    let mut editor = Editor::new(empty.clone());
    for (i, x) in [300., -400., -300., -200., -100., 0., 100., 200., 300.]
        .into_iter()
        .enumerate()
    {
        let t = if i == 0 { 100 } else { 2 };
        editor.execute(Command::AddNote {
            line: 0,
            note: Note {
                start_time: Beat::new(t, 0, 1)?,
                end_time: Beat::new(t, 0, 1)?,
                x,
                ..Default::default()
            },
        })?;
    }
    let mut shader_raw = editor.document().as_json().clone();
    let assets = out.join("shader-assets");
    std::fs::create_dir_all(&assets)?;
    std::fs::write(
        assets.join("shift.glsl"),
        "#version 100\nprecision mediump float;varying vec2 uv;uniform sampler2D screenTexture;uniform vec2 offset; // %0.18,0.0%\nvoid main(){gl_FragColor=vec4(texture2D(screenTexture,uv+offset).bgr,1.0);}",
    )?;
    for (case, (shader, global)) in [
        ("grayscale", false),
        ("pixel", false),
        ("fisheye", true),
        ("/shift.glsl", true),
    ]
    .into_iter()
    .enumerate()
    {
        shader_raw["phiStudioExtra"] = serde_json::json!({"bpm":120,"effects":[{"start":[0,0,1],"end":[10,0,1],"shader":shader,"global":global}]});
        let full = ChartDocument::from_value(shader_raw.clone())?;
        let mut without = Editor::new(full.clone());
        without.execute(Command::DeleteNote(NoteId { line: 0, index: 1 }))?;
        preview
            .replace_chart(without.document(), Some(&assets), &options)
            .await?;
        preview.render(0.9, preview.target_viewport());
        let background = preview.snapshot();
        preview
            .replace_chart(&full, Some(&assets), &options)
            .await?;
        preview.render(0.9, preview.target_viewport());
        let image = preview.snapshot();
        let width = image.width as usize;
        let height = image.height as usize;
        ensure!(
            preview.shader_report()["enabled"] == true,
            "Shader did not load: {}",
            preview.shader_report()
        );
        let mut best = (0usize, 0usize);
        for (i, (a, b)) in image
            .bytes
            .as_chunks::<4>()
            .0
            .iter()
            .zip(background.bytes.as_chunks::<4>().0.iter())
            .enumerate()
        {
            let delta = (0..3).map(|j| a[j].abs_diff(b[j]) as usize).sum::<usize>();
            if delta > best.0 {
                best = (delta, i);
            }
        }
        ensure!(best.0 > 20, "No postprocessed target pixels");
        let x = best.1 % width;
        let y = best.1 / width;
        let point = [x as f32 / width as f32, 1. - y as f32 / height as f32];
        last_shader_point = Some(point);
        let started = std::time::Instant::now();
        let picked = preview.pick_note(point, [0.006, 0.006])?;
        ensure!(
            picked == Some(NoteId { line: 0, index: 1 }),
            "Shader {shader} picked {picked:?} at {point:?}"
        );
        ensure!(
            preview.snapshot().bytes == image.bytes,
            "Shader picking changed the displayed frame"
        );
        image.export_png(out.join(format!("shader-{case}.png")).to_str().unwrap());
        reports.push(serde_json::json!({"shader":shader,"global":global,"point":point,"milliseconds":started.elapsed().as_millis(),"raw_index":1}));
    }
    let point = last_shader_point.unwrap();
    for display in [[2732., 1536.75], [300., 168.75], [2388., 1343.25]] {
        preview.fit_display(display, 16. / 9.);
        let image = preview.snapshot();
        ensure!(
            [image.width as u32, image.height as u32] == preview.render_size(),
            "Stale resize output"
        );
        ensure!(
            preview.pick_note(point, [0.006, 0.006])? == Some(NoteId { line: 0, index: 1 }),
            "Resizing broke shader-aware picking"
        );
        ensure!(
            preview.snapshot().bytes == image.bytes,
            "Picking changed a resized frame"
        );
        reports.push(serde_json::json!({"display":display,"render_size":preview.render_size(),"resize_picking":true}));
    }
    std::fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&reports)?,
    )?;
    std::fs::write(
        out.join("result.txt"),
        "PASS\nOriginal prpr rendered pixels pick the correct source note for Tap/Hold/Flick/Drag, above/below, rotations, aspect ratios, parent transforms, incline, distance controls and native shader postprocessing; subsequent rendering and source data are unchanged.\n",
    )?;
    Ok(())
}
