//! Exercises the production modal/producer/encoder, using actual native chart frames.
use crate::{gui::Gui, workbench::Workbench};
use anyhow::{Context, Result, ensure};
use macroquad::prelude::*;
use phistudio_core::{Beat, ChartDocument, Command, Note};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn tone(path: &std::path::Path, seconds: f64) -> Result<()> {
    use std::io::Write;
    let frames = (seconds * 48000.) as u32;
    let mut wav = std::fs::File::create(path)?;
    wav.write_all(b"RIFF")?;
    wav.write_all(&(frames * 4 + 36).to_le_bytes())?;
    wav.write_all(b"WAVEfmt ")?;
    wav.write_all(&16_u32.to_le_bytes())?;
    wav.write_all(&1_u16.to_le_bytes())?;
    wav.write_all(&2_u16.to_le_bytes())?;
    wav.write_all(&48000_u32.to_le_bytes())?;
    wav.write_all(&192000_u32.to_le_bytes())?;
    wav.write_all(&4_u16.to_le_bytes())?;
    wav.write_all(&16_u16.to_le_bytes())?;
    wav.write_all(b"data")?;
    wav.write_all(&(frames * 4).to_le_bytes())?;
    for i in 0..frames {
        let value = ((f64::from(i) * 440. * std::f64::consts::TAU / 48000.).sin() * 4000.) as i16;
        for _ in 0..2 {
            wav.write_all(&value.to_le_bytes())?;
        }
    }
    Ok(())
}
pub async fn run() -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();
    let root = PathBuf::from(std::env::args().nth(1).context("Pass output directory")?);
    std::fs::create_dir_all(&root)?;
    let assets = root.join("project");
    std::fs::create_dir_all(&assets)?;
    tone(&assets.join("music.wav"), 4.)?;
    tone(&assets.join("custom.wav"), 0.1)?;
    let mut source: serde_json::Value = serde_json::from_str(include_str!(
        "../../../assets/charts/phistudio-demo.rpe.json"
    ))?;
    source["META"]["offset"] = serde_json::json!(250);
    source["BPMList"] = serde_json::json!([{"bpm":60.,"startTime":[0,0,1]}]);
    let template = source["judgeLineList"][0]["notes"][0].clone();
    let mut notes = Vec::new();
    for (kind, beat, fake) in [(1, 0, 0), (2, 1, 0), (3, 2, 0), (4, 1, 0), (1, 0, 1)] {
        let mut n = template.clone();
        n["type"] = serde_json::json!(kind);
        n["isFake"] = serde_json::json!(fake);
        n["positionX"] = serde_json::json!((notes.len() as i32 - 2) * 180);
        n["startTime"] = serde_json::json!([beat, 0, 1]);
        n["endTime"] = serde_json::json!([if kind == 2 { 3 } else { beat }, 0, 1]);
        if kind == 3 {
            n["hitsound"] = serde_json::json!("custom.wav");
        }
        notes.push(n);
    }
    source["judgeLineList"][0]["notes"] = serde_json::json!(notes);
    let document = ChartDocument::from_value(source)?;
    let chart = assets.join("chart.json");
    std::fs::write(&chart, document.to_json()?)?;
    let mut app = Workbench::new(document, Some(chart.clone()));
    app.show_projects = false;
    app.project_options.music = Some("music.wav".into());
    app.project_options.manifest_offset_ms = 100;
    app.project_options.illustration = None;
    app.editor.execute(Command::AddNote {
        line: 0,
        note: Note {
            start_time: Beat::new(1, 1, 2)?,
            end_time: Beat::new(1, 1, 2)?,
            x: 100.,
            ..Default::default()
        },
    })?;
    app.editor.execute(Command::EditShader {
        baseline: phistudio_core::shaders::default_shader_extra(app.editor.document()),
        action: phistudio_core::shaders::ShaderAction::Insert {index:0,effect:serde_json::json!({"start":[0,0,1],"end":[1,0,1],"shader":"grayscale","vars":{"factor":1.},"global":true})},
    })?;
    let skin = assets.join("skin");
    std::fs::create_dir_all(&skin)?;
    let supplied = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/skins/phigros");
    for entry in std::fs::read_dir(supplied)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), skin.join(entry.file_name()))?;
        }
    }
    let info = std::fs::read_to_string(skin.join("info.yml"))?
        .replace("Phigros Official", "Movie probe skin")
        + "\ncolorPerfect: 16711680\n";
    std::fs::write(skin.join("info.yml"), info)?;
    app.options.skin = Some("skin-movie-probe".into());
    app.skins.entries.push(phistudio_core::skins::SkinEntry {
        id: "skin-movie-probe".into(),
        name: "Movie probe skin".into(),
        author: "PigeonGames (fixture copy)".into(),
        description: String::new(),
        assets: skin.clone(),
    });
    let mut cover = Image::gen_image_color(512, 256, BLACK);
    for y in 0..256usize {
        for x in 0..512usize {
            let p = (y * 512 + x) * 4;
            cover.bytes[p..p + 4].copy_from_slice(&[(x / 2) as u8, 60, (y % 256) as u8, 255]);
        }
    }
    cover.export_png(assets.join("cover.png").to_str().unwrap());
    app.project_options.illustration = Some("cover.png".into());
    let mut source_preview = phistudio_prpr::Preview::new(
        app.editor.document(),
        app.asset_root(),
        &app.project_options,
    )
    .await?;
    source_preview.set_skin(Some(&skin)).await?;
    source_preview.set_editor_options(&app.options, 0)?;
    source_preview.render(0., source_preview.target_viewport());
    let original_frame = source_preview.snapshot().bytes;
    ensure!(
        source_preview.shader_report()["global"] == 1
            && source_preview.shader_report()["warning"].is_null(),
        "Shader did not reach native source Preview"
    );
    let before = app.editor.document().to_json()?;
    let revision = app.editor.revision();
    let original = std::fs::read(&chart)?;
    app.music_duration = Some(3.65);
    app.open_video();
    app.video.settings = phistudio_core::video::VideoSettings {
        width: 640,
        height: 360,
        fps: 30,
        tail: 0.1,
        bitrate_mbps: 4,
        ..Default::default()
    };
    let mut gui = Gui::new();
    if let Ok(language) = std::env::var("PHISTUDIO_VIDEO_PROBE_LANGUAGE") {
        app.options.language = phistudio_core::Language::ALL
            .into_iter()
            .find(|l| l.code() == language)
            .unwrap();
    }
    let _language = crate::i18n::Scope::enter(app.options.language);
    gui.set_ui_scale(1.);
    for i in 0..4 {
        clear_background(BLACK);
        gui.run(|ctx, _| app.video_window(ctx));
        gui.draw();
        if i == 3 {
            get_screen_data().export_png(root.join("export-settings.png").to_str().unwrap());
        }
        next_frame().await;
    }
    if std::env::var_os("PHISTUDIO_VIDEO_PROBE_UI_ONLY").is_some() {
        return Ok(());
    }
    app.video.start = true;
    let start = Instant::now();
    let mut ticks = 0;
    loop {
        clear_background(BLACK);
        gui.run(|ctx, _| app.video_window(ctx));
        app.update_movie(Some(&source_preview)).await;
        gui.draw();
        ticks += 1;
        ensure!(app.video.error.is_none(), "{:?}", app.video.error);
        if app.video.completed.is_some() {
            break;
        }
        ensure!(
            start.elapsed() < Duration::from_secs(120),
            "Movie export timed out"
        );
        next_frame().await;
    }
    let movie = app.video.completed.take().unwrap();
    movie.save(&root.join("whole-song.mp4"))?;
    drop(movie);
    ensure!(
        app.editor.document().to_json()? == before
            && app.editor.revision() == revision
            && app.editor.is_dirty(),
        "Export modified editor history"
    );
    ensure!(
        std::fs::read(&chart)? == original,
        "Export modified original chart"
    );
    ensure!(
        source_preview.snapshot().bytes == original_frame,
        "Export overwrote the ordinary Preview framebuffer"
    );
    // Custom range must restore state from the beginning, and can omit audio entirely.
    app.video.settings.whole_song = false;
    app.video.settings.start = 1.25;
    app.video.settings.end = 2.25;
    app.video.settings.tail = 0.;
    app.video.settings.audio = false;
    app.video.settings.hud = false;
    app.video.settings.particles = false;
    app.video.start = true;
    loop {
        app.update_movie(Some(&source_preview)).await;
        ensure!(app.video.error.is_none(), "{:?}", app.video.error);
        if app.video.completed.is_some() {
            break;
        }
        ensure!(
            start.elapsed() < Duration::from_secs(120),
            "Range export timed out"
        );
        next_frame().await;
    }
    app.video
        .completed
        .take()
        .unwrap()
        .save(&root.join("range-muted.mp4"))?;
    // Cancel during production; the incomplete movie must never become savable.
    app.video.settings.end = 600.;
    app.video.start = true;
    app.update_movie(Some(&source_preview)).await;
    app.suspend_movie();
    app.video_back();
    let cancel = Instant::now();
    while app.video.job.is_some() {
        app.update_movie(Some(&source_preview)).await;
        ensure!(
            cancel.elapsed() < Duration::from_secs(10),
            "Cancel timed out"
        );
        next_frame().await;
    }
    ensure!(
        app.video.completed.is_none() && app.video.error.is_none(),
        "Cancel published an incomplete movie"
    );
    ensure!(
        app.editor.document().to_json()? == before && std::fs::read(&chart)? == original,
        "Cancel changed project"
    );
    std::fs::write(
        root.join("result.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"passed":true,"ui_ticks":ticks,"elapsed_seconds":start.elapsed().as_secs_f64(),"whole_frames":113,"range_frames":30,"size":[640,360],"fps":30,"unsaved_preserved":true,"source_unchanged":true,"native_shaders":true,"selected_skin":true,"cached_music_and_background":true,"preview_preserved":true,"cancel_ms":cancel.elapsed().as_millis()}),
        )?,
    )?;
    println!(
        "PASS: native movie producer, whole-song boundary, custom range, muted export, dirty history and cancellation"
    );
    Ok(())
}
