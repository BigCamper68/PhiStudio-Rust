//! Real file-worker gain, actual native preview bounds and the production forms.
use super::*;
use phistudio_core::{Command, NoteSoundGain};
use std::path::Path;

async fn capture(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &Path,
    name: &str,
    scroll: Option<f32>,
) -> Result<()> {
    for _ in 0..4 {
        paint_observed(gui, app, preview, 270., |ctx| {
            if let Some(fraction) = scroll {
                let zones = ctx
                    .data_mut(|d| {
                        d.get_temp::<Vec<crate::scroll::Zone>>(Id::new("phistudio-scroll-zones"))
                    })
                    .unwrap_or_default();
                for zone in zones
                    .into_iter()
                    .filter(|z| z.axis == 1 && z.layer != egui::LayerId::background())
                {
                    if let Some(mut state) = egui::scroll_area::State::load(ctx, zone.id) {
                        state.offset.y = zone.max_offset * fraction;
                        state.store(ctx, zone.id);
                    }
                }
            }
        })?;
        next_frame().await;
    }
    paint(gui, app, preview, 270.)?;
    get_screen_data().export_png(out.join(format!("{name}.png")).to_str().unwrap());
    Ok(())
}
pub(super) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &Path,
) -> Result<()> {
    let root = tempfile::tempdir()?;
    let chart = root.path().join("chart.json");
    phistudio_core::save_chart(app.editor.document(), &chart)?;
    let doc = app.editor.document().clone();
    *app = Workbench::new(doc, Some(chart));
    app.skin = Some(preview.editor_skin());
    let id = NoteId { line: 0, index: 0 };
    app.select(Some(id));
    let mut files = crate::files::FileManager::new(None);
    files.update(app);
    let mut note = app.editor.document().note(0, 0)?;
    note.judge_area = Some(1.75);
    note.tint_hit_effects = Some([80, 210, 255]);
    app.execute(Command::UpdateNote { id, note });
    // Exercise the same request/worker/completion path used on Android and iOS.
    app.set_note_volume(id, 0.5);
    let deadline = get_time() + 20.;
    loop {
        files.update(app);
        if app.editor.document().note_sound_gain(id).is_some() {
            break;
        }
        ensure!(app.error.is_none(), "Gain worker: {:?}", app.error);
        ensure!(get_time() < deadline, "Gain worker timed out");
        next_frame().await;
    }
    let gain: NoteSoundGain = app.editor.document().note_sound_gain(id).unwrap();
    ensure!(
        app.selected == Some(id),
        "Volume editing cleared the note selection"
    );
    ensure!(
        gain.volume == 0.5 && gain.source.is_none(),
        "Gain did not retain its original source"
    );
    let original = app.editor.document().as_json().clone();
    preview
        .replace_chart(
            app.editor.document(),
            Some(root.path()),
            &app.project_options,
        )
        .await?;
    app.options.preview_behind_grid = true;
    app.show_context = false;
    let mut cases = vec![];
    for multi in [false, true] {
        app.multi_line.enabled = multi;
        app.multi_line.lines = vec![0];
        capture(
            gui,
            app,
            preview,
            out,
            if multi {
                "grid-preview-multiple"
            } else {
                "grid-preview"
            },
            None,
        )
        .await?;
        let surface = app.multi_line.base.unwrap().full_rect();
        ensure!(
            surface.contains_rect(app.preview.rect),
            "Preview overlaps a grid header"
        );
        ensure!(
            (app.preview.rect.width() / app.preview.rect.height() - app.preview_aspect()).abs()
                < 0.001,
            "Preview was cropped or stretched"
        );
        cases.push(serde_json::json!({"multiple":multi,"surface":[surface.left(),surface.top(),surface.width(),surface.height()],"preview":[app.preview.rect.left(),app.preview.rect.top(),app.preview.rect.width(),app.preview.rect.height()]}));
    }
    app.multi_line.enabled = false;
    app.show_context = true;
    app.compact_properties = true;
    for language in [Language::English, Language::Russian, Language::Japanese] {
        app.options.language = language;
        capture(
            gui,
            app,
            preview,
            out,
            &format!("note-properties-{}", language.code()),
            Some(1.),
        )
        .await?;
    }
    app.compact_properties = false;
    app.select(None);
    app.show_settings = true;
    app.options.language = Language::Russian;
    for (i, fraction) in [0.45, 0.60].into_iter().enumerate() {
        capture(
            gui,
            app,
            preview,
            out,
            &format!("numeric-settings-{i}"),
            Some(fraction),
        )
        .await?;
    }
    ensure!(
        app.editor.document().as_json() == &original,
        "Viewing properties changed the chart"
    );
    let report = serde_json::json!({"gain_worker":true,"gain":gain,"preview_cases":cases,"chart_unchanged":true,"framebuffer":[screen_width(),screen_height()]});
    std::fs::write(
        out.join("note-properties.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("PASS: native note properties, gain worker, grid preview fitted below headers");
    Ok(())
}
