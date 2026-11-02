//! Native GL gallery and actual panel picking on multiple source lines.
use super::*;

pub(super) fn capture_screen() -> Result<Image> {
    let width = screen_width() as u16;
    let height = screen_height() as u16;
    let mut image = Image {
        width,
        height,
        bytes: vec![0; width as usize * height as usize * 4],
    };
    unsafe {
        use macroquad::miniquad::gl::*;
        let mut framebuffer = 0;
        glGetIntegerv(GL_FRAMEBUFFER_BINDING, &mut framebuffer);
        glBindFramebuffer(GL_FRAMEBUFFER, 0);
        glReadPixels(
            0,
            0,
            width as i32,
            height as i32,
            GL_RGBA,
            GL_UNSIGNED_BYTE,
            image.bytes.as_mut_ptr() as _,
        );
        let error = glGetError();
        ensure!(error == 0, "OpenGL screen read failed: {error}");
        glBindFramebuffer(GL_FRAMEBUFFER, framebuffer as u32);
    }
    Ok(image)
}

pub(super) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &std::path::Path,
) -> Result<()> {
    let mut raw = app.editor.document().as_json().clone();
    let line = raw["judgeLineList"][0].clone();
    // More rows than fit at once exercise the enlarged scrollable line list.
    raw["judgeLineList"] = serde_json::Value::Array(vec![line; 16]);
    app.replace_document(ChartDocument::from_value(raw.clone())?, None, None);
    preview
        .replace_chart(app.editor.document(), None, &app.project_options)
        .await?;
    app.multi_line.enabled = true;
    app.multi_line.lines = vec![0, 1, 2];
    app.multi_line.widths = [180., 230.];
    for line in [0, 1, 2] {
        app.execute(phistudio_core::Command::EditStoryboard {
            line,
            kind: phistudio_core::StoryKind::ScaleX,
            index: None,
            event: Some(phistudio_core::StoryEvent::new(
                phistudio_core::StoryKind::ScaleX,
                phistudio_core::Beat::new(1, 0, 1)?,
                phistudio_core::Beat::new(3, 0, 1)?,
            )),
        });
    }
    raw = app.editor.document().as_json().clone();
    preview
        .replace_chart(app.editor.document(), None, &app.project_options)
        .await?;
    let mut cases = vec![];
    for (events, merged, label) in [
        (false, false, "columns"),
        (false, true, "overlay"),
        (true, false, "events"),
        (true, false, "storyboard"),
    ] {
        app.advanced.right_mode = if label == "storyboard" {
            RightMode::Storyboard
        } else {
            RightMode::Events
        };
        app.multi_line.events = events;
        app.multi_line.merged = merged;
        for offset in [0., 160.] {
            app.multi_line.offsets = [offset; 2];
            for language in [Language::English, Language::Russian] {
                app.options.language = language;
                for _ in 0..4 {
                    paint(gui, app, preview, 210.)?;
                    next_frame().await;
                }
                let grid = crate::grid_input::geometry(app.grid_area, app);
                let panels = app.line_panels(grid, events);
                ensure!(!panels.is_empty());
                let name = format!("{label}-{}-{offset}", language.code());
                paint(gui, app, preview, 210.)?;
                let frame = capture_screen()?;
                ensure!(
                    frame
                        .bytes
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .filter(|p| p[..3] != [0, 0, 0])
                        .count()
                        > 1000,
                    "Native GUI capture is blank"
                );
                frame.export_png(out.join(format!("{name}.png")).to_str().unwrap());
                cases.push(serde_json::json!({"case":name,"visible_lines":panels.iter().map(|(line,_)|*line).collect::<Vec<_>>()}));
                next_frame().await;
            }
        }
    }
    app.advanced.right_mode = RightMode::Events;
    app.multi_line.events = false;
    app.multi_line.merged = false;
    // A compact grid may fit only one column. Bring the second source line
    // into view before checking its native pointer picking.
    app.multi_line.offsets = [app.multi_line.widths[0] + 8., 0.];
    app.beat = app.editor.document().note(1, 0)?.start_time.as_f64();
    app.set_tool(crate::workbench::Tool::Select);
    for _ in 0..3 {
        paint(gui, app, preview, 210.)?;
        next_frame().await;
    }
    let grid = crate::grid_input::geometry(app.grid_area, app);
    let Some((_, panel)) = app
        .line_panels(grid, false)
        .into_iter()
        .find(|(line, _)| *line == 1)
    else {
        anyhow::bail!("Second source line is outside the picking viewport");
    };
    let note = app.editor.document().note(1, 0)?;
    let point = egui::pos2(panel.x(note.x), panel.y(note.start_time.as_f64()));
    gui.probe_input(
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
        false,
    );
    paint(gui, app, preview, 210.)?;
    next_frame().await;
    gui.probe_input(
        vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
        false,
    );
    paint(gui, app, preview, 210.)?;
    next_frame().await;
    ensure!(
        app.selected.is_some_and(|id| id.line == 1),
        "Second column picked another source line: {:?}",
        app.selected
    );
    app.multi_line.show = true;
    for _ in 0..4 {
        paint(gui, app, preview, 210.)?;
        next_frame().await;
    }
    paint(gui, app, preview, 210.)?;
    capture_screen()?.export_png(out.join("controls.png").to_str().unwrap());
    app.audio_guide.show = true;
    for _ in 0..4 {
        paint(gui, app, preview, 210.)?;
        next_frame().await;
    }
    let point = egui::pos2(80., 20.);
    for pressed in [true, false] {
        gui.probe_input(
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
            false,
        );
        paint(gui, app, preview, 210.)?;
        next_frame().await;
    }
    for _ in 0..3 {
        paint(gui, app, preview, 210.)?;
        next_frame().await;
    }
    paint(gui, app, preview, 210.)?;
    capture_screen()?.export_png(out.join("windows.png").to_str().unwrap());
    ensure!(
        app.editor.document().as_json() == &raw,
        "View/picking changed chart"
    );
    std::fs::write(
        out.join("multiline.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"cases":cases,"picked_line":1,"chart_unchanged":true}),
        )?,
    )?;
    println!(
        "PASS: multiple-line columns/overlay/events/storyboard, scrolling and source-line picking in native GPU frames"
    );
    Ok(())
}
