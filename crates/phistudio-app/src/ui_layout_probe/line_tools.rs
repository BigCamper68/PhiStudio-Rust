//! Native pivot pixels, unchanged note pixels, and the real texture-chooser UI.
use super::*;
use phistudio_core::{Beat, Command, Note};

fn red_bounds(image: &Image) -> Result<[f32; 4]> {
    let (mut left, mut top, mut right, mut bottom) =
        (image.width as usize, image.height as usize, 0, 0);
    let mut found = false;
    for y in 0..image.height as usize {
        for x in 0..image.width as usize {
            let i = (y * image.width as usize + x) * 4;
            if image.bytes[i] > 200 && image.bytes[i + 1] < 60 && image.bytes[i + 2] < 60 {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x);
                bottom = bottom.max(y);
                found = true;
            }
        }
    }
    ensure!(found, "Native texture was not drawn");
    Ok([
        left as f32,
        top as f32,
        right as f32 + 1.,
        bottom as f32 + 1.,
    ])
}
async fn native(
    preview: &mut Preview,
    document: &ChartDocument,
    root: &std::path::Path,
) -> Result<Image> {
    preview
        .replace_chart(document, Some(root), &Default::default())
        .await?;
    preview.set_editor_options(
        &phistudio_core::EditorOptions {
            preview_hud: false,
            preview_particles: false,
            mark_active_line_id: false,
            highlight_current_line: false,
            ..Default::default()
        },
        usize::MAX,
    )?;
    preview.fit_display([1280., 720.], 16. / 9.);
    for _ in 0..3 {
        preview.render(0., preview.target_viewport());
        next_frame().await;
    }
    preview.render(0., preview.target_viewport());
    ensure!(
        preview.line_count() == 1 && preview.line_states().len() == 1 && preview.note_count() == 1,
        "Visual pivots changed source line/note counts"
    );
    Ok(preview.render_texture().get_texture_data())
}
pub(super) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &std::path::Path,
) -> Result<()> {
    let root = tempfile::tempdir()?;
    let assets = root.path().join("projects/probe");
    std::fs::create_dir_all(assets.join("textures"))?;
    Image::gen_image_color(200, 100, Color::from_rgba(240, 30, 30, 255))
        .export_png(assets.join("anchor.png").to_str().unwrap());
    for i in 0..20 {
        Image::gen_image_color(
            20 + i * 7,
            20 + (20 - i) * 4,
            Color::from_rgba(40 + i as u8 * 8, 180, 220, 255),
        )
        .export_png(
            assets
                .join(format!("textures/texture-{i:02}.png"))
                .to_str()
                .unwrap(),
        );
    }
    std::fs::write(assets.join("bad.png"), b"not an image")?;
    let mut raw = ChartDocument::empty("Anchor pixels", 120.)?
        .as_json()
        .clone();
    raw["judgeLineList"][0]["Texture"] = serde_json::json!("anchor.png");
    let beat = Beat::new(0, 1, 2)?;
    raw["judgeLineList"][0]["notes"] = serde_json::json!([serde_json::to_value(Note {
        start_time: beat,
        end_time: beat,
        x: 400.,
        ..Default::default()
    })?]);
    raw["judgeLineList"][0]["numOfNotes"] = serde_json::json!(1);
    let baseline = native(preview, &ChartDocument::from_value(raw.clone())?, &assets).await?;
    let base = red_bounds(&baseline)?;
    baseline.export_png(out.join("anchor-center.png").to_str().unwrap());
    let mut up = raw.clone();
    up["judgeLineList"][0]["eventLayers"][0]["moveYEvents"][0]["start"] = serde_json::json!(30);
    up["judgeLineList"][0]["eventLayers"][0]["moveYEvents"][0]["end"] = serde_json::json!(30);
    let up = native(preview, &ChartDocument::from_value(up)?, &assets).await?;
    let up = red_bounds(&up)?;
    let y_sign = ((up[1] + up[3]) - (base[1] + base[3])).signum();
    ensure!(y_sign != 0., "Positive native Y reference did not move");
    let mut cases = vec![];
    for anchor in [[0., 0.], [1., 1.], [-0.25, 1.5]] {
        let mut value = raw.clone();
        value["judgeLineList"][0]["anchor"] = serde_json::json!(anchor);
        let image = native(preview, &ChartDocument::from_value(value)?, &assets).await?;
        let bounds = red_bounds(&image)?;
        let dx = (bounds[0] + bounds[2] - base[0] - base[2]) * 0.5;
        let dy = (bounds[1] + bounds[3] - base[1] - base[3]) * 0.5;
        let expected_x = (0.5 - anchor[0]) * (base[2] - base[0]);
        let expected_y = (0.5 - anchor[1]) * (base[3] - base[1]) * y_sign;
        ensure!(
            (dx - expected_x).abs() < 3. && (dy - expected_y).abs() < 3.,
            "Anchor {anchor:?}: native pixel shift {dx},{dy}, expected {expected_x},{expected_y}"
        );
        let mut note_pixels = 0;
        for y in 0..image.height as usize {
            for x in (image.width as usize * 7 / 10)..image.width as usize {
                let at = (y * image.width as usize + x) * 4;
                ensure!(
                    image.bytes[at..at + 4] == baseline.bytes[at..at + 4],
                    "Anchor moved pixels in the separate note/HUD region"
                );
                if baseline.bytes[at..at + 3] != [10, 15, 27] {
                    note_pixels += 1;
                }
            }
        }
        ensure!(note_pixels > 50, "Unchanged-note region contains no note");
        image.export_png(
            out.join(format!("anchor-{}-{}.png", anchor[0], anchor[1]))
                .to_str()
                .unwrap(),
        );
        cases.push(serde_json::json!({"anchor":anchor,"bounds":bounds,"shift":[dx,dy],"unchanged_note_pixels":note_pixels}));
    }
    let chart = assets.join("chart.json");
    phistudio_core::save_chart(&ChartDocument::from_value(raw.clone())?, &chart)?;
    app.replace_document(
        ChartDocument::from_value(raw.clone())?,
        Some(chart),
        Some(assets.clone()),
    );
    app.workspace.root = Some(root.path().to_owned());
    preview
        .replace_chart(app.editor.document(), Some(&assets), &app.project_options)
        .await?;
    app.show_line_properties = true;
    let mut ui_cases = vec![];
    for language in [Language::English, Language::Russian, Language::Japanese] {
        app.options.language = language;
        for _ in 0..6 {
            paint(gui, app, preview, 210.)?;
            next_frame().await;
        }
        paint(gui, app, preview, 210.)?;
        super::multiline::capture_screen()?.export_png(
            out.join(format!("properties-{}.png", language.code()))
                .to_str()
                .unwrap(),
        );
        app.open_line_textures();
        let deadline = get_time() + 15.;
        loop {
            paint(gui, app, preview, 210.)?;
            next_frame().await;
            let report = app.line_tools.probe_report();
            if report["catalog_loaded"] == true
                && report["ready"].as_u64().unwrap_or(0) > 0
                && report["pending"] == 0
            {
                break;
            }
            ensure!(
                get_time() < deadline,
                "Texture worker did not finish: {report}"
            );
        }
        paint(gui, app, preview, 210.)?;
        super::multiline::capture_screen()?.export_png(
            out.join(format!("textures-{}.png", language.code()))
                .to_str()
                .unwrap(),
        );
        ui_cases.push(serde_json::json!({"language":language.code(),"textures":app.line_tools.probe_report()}));
        app.back();
    }
    ensure!(
        app.editor.document().as_json() == &raw && !app.editor.is_dirty(),
        "Texture chooser changed chart without Apply"
    );
    app.commit_authoring(Command::AddLines { count: 3 });
    ensure!(
        app.editor.document().line_count() == 4 && app.line == 1,
        "Create lines focus/count"
    );
    app.history_step(false);
    ensure!(
        app.editor.document().as_json() == &raw,
        "Create lines did not undo exactly"
    );
    std::fs::write(
        out.join("line-tools.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"native_anchor_cases":cases,"texture_ui":ui_cases,"chart_unchanged":true,"creation_undo_exact":true}),
        )?,
    )?;
    println!(
        "PASS: native anchor pixels, unchanged notes, source counts, texture thumbnails and creation Undo"
    );
    Ok(())
}
