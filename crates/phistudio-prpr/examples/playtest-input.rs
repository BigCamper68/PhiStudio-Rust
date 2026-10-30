//! Real input drives original prpr Judge; the fixture never edits user projects.
use anyhow::{Result, ensure};
use macroquad::prelude::*;
use phistudio_core::{
    Beat, ChartDocument, Command, Editor, EditorOptions, Note, NoteKind, ProjectOptions,
};
use phistudio_prpr::Preview;

fn check_line_color(preview: &Preview, counts: [u32; 4]) -> Result<(serde_json::Value, Image)> {
    let skin = preview.skin_report();
    let expected = if counts[2] + counts[3] > 0 {
        serde_json::json!([1., 1., 1., 1.])
    } else if counts[1] > 0 {
        skin["good"].clone()
    } else {
        skin["perfect"].clone()
    };
    ensure!(
        skin["line_color"] == expected,
        "Wrong AP/FC color after {counts:?}: {skin}"
    );
    let image = preview.snapshot();
    let rgba: [f32; 4] = serde_json::from_value(expected.clone())?;
    let rgb = rgba[..3]
        .iter()
        .zip([10., 15., 27.])
        .map(|(v, background)| (v * 255. * rgba[3] + background * (1. - rgba[3])).round() as i16)
        .collect::<Vec<_>>();
    let (w, h) = (image.width as usize, image.height as usize);
    let mut pixels = 0;
    for y in h / 2 - 6..=h / 2 + 6 {
        for x in w / 8..w * 7 / 8 {
            let at = (y * w + x) * 4;
            if (0..3).all(|i| (i16::from(image.bytes[at + i]) - rgb[i]).abs() <= 4) {
                pixels += 1;
            }
        }
    }
    ensure!(
        pixels > w / 10,
        "Rendered line does not use {expected}: only {pixels} pixels after {counts:?}"
    );
    Ok((
        serde_json::json!({"counts":counts,"line_color":expected,"colored_pixels":pixels}),
        image,
    ))
}
fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "Playtest input regression".into(),
            window_width: 1280,
            window_height: 720,
            ..Default::default()
        },
        async {
            if let Err(error) = run().await {
                eprintln!("{error:#}");
                std::process::exit(1);
            }
            std::process::exit(0);
        },
    );
}
async fn run() -> Result<()> {
    let output = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "artifacts/checks/playtest-input".into()),
    );
    std::fs::create_dir_all(&output)?;
    let output = output.canonicalize()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let _runtime = runtime.enter();
    let mut editor = Editor::new(ChartDocument::empty("Practice input", 120.)?);
    for (kind, start, end) in [
        (NoteKind::Tap, 2, 2),
        (NoteKind::Hold, 4, 6),
        (NoteKind::Drag, 7, 7),
        (NoteKind::Flick, 9, 9),
    ] {
        editor.execute(Command::AddNote {
            line: 0,
            note: Note {
                kind,
                start_time: Beat::new(start, 0, 1)?,
                end_time: Beat::new(end, 0, 1)?,
                ..Default::default()
            },
        })?;
    }
    let original = editor.document().as_json().clone();
    let resources = tempfile::tempdir()?;
    let frames = 8 * 8_000_u32;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(frames * 2 + 36).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&8_000_u32.to_le_bytes());
    wav.extend_from_slice(&16_000_u32.to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(frames * 2).to_le_bytes());
    wav.resize(44 + frames as usize * 2, 0);
    std::fs::write(resources.path().join("tail.wav"), wav)?;
    let options = ProjectOptions {
        music: Some("tail.wav".into()),
        ..Default::default()
    };
    let mut preview = Preview::new(editor.document(), Some(resources.path()), &options).await?;
    ensure!(
        preview.music_duration() == Some(8.),
        "Decoded music duration is unavailable"
    );
    preview.set_editor_options(
        &EditorOptions {
            mark_active_line_id: false,
            // Editor highlighting must never conceal the native AP/FC color.
            highlight_current_line: true,
            ..Default::default()
        },
        0,
    )?;
    let mode = std::env::args().nth(2).unwrap_or_default();
    if mode == "retina" {
        preview.fit_display([2388., 1343.25], 16. / 9.);
    }
    for _ in 0..10 {
        preview.render(0., preview.target_viewport());
        next_frame().await;
    }
    preview.begin_playtest(0.).await?;
    preview.set_playtest_viewport((0, 0, 1280, 720));
    preview.set_playback(0., false, true)?;
    preview.render(0., preview.target_viewport());
    let (initial, image) = check_line_color(&preview, [0; 4])?;
    image.export_png(output.join("initial-ap.png").to_str().unwrap());
    let mut colors = vec![initial];
    let mut images = Vec::new();
    let mut previous_counts = [0; 4];
    for _ in 0..3 {
        preview.pump_playtest_input()?;
        next_frame().await;
    }
    std::fs::write(output.join("ready"), "ready\n")?;
    let start = get_time();
    loop {
        preview.pump_playtest_input()?;
        let elapsed = get_time() - start;
        // Grade probes park the chart inside the native timing window while
        // receiving a real X11 click. Process scheduling cannot change its grade.
        // The ordinary Tap/Hold/Drag/Flick scenarios keep the real frame clock.
        let time = match mode.as_str() {
            "good" => 0.87 + (elapsed - 0.8).max(0.),
            "bad" => 0.80 + (elapsed - 0.8).max(0.),
            _ => elapsed,
        };
        preview.set_playback(time, true, false)?;
        preview.render(time, preview.target_viewport());
        let counts = preview.playtest_report().unwrap().counts;
        if counts != previous_counts {
            let (mut color, image) = check_line_color(&preview, counts)?;
            color["seconds"] = serde_json::json!(time);
            colors.push(color);
            // Compress captures after the timed input sequence has finished.
            images.push(image);
            previous_counts = counts;
        }
        set_default_camera();
        clear_background(BLACK);
        draw_texture_ex(
            preview.render_texture(),
            0.,
            0.,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(1280., 720.)),
                flip_y: true,
                ..Default::default()
            },
        );
        if time > 5.2 {
            preview.set_playback(time, false, false)?;
            preview.render(time, preview.target_viewport());
            let (_, image) = check_line_color(&preview, counts)?;
            image.export_png(output.join("paused-color.png").to_str().unwrap());
            for (index, image) in images.iter().enumerate() {
                image.export_png(
                    output
                        .join(format!("judge-{}.png", index + 1))
                        .to_str()
                        .unwrap(),
                );
            }
            ensure!(
                !preview.playtest_report().unwrap().finished,
                "Playtest ended at the last note instead of the music tail"
            );
            let mut timing = Vec::new();
            for (at, finished) in [(7.99, false), (8.4, false), (9.49, false), (9.51, true)] {
                preview.set_playback(at, false, true)?;
                preview.render(at, preview.target_viewport());
                ensure!(
                    preview.playtest_report().unwrap().finished == finished,
                    "Wrong music-tail completion at {at}"
                );
                timing.push(serde_json::json!({"seconds":at,"finished":finished}));
            }
            let report = preview.playtest_report().unwrap();
            image.export_png(output.join("result.png").to_str().unwrap());
            ensure!(report.finished, "Practice never finished");
            ensure!(report.total == 4, "Wrong practice total");
            std::fs::write(
                output.join("report.json"),
                serde_json::to_vec_pretty(
                    &serde_json::json!({"score":report.score,"accuracy":report.accuracy,"counts":report.counts,"max_combo":report.max_combo,"total":report.total,"finished":report.finished,"line_colors":colors,"music_seconds":8,"last_note_seconds":4.5,"completion":timing}),
                )?,
            )?;
            break;
        }
        next_frame().await;
    }
    preview.begin_playtest(1.5).await?;
    preview.render(1.5, preview.target_viewport());
    let (_, image) = check_line_color(&preview, [0; 4])?;
    image.export_png(output.join("retry-ap.png").to_str().unwrap());
    ensure!(
        preview.playtest_report().unwrap().total == 3,
        "Cursor practice counts skipped notes"
    );
    preview.end_playtest();
    let mut offsets = Vec::new();
    for (meta_ms, manifest_ms, expected_end) in [(500, 250, 7.25), (-500, -750, 9.25)] {
        let mut raw = original.clone();
        raw["META"]["offset"] = serde_json::json!(meta_ms);
        let document = ChartDocument::from_value(raw)?;
        let options = ProjectOptions {
            manifest_offset_ms: manifest_ms,
            ..options.clone()
        };
        preview
            .replace_chart(&document, Some(resources.path()), &options)
            .await?;
        ensure!(
            preview.music_duration() == Some(expected_end),
            "Chart/Music offset is not applied to duration"
        );
        preview.begin_playtest(0.).await?;
        for (at, finished) in [(expected_end - 0.01, false), (expected_end + 1.51, true)] {
            preview.set_playback(at, false, true)?;
            preview.render(at, preview.target_viewport());
            ensure!(
                preview.playtest_report().unwrap().finished == finished,
                "Offset chart completed at the wrong time"
            );
        }
        offsets.push(serde_json::json!({"meta_ms":meta_ms,"manifest_ms":manifest_ms,"chart_end":expected_end}));
        preview.end_playtest();
    }
    preview
        .replace_chart(editor.document(), None, &ProjectOptions::default())
        .await?;
    ensure!(
        preview.music_duration().is_none(),
        "Previous project's music duration leaked into a chart without music"
    );
    preview.begin_playtest(0.).await?;
    preview.set_playback(7.01, false, true)?;
    preview.render(7.01, preview.target_viewport());
    ensure!(
        preview.playtest_report().unwrap().finished,
        "Standalone chart fallback cannot finish"
    );
    preview.end_playtest();
    std::fs::write(
        output.join("duration-report.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"offsets":offsets,"no_music_fallback":true}),
        )?,
    )?;
    ensure!(
        preview.skin_report()["line_color"] == preview.skin_report()["perfect"],
        "Exit did not restore the editor AP color"
    );
    preview.begin_playtest(0.).await?;
    ensure!(
        preview.playtest_report().unwrap().total == 4,
        "Retry failed to restore native fake flags"
    );
    preview.end_playtest();
    ensure!(
        editor.document().as_json() == &original,
        "Practice changed authored data"
    );
    Ok(())
}
