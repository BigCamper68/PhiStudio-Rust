//! Actual decoder, worker, cache, native renderer and note-grid overlay together.
use super::*;
use std::path::Path;
pub(super) fn wav() -> Vec<u8> {
    let sr = 24000_u32;
    let frames = sr * 4;
    let bytes = frames * 4;
    let mut wav = Vec::with_capacity(bytes as usize + 44);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(bytes + 36).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&sr.to_le_bytes());
    wav.extend_from_slice(&(sr * 4).to_le_bytes());
    wav.extend_from_slice(&4_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&bytes.to_le_bytes());
    for i in 0..frames {
        let time = f64::from(i) / f64::from(sr);
        let mut value = 0.;
        for hit in 0..8 {
            let t = time - (0.25 + f64::from(hit) * 0.5);
            if (0. ..0.22).contains(&t) {
                let envelope = (t / 0.001).min(1.) * (-t / 0.035).exp();
                let hz = [100., 650., 3300.][hit as usize % 3];
                value += 0.6 * envelope * (std::f64::consts::TAU * hz * t).cos();
            }
        }
        let sample = (value.clamp(-1., 1.) * 32760.) as i16;
        wav.extend_from_slice(&sample.to_le_bytes());
        wav.extend_from_slice(&(-sample).to_le_bytes());
    }
    wav
}
fn update(app: &mut Workbench, preview: &Preview) {
    app.audio_guide.update(
        app.session,
        app.asset_revision,
        app.project_options.music.as_deref(),
        preview.analysis_music(),
        &app.options.audio_guide,
    );
}
async fn ready(gui: &mut Gui, app: &mut Workbench, preview: &mut Preview) -> Result<()> {
    let deadline = get_time() + 20.;
    loop {
        update(app, preview);
        paint(gui, app, preview, 210.)?;
        if app.audio_guide.probe_report()["ready"] == true {
            return Ok(());
        }
        ensure!(
            get_time() < deadline,
            "Audio guide did not become ready: {}",
            app.audio_guide.probe_report()
        );
        next_frame().await;
    }
}
async fn capture(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &Path,
    name: &str,
) -> Result<Image> {
    for _ in 0..3 {
        paint(gui, app, preview, 210.)?;
        next_frame().await;
    }
    paint(gui, app, preview, 210.)?;
    let image = get_screen_data();
    image.export_png(out.join(format!("{name}.png")).to_str().unwrap());
    Ok(image)
}
fn changed(a: &Image, b: &Image, rect: Rect, ppp: f32) -> usize {
    let (w, h) = (usize::from(a.width), usize::from(a.height));
    let mut count = 0;
    for y in
        (rect.top() * ppp).ceil().max(0.) as usize..((rect.bottom() * ppp).floor() as usize).min(h)
    {
        for x in (rect.left() * ppp).ceil().max(0.) as usize
            ..((rect.right() * ppp).floor() as usize).min(w)
        {
            let at = (y * w + x) * 4;
            if a.bytes[at..at + 3] != b.bytes[at..at + 3] {
                count += 1;
            }
        }
    }
    count
}
pub(super) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &Path,
) -> Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("music.wav"), wav())?;
    let chart = root.path().join("chart.json");
    let mut raw = app.editor.document().as_json().clone();
    raw["BPMList"] =
        serde_json::json!([{"startTime":[0,0,1],"bpm":120},{"startTime":[4,0,1],"bpm":180}]);
    phistudio_core::save_chart(&ChartDocument::from_value(raw.clone())?, &chart)?;
    std::fs::write(
        root.path().join("info.yml"),
        "chart: chart.json\nmusic: music.wav\n",
    )?;
    app.replace_document(
        ChartDocument::from_value(raw.clone())?,
        Some(chart.clone()),
        Some(root.path().to_owned()),
    );
    app.audio_guide.set_cache_root(Some(root.path().to_owned()));
    preview
        .replace_chart(
            app.editor.document(),
            app.asset_root(),
            &app.project_options,
        )
        .await?;
    app.options.language = Language::Russian;
    app.options.audio_guide.waveform = true;
    app.options.audio_guide.attacks = true;
    app.timeline.zoom = 96.;
    app.transport.beat = 0.;
    ready(gui, app, preview).await?;
    let initial = app.audio_guide.probe_report();
    ensure!(
        initial["markers"].as_array().unwrap().len() == 8,
        "Expected eight attacks without release markers: {initial}"
    );
    let enabled = capture(gui, app, preview, out, "guide-on").await?;
    let grid = crate::grid_input::geometry(app.timeline.area, app);
    let revision = app.editor.revision();
    app.options.audio_guide.waveform = false;
    app.options.audio_guide.attacks = false;
    let disabled = capture(gui, app, preview, out, "guide-off").await?;
    let note_pixels = changed(&enabled, &disabled, grid.rect, app.pixels_per_point);
    let event_pixels = changed(
        &enabled,
        &disabled,
        grid.event_rect.shrink(2.),
        app.pixels_per_point,
    );
    ensure!(
        note_pixels > 100,
        "Waveform/markers were not actually drawn"
    );
    ensure!(
        event_pixels == 0,
        "Audio guides leaked into the event grid: {event_pixels}"
    );
    app.options.audio_guide.waveform = true;
    app.options.audio_guide.attacks = true;
    let mut cases = vec![];
    for (name, offset, beat, zoom) in [
        ("positive-offset", 250, 1., 160.),
        ("negative-offset", -250, 3., 64.),
    ] {
        app.project_options.manifest_offset_ms = offset;
        app.transport.beat = beat;
        app.timeline.zoom = zoom;
        preview
            .replace_chart(
                app.editor.document(),
                app.asset_root(),
                &app.project_options,
            )
            .await?;
        update(app, preview);
        capture(gui, app, preview, out, name).await?;
        ensure!(
            app.audio_guide.probe_report()["epoch"] == initial["epoch"],
            "Timing changes repeated spectral analysis"
        );
        cases.push(serde_json::json!({"offset_ms":offset,"beat":beat,"zoom":zoom}));
    }
    ensure!(
        app.editor.document().as_json() == &raw && app.editor.revision() == revision,
        "Audio guides changed chart/history"
    );
    app.replace_document(
        ChartDocument::from_value(raw.clone())?,
        Some(chart),
        Some(root.path().to_owned()),
    );
    preview
        .replace_chart(
            app.editor.document(),
            app.asset_root(),
            &app.project_options,
        )
        .await?;
    ready(gui, app, preview).await?;
    let reopened = app.audio_guide.probe_report();
    ensure!(
        reopened["cached"] == true,
        "Project switch lost the shared cache"
    );
    capture(gui, app, preview, out, "reopened-cached").await?;
    std::fs::write(
        out.join("audio-guide.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"initial":initial,"reopened":reopened,
        "changed_note_pixels":note_pixels,"changed_event_pixels":event_pixels,"timing_cases":cases,"chart_unchanged":true}))?,
    )?;
    println!(
        "PASS audio guide: decoder, async analysis, note-only GPU overlay, BPM/offset/zoom, cache and unchanged chart"
    );
    Ok(())
}
