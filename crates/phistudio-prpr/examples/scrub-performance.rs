//! Synchronized dense-chart seek timings and exact frame checksums.
use anyhow::{Result, ensure};
use macroquad::prelude::*;
use phistudio_core::{Beat, ChartDocument, EditorOptions, Note, NoteKind, ProjectOptions};
use phistudio_prpr::Preview;
use std::time::Instant;
fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "Dense scrub performance".into(),
            window_width: 1280,
            window_height: 720,
            ..Default::default()
        },
        async {
            if let Err(e) = run().await {
                eprintln!("{e:#}");
                std::process::exit(1);
            }
            std::process::exit(0);
        },
    );
}
async fn run() -> Result<()> {
    let out = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "artifacts/checks/scrub-performance".into()),
    );
    std::fs::create_dir_all(&out)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();
    let mut root = ChartDocument::empty("Dense scrub", 120.)?.as_json().clone();
    let base = root["judgeLineList"][0].clone();
    let mut lines = vec![];
    for line_id in 0..24 {
        let mut line = base.clone();
        let mut notes = vec![];
        for index in 0..480 {
            let time = index as f64 * 0.04 + (line_id % 5) as f64 * 0.002;
            let beat = Beat::from_grid(time * 2., 1_000_000)?;
            notes.push(serde_json::to_value(Note {
                start_time: beat,
                end_time: beat,
                x: ((line_id % 5) as f64 - 2.) * 100.,
                ..Default::default()
            })?);
        }
        notes.push(serde_json::to_value(Note {
            kind: NoteKind::Hold,
            start_time: Beat::ZERO,
            end_time: Beat::new(40, 0, 1)?,
            x: (line_id % 3) as f64 * 200. - 200.,
            ..Default::default()
        })?);
        line["notes"] = notes.into();
        line["numOfNotes"] = serde_json::json!(481);
        lines.push(line);
    }
    root["judgeLineList"] = lines.into();
    let document = ChartDocument::from_value(root)?;
    let original = document.as_json().clone();
    let mut preview = Preview::new(&document, None, &ProjectOptions::default()).await?;
    if std::env::args().any(|a| a == "retina") {
        preview.fit_display([2388., 1343.25], 16. / 9.);
    }
    let mut report = vec![];
    let mut plain = None;
    for enabled in [false, true] {
        preview.set_editor_options(
            &EditorOptions {
                preview_particles: enabled,
                preview_hud: false,
                mark_active_line_id: false,
                highlight_current_line: false,
                ..Default::default()
            },
            0,
        )?;
        let mut timings = vec![];
        for (index, time) in [
            8.08, 8.113, 8.146, 8.18, 8.213, 8.246, 8.28, 8.313, 8.346, 8.38, 8.413, 8.446,
        ]
        .into_iter()
        .enumerate()
        {
            unsafe {
                get_internal_gl().flush();
                macroquad::miniquad::gl::glFinish();
            }
            let start = Instant::now();
            preview.set_playback(time, false, true)?;
            preview.render(time, preview.target_viewport());
            unsafe {
                get_internal_gl().flush();
                macroquad::miniquad::gl::glFinish();
            }
            let elapsed = start.elapsed().as_secs_f64() * 1000.;
            timings.push(elapsed);
            if index == 0 {
                let image = preview.snapshot();
                if enabled {
                    ensure!(
                        plain.as_ref() == Some(&image.bytes),
                        "Paused scrub reconstructed effects"
                    );
                } else {
                    plain = Some(image.bytes.clone());
                }
                image.export_png(
                    out.join(if enabled { "effects.png" } else { "plain.png" })
                        .to_str()
                        .unwrap(),
                );
            }
            next_frame().await;
        }
        timings.sort_by(f64::total_cmp);
        let median = timings[timings.len() / 2];
        println!(
            "effects={enabled} · {} notes · {:?} · median {median:.3} ms",
            document.note_count(),
            preview.render_size()
        );
        report.push(serde_json::json!({"effects":enabled,"notes":document.note_count(),"render_size":preview.render_size(),"median_ms":median,"max_ms":timings.last(),"samples_ms":timings}));
    }
    ensure!(
        document.as_json() == &original,
        "Benchmark changed chart data"
    );
    std::fs::write(
        out.join("timings.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}
