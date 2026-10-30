//! Actual PCM analysis, explicit planning, grid-only ghosts and native notes.
use super::*;
use phistudio_core::{Command, Editor, tools::AudioPosition};
use std::path::Path;
async fn frame(gui: &mut Gui, app: &mut Workbench, preview: &mut Preview) -> Result<()> {
    app.audio_guide.generation_requested = app.audio_notes.show;
    app.audio_guide.update(
        app.session,
        app.asset_revision,
        app.project_options.music.as_deref(),
        preview.analysis_music(),
        &app.options.audio_guide,
    );
    paint(gui, app, preview, 260.)?;
    next_frame().await;
    Ok(())
}
async fn capture(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &Path,
    name: &str,
) -> Result<Image> {
    for _ in 0..4 {
        frame(gui, app, preview).await?;
    }
    paint(gui, app, preview, 260.)?;
    let image = get_screen_data();
    image.export_png(out.join(format!("{name}.png")).to_str().unwrap());
    Ok(image)
}
pub(super) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &Path,
) -> Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("music.wav"), audio::wav())?;
    let mut editor = Editor::new(ChartDocument::empty("Audio note generator", 120.)?);
    editor.execute(Command::AddLines { count: 2 })?;
    let mut raw = editor.document().as_json().clone();
    raw["META"]["song"] = serde_json::json!("music.wav");
    let document = ChartDocument::from_value(raw)?;
    let path = root.path().join("chart.json");
    phistudio_core::save_chart(&document, &path)?;
    *app = Workbench::new(document, Some(path));
    app.project_options.music = Some("music.wav".into());
    app.show_context = false;
    app.multi_line.enabled = true;
    app.multi_line.lines = vec![0, 1, 2];
    app.multi_line.merged = false;
    app.options.audio_notes.position = AudioPosition::PingPong;
    app.options.audio_notes.per_band = true;
    app.options.audio_notes.lanes = 5;
    preview
        .replace_chart(
            app.editor.document(),
            Some(root.path()),
            &app.project_options,
        )
        .await?;
    app.skin = Some(preview.editor_skin());
    let source = app.editor.document().as_json().clone();
    app.editor.mark_saved();
    let options = app.options.audio_guide.clone();
    app.open_audio_notes();
    app.audio_notes.multiple = true;
    let deadline = get_time() + 30.;
    while app
        .audio_guide
        .generation_markers(&app.options.audio_guide)
        .is_none()
    {
        ensure!(
            get_time() < deadline,
            "Analysis timed out: {}",
            app.audio_guide.probe_report()
        );
        frame(gui, app, preview).await?;
    }
    let epoch = app.audio_guide.probe_report()["epoch"].clone();
    app.build_audio_note_preview();
    let deadline = get_time() + 20.;
    while app.audio_note_report()["valid"] != true {
        ensure!(
            get_time() < deadline,
            "Planning timed out: {}",
            app.audio_note_report()
        );
        frame(gui, app, preview).await?;
    }
    app.focus_audio_note_preview();
    let staged = app.audio_note_report();
    let count = staged["count"].as_u64().unwrap() as usize;
    ensure!(count >= 6, "Too few synthetic attacks: {staged}");
    ensure!(
        app.editor.document().as_json() == &source
            && !app.editor.is_dirty()
            && preview.note_count() == 0,
        "Staging edited the document/native scene"
    );
    ensure!(
        app.options.audio_guide == options,
        "The generator enabled passive guides"
    );
    for language in [Language::English, Language::Russian, Language::Japanese] {
        app.options.language = language;
        capture(
            gui,
            app,
            preview,
            out,
            &format!("generator-{}", language.code()),
        )
        .await?;
    }
    // Collapse the actual egui window to inspect the grid on compact viewports.
    let deadline = get_time() + 0.35;
    while get_time() < deadline {
        paint_observed(gui, app, preview, 260., |ctx| {
            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                ctx,
                Id::new("Notes from audio markers").with("collapsing"),
                true,
            );
            state.set_open(false);
            state.store(ctx);
        })?;
        next_frame().await;
    }
    let visible = capture(gui, app, preview, out, "ghost-notes").await?;
    app.audio_notes.hide_ghosts = true;
    let hidden = capture(gui, app, preview, out, "ghosts-hidden").await?;
    let rect = app.multi_line.base.unwrap().rect;
    let p = app.pixels_per_point;
    let mut changed = 0;
    for y in (rect.top() * p).ceil().max(0.) as usize
        ..(rect.bottom() * p).floor().min(visible.height as f32) as usize
    {
        for x in (rect.left() * p).ceil().max(0.) as usize
            ..(rect.right() * p).floor().min(visible.width as f32) as usize
        {
            // Macroquad's raw readback is bottom-up; PNG export flips rows.
            let i = ((visible.height as usize - 1 - y) * visible.width as usize + x) * 4;
            if visible.bytes[i..i + 3] != hidden.bytes[i..i + 3] {
                changed += 1;
            }
        }
    }
    ensure!(
        changed > 50,
        "Grid-only preview notes were not drawn: {changed}"
    );
    app.audio_notes.hide_ghosts = false;
    // The staged notes use the same native skin SFX player without entering
    // the chart/Judge. Exercise the real player, not only a counted callback.
    preview.set_audio_levels(0.5, 0.5)?;
    app.playing = true;
    app.audition_audio_notes(0., true, |kind| preview.audition(None, kind))?;
    let mut preview_hitsounds = 0;
    for step in 1..=90 {
        preview_hitsounds += app.audition_audio_notes(f64::from(step) * 0.05, false, |kind| {
            preview.audition(None, kind)
        })?;
    }
    app.playing = false;
    ensure!(
        preview_hitsounds == count,
        "Staged hitsounds differ from preview count"
    );
    ensure!(
        app.audition_audio_notes(5., false, |_| anyhow::bail!("Paused audition played"))? == 0
            && app.editor.document().as_json() == &source
            && preview.note_count() == 0,
        "Audition changed the chart/native scene or played while paused"
    );
    app.apply_audio_note_preview();
    ensure!(
        app.editor.document().note_count() == count,
        "Inserted count differs from preview"
    );
    let after = app.editor.document().as_json().clone();
    preview
        .replace_chart(
            app.editor.document(),
            Some(root.path()),
            &app.project_options,
        )
        .await?;
    ensure!(
        preview.note_count() == count,
        "Generated notes did not reach native prpr"
    );
    capture(gui, app, preview, out, "inserted-notes").await?;
    app.history_step(false);
    ensure!(
        app.editor.document().as_json() == &source && !app.editor.is_dirty(),
        "One Undo did not restore the exact source"
    );
    app.history_step(true);
    ensure!(
        app.editor.document().as_json() == &after,
        "Redo changed generated data"
    );
    app.open_audio_notes();
    app.build_audio_note_preview();
    let deadline = get_time() + 20.;
    while app.audio_note_report()["valid"] != true {
        ensure!(get_time() < deadline, "Repeat planning timed out");
        frame(gui, app, preview).await?;
    }
    ensure!(
        app.audio_note_report()["count"] == 0,
        "Repeated generation duplicates existing notes"
    );
    ensure!(
        app.audio_guide.probe_report()["epoch"] == epoch,
        "Chart generation repeated FFT"
    );
    let report = serde_json::json!({"staged":staged,"ghost_pixels":changed,"preview_hitsounds":preview_hitsounds,"native_notes":count,"exact_undo_redo":true,"repeat_count":0,"fft_reused":true,"passive_flags_preserved":true,"framebuffer":[screen_width(),screen_height()],"pixels_per_point":p});
    std::fs::write(
        out.join("audio-notes.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "PASS: real analysis/planner, {count} native notes, {changed} ghost pixels, exact one-step Undo/Redo, idempotent repeat, reused FFT"
    );
    Ok(())
}
