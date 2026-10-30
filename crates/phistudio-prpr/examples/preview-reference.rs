//! Compare the adapter with the unchanged GameScene, including its private HUD.
//! Run tools/preview-reference.py under Xvfb; no user projects are touched.
use anyhow::{Context, Result, ensure};
use glyph_brush::ab_glyph::FontArc;
use macroquad::prelude::*;
use phistudio_core::{ProjectOptions, load_chart};
use phistudio_prpr::Preview;
use prpr::{
    config::{Config, Mods},
    fs::fs_from_file,
    info::ChartInfo,
    scene::{GameMode, GameScene, LoadingScene, Scene},
    time::TimeManager,
    ui::{TextPainter, Ui},
};
use std::{cell::Cell, path::PathBuf, rc::Rc};

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "Phira preview reference".into(),
            window_width: 1280,
            window_height: 720,
            sample_count: 1,
            ..Default::default()
        },
        async {
            // Native FileSystem uses Tokio filesystem futures. Pollster supplies
            // a wakeable executor; Macroquad's frame waker intentionally panics.
            if let Err(error) = pollster::block_on(verify()) {
                eprintln!("Reference comparison failed: {error:#}");
                std::process::exit(1);
            }
        },
    );
}

async fn verify() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let _guard = runtime.enter();
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .context("Expected output directory")?,
    )
    .canonicalize()?;
    std::env::set_current_dir(&directory)?;
    let mut text = TextPainter::new(
        FontArc::try_from_slice(include_bytes!("../../../assets/fonts/phira.ttf"))?,
        None,
    );
    let mut reports = Vec::new();
    let shader_cases = directory.join("shader-cases.json");
    let shader_mode = shader_cases.exists();
    let cases: Vec<(f32, bool, bool, Option<serde_json::Value>)> = if shader_mode {
        serde_json::from_slice::<Vec<serde_json::Value>>(&std::fs::read(shader_cases)?)?
            .into_iter()
            .map(|v| {
                (
                    v["aspect"].as_f64().unwrap_or(16. / 9.) as f32,
                    false,
                    false,
                    Some(v["extra"].clone()),
                )
            })
            .collect()
    } else {
        vec![
            (16. / 9., false, false, None),
            (1.5, false, false, None),
            (2., true, false, None),
            (16. / 9., true, false, None),
            (16. / 9., false, true, None),
            (16. / 9., true, true, None),
            (16. / 9., false, false, None),
            (16. / 9., false, false, None), // Storyboard Text: native missing-glyph boxes.
        ]
    };
    for (case, (aspect, fixed_attach, attached, extra)) in cases.into_iter().enumerate() {
        if let Some(extra) = &extra {
            std::fs::write(directory.join("extra.json"), serde_json::to_vec(extra)?)?;
        } else if directory.join("extra.json").exists() {
            std::fs::remove_file(directory.join("extra.json"))?;
        }
        let chart_file = if !shader_mode && case == 7 {
            "chart-text-emoji.json"
        } else if attached {
            "chart-attached.json"
        } else {
            "chart.json"
        };
        let document = load_chart(&directory.join(chart_file))?;
        let original = document.as_json().clone();
        let cover = if case == 6 {
            "wide-cover.png"
        } else {
            "cover.png"
        };
        let options = ProjectOptions {
            illustration: Some(cover.into()),
            music: Some("song.wav".into()),
            background_dim: Some(0.55),
            aspect_ratio: Some(aspect),
            force_aspect_ratio: true,
            use_attach_ui_fix: Some(fixed_attach),
            ..Default::default()
        };
        let mut preview = Preview::new(&document, Some(&directory), &options).await?;
        if directory.join("use-custom-skin").exists() {
            let metadata = phistudio_prpr::inspect_skin(&directory.join("pack"))?;
            preview.set_skin(Some(&directory.join("pack"))).await?;
            ensure!(
                preview.editor_skin().name == metadata.0,
                "Selected skin did not reach native Preview"
            );
            ensure!(
                preview.editor_skin().normal.body_texture.is_some(),
                "Native repeating Hold body was not retained"
            );
        }
        // Both renderers disable particles for these chart/shader comparisons.
        // Paused editor cursors now reconstruct effects unless explicitly off.
        preview.set_editor_options(
            &phistudio_core::EditorOptions {
                preview_particles: false,
                mark_active_line_id: false,
                highlight_current_line: false,
                ..Default::default()
            },
            0,
        )?;
        let mut fs = fs_from_file(&directory)?;
        let (illustration, background, _) = LoadingScene::load(fs.as_mut(), cover).await?;
        let mut config = Config::default();
        config.mods = Mods::AUTOPLAY;
        config.res_pack_path = Some(directory.join("pack").display().to_string());
        config.volume_music = 0.;
        config.volume_sfx = 0.;
        config.particle = false;
        config.disable_effect = extra.is_none();
        config.interactive = false;
        let info = ChartInfo {
            chart: chart_file.into(),
            music: "song.wav".into(),
            illustration: cover.into(),
            name: document.name().into(),
            level: document.as_json()["META"]["level"].as_str().unwrap().into(),
            background_dim: 0.55,
            aspect_ratio: aspect,
            force_aspect_ratio: true,
            use_attach_ui_fix: Some(fixed_attach),
            ..Default::default()
        };
        let mut native = GameScene::new(
            GameMode::View,
            info,
            config,
            fs,
            None,
            background,
            illustration,
            None,
            None,
            None,
        )
        .await?;
        if case == 0 {
            for (name, clip) in [
                ("click", &native.res.res_pack.sfx_click),
                ("flick", &native.res.res_pack.sfx_flick),
                ("drag", &native.res.res_pack.sfx_drag),
            ] {
                write_clip(&directory.join(format!("{name}-reference.wav")), clip)?;
            }
        }
        let target = render_target(1280, 720);
        native.res.camera.render_target = Some(target);
        let clock = Rc::new(Cell::new(0.));
        let now = clock.clone();
        let mut time = TimeManager::manual(Box::new(move || now.get()));
        time.seek_to(2.);
        native.update(&mut time)?; // Complete the unchanged scene's entry fade.
        for (frame, at) in [0.25, 1.6, 2.2, 3.7].into_iter().enumerate() {
            native.chart.reset();
            native.judge.reset();
            for event in [0., 0.5, 1., 1.5, 2., 2.5, 3., 3.5, at]
                .into_iter()
                .filter(|t| *t <= at)
            {
                time.seek_to(event);
                native.update(&mut time)?;
                native.res.time = event;
                native
                    .judge
                    .update(&mut native.res, &mut native.chart, &mut native.bad_notes);
                native.chart.update(&mut native.res);
            }
            native.res.time = at;
            let mut ui = Ui::new(&mut text, Some((0, 0, 1280, 720)));
            native.render(&mut time, &mut ui)?;
            unsafe { get_internal_gl() }.flush();
            let reference = target.texture.get_texture_data();
            set_default_camera();
            preview.set_playback(at, false, true)?;
            preview.render(at, (0, 0, 1280, 720));
            let actual = preview.snapshot();
            reference.export_png(
                directory
                    .join(format!("native-{case}-{frame}.png"))
                    .to_str()
                    .unwrap(),
            );
            actual.export_png(
                directory
                    .join(format!("editor-{case}-{frame}.png"))
                    .to_str()
                    .unwrap(),
            );
            let mut different = 0;
            let mut maximum = 0;
            for (a, b) in actual
                .bytes
                .as_chunks::<4>()
                .0
                .iter()
                .zip(reference.bytes.as_chunks::<4>().0.iter())
            {
                let delta = (0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap();
                if delta > 1 {
                    different += 1;
                }
                maximum = maximum.max(delta);
            }
            let report = preview.playback_report();
            ensure!(
                report["combo"] == native.judge.combo(),
                "Native combo differs: {report}"
            );
            ensure!(
                report["score"] == native.judge.score(),
                "Native score differs: {report}"
            );
            reports.push(serde_json::json!({"case":case,"aspect":aspect,"fixed_attach_ui":fixed_attach,
                "seconds":at,"shaders":preview.shader_report(),"skin":preview.skin_report(),"different_rgb_pixels":different,"maximum_channel_delta":maximum,"playback":report}));
            std::fs::write(
                directory.join("report.json"),
                serde_json::to_vec_pretty(&reports)?,
            )?;
            println!("case {case} at {at}: {different} differing RGB pixels; max {maximum}");
            ensure!(
                different == 0,
                "Preview differs from unchanged GameScene: case {case}, time {at}"
            );
        }
        // Editor options configure the native renderer; attachUI must never move
        // the invisible editor-return target.
        let static_bounds = preview.hud_pause_bounds();
        for scale in [0.8, 1., 1.2] {
            let settings = phistudio_core::EditorOptions {
                preview_note_scale: scale,
                mark_active_line_id: false,
                highlight_current_line: false,
                ..Default::default()
            };
            preview.set_editor_options(&settings, 0)?;
            preview.render(0.7, (0, 0, 1280, 720));
            let report = preview.playback_report();
            ensure!(
                (report["editor_preview"]["note_width"].as_f64().unwrap()
                    - f64::from(scale) * prpr::core::NOTE_WIDTH_RATIO_BASE)
                    .abs()
                    < 1e-7,
                "Native note scale mismatch"
            );
            ensure!(
                preview.hud_pause_bounds() == static_bounds,
                "Static pause return moved with chart time/scale"
            );
        }
        for aspect in phistudio_core::PreviewAspect::ALL {
            let settings = phistudio_core::EditorOptions {
                preview_aspect: aspect,
                mark_active_line_id: false,
                highlight_current_line: false,
                ..Default::default()
            };
            preview.set_editor_options(&settings, 0)?;
            preview.render(1.1, (0, 0, 1280, 720));
            let actual = preview.playback_report()["editor_preview"]["aspect"]
                .as_f64()
                .unwrap();
            ensure!(
                (actual - f64::from(aspect.ratio().unwrap_or(options.aspect_ratio.unwrap()))).abs()
                    < 1e-6,
                "Aspect override mismatch"
            );
        }
        target.delete();
        ensure!(
            document.as_json() == &original,
            "Reference check modified the chart"
        );
    }
    std::fs::write(
        directory.join("result.txt"),
        "PASS\nOriginal GameScene HUD, background blur/dim, aspect and attachUI modes match RGB pixels\nAutoplay scores/combo match the original Judge\n",
    )?;
    Ok(())
}

fn write_clip(path: &std::path::Path, clip: &sasa::AudioClip) -> Result<()> {
    let frames = (clip.length() * 48_000.).ceil() as u32;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(frames * 2 + 36).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&48_000_u32.to_le_bytes());
    wav.extend_from_slice(&96_000_u32.to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(frames * 2).to_le_bytes());
    for sample in 0..frames {
        let value = clip.sample(sample as f64 / 48_000.).map_or(0., |f| f.avg());
        wav.extend_from_slice(&((value.clamp(-1., 1.) * 32767.).round() as i16).to_le_bytes());
    }
    std::fs::write(path, wav)?;
    Ok(())
}
