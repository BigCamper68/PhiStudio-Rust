use super::*;
pub(super) struct Baseline {
    pub initial_lines: usize,
    pub initial_notes: usize,
    pub initial_line0: usize,
    pub bundled_demo: bool,
}
pub(super) fn step(
    app: &mut Workbench,
    preview: &mut Preview,
    directory: &std::path::Path,
    frame: u32,
    baseline: &Baseline,
) -> Result<bool> {
    let Baseline {
        initial_lines,
        initial_notes,
        initial_line0,
        bundled_demo,
    } = *baseline;
    if frame == 3 {
        ensure!(
            app.render_error.is_none() && app.error.is_none(),
            "Import/preview failed: {:?} {:?}",
            app.error,
            app.render_error
        );
        ensure!(
            preview.line_count() == initial_lines && preview.note_count() == initial_notes,
            "Imported chart did not reach prpr intact"
        );
        unsafe { get_internal_gl() }.flush();
        let screenshot = get_screen_data();
        let rendered_chart = preview.snapshot();
        rendered_chart.export_png(
            directory
                .join("prpr-preview.png")
                .to_str()
                .context("Output path must be UTF-8")?,
        );
        ensure!(
            preview.editor_skin().normal.tap.size == [989., 100.],
            "The supplied note skin did not reach prpr"
        );
        let tap_pixels = rendered_chart
            .bytes
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[2] > 160 && p[1] > 130 && p[2] > p[0].saturating_add(20))
            .count();
        ensure!(
            !bundled_demo || tap_pixels > 10,
            "prpr parsed notes but did not paint the Tap texture"
        );
        screenshot.export_png(
            directory
                .join("workbench-before.png")
                .to_str()
                .context("Output path must be UTF-8")?,
        );
        std::fs::write(
            directory.join("ui-geometry.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "pixels_per_point":app.pixels_per_point,
                "framebuffer":[screen_width(),screen_height()],
                "grid":[app.timeline.area.left(),app.timeline.area.top(),app.timeline.area.right(),app.timeline.area.bottom()],
                "preview":[app.preview.rect.left(),app.preview.rect.top(),app.preview.rect.right(),app.preview.rect.bottom()]
            }))?,
        )?;
        let start = Beat::new(2, 1, 4)?;
        let note = Note {
            kind: NoteKind::Hold,
            start_time: start,
            end_time: Beat::new(5, 1, 4)?,
            x: -75.,
            ..Note::default()
        };
        app.editor.execute(Command::AddNote { line: 0, note })?;
        ensure!(
            app.editor.document().note_count() == initial_notes + 1,
            "Add note failed"
        );
        app.editor.undo();
        ensure!(
            app.editor.document().note_count() == initial_notes,
            "Undo failed"
        );
        app.editor.redo();
        ensure!(
            app.editor.document().note_count() == initial_notes + 1,
            "Redo failed"
        );
        app.select(Some(phistudio_core::NoteId {
            line: 0,
            index: initial_line0,
        }));
        app.transport.beat = 3.5;
    }
    if frame == 7 {
        ensure!(
            app.render_error.is_none(),
            "prpr reload failed: {:?}",
            app.render_error
        );
        ensure!(
            preview.note_count() == initial_notes + 1,
            "Edited document did not reach prpr"
        );
        let path = directory.join("edited.rpe.json");
        save_chart(app.editor.document(), &path)?;
        ensure!(
            load_chart(&path)?.as_json() == app.editor.document().as_json(),
            "Save/reopen changed chart JSON"
        );
        unsafe { get_internal_gl() }.flush();
        get_screen_data().export_png(
            directory
                .join("workbench-after.png")
                .to_str()
                .context("Output path must be UTF-8")?,
        );
        std::fs::write(
            directory.join("project-options.json"),
            serde_json::to_vec_pretty(&app.project_options)?,
        )?;
        std::fs::write(
            directory.join("resource-warning.txt"),
            preview.resource_warning().unwrap_or(""),
        )?;
        std::fs::write(
            directory.join("smoke-test.txt"),
            format!(
                "PASS\nPhira revision: {}\nInput: {} lines, {} notes\nAdd hold → undo → redo: {} → {} → {}\nprpr after reload: {} notes\nSave/reopen: JSON equal\nPreview: Phira autoplay/HUD; Phigros Official skin, hitsounds and effects by PigeonGames\n",
                phistudio_prpr::PHIRA_REVISION,
                initial_lines,
                initial_notes,
                initial_notes + 1,
                initial_notes,
                initial_notes + 1,
                preview.note_count()
            ),
        )?;
        app.preview.floating = true;
        app.windows.floating_properties = true;
    }
    if frame == 11 {
        unsafe { get_internal_gl() }.flush();
        get_screen_data().export_png(
            directory
                .join("floating-windows.png")
                .to_str()
                .context("Output path must be UTF-8")?,
        );
        println!("Smoke test PASS: {}", directory.display());
        return Ok(true);
    }

    Ok(false)
}
