//! Paint the actual localized sound settings with the shared GPU adapter.
use super::*;
use crate::scroll::TouchScroll as _;
use std::path::Path;

pub(super) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &Path,
) -> Result<()> {
    let original = app.editor.document().as_json().clone();
    let mut sounds = crate::editor_sounds::Engine::new(preview.create_interface_audio()?);
    for language in [Language::English, Language::Russian] {
        app.options.language = language;
        let mut content_height = 0.;
        let mut horizontal_overflow = 0.;
        for frame in 0..10 {
            if frame == 2 {
                let pos = egui::pos2(80., 24.);
                let mut events = vec![egui::Event::PointerMoved(pos)];
                for pressed in [true, false] {
                    events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    });
                }
                gui.probe_input(events, false);
            }
            set_default_camera();
            clear_background(BLACK);
            gui.run(|ctx, _| {
                let _language = crate::i18n::Scope::enter(language);
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.style_mut().wrap = Some(true);
                    let width = ui.available_width();
                    let output = egui::ScrollArea::vertical().touch_show_y(ui, |ui| {
                        app.editor_sounds_ui(ui);
                    });
                    content_height = output.content_size.y;
                    horizontal_overflow = output.content_size.x - width;
                });
            });
            sounds.update(app);
            preview.render(0., preview.target_viewport());
            gui.draw();
            unsafe { macroquad::miniquad::gl::glFinish() };
            if frame < 9 {
                next_frame().await;
            }
        }
        ensure!(
            content_height > 300.,
            "Sound settings did not expand after a real button click"
        );
        ensure!(
            horizontal_overflow <= 1.,
            "Sound controls overflow the viewport by {horizontal_overflow} points"
        );
        let image = get_screen_data();
        ensure!(
            image
                .bytes
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[0] > 20 || p[1] > 20 || p[2] > 20)
                .count()
                > 1000,
            "Sound settings screenshot is blank"
        );
        image.export_png(
            out.join(format!("editor-sounds-{}.png", language.code()))
                .to_str()
                .unwrap(),
        );
        next_frame().await;
    }
    ensure!(
        app.editor.document().as_json() == &original,
        "Sound settings modified the chart"
    );
    std::fs::write(
        out.join("result.txt"),
        "PASS: localized editor sound settings rendered without modifying chart data\n",
    )?;
    Ok(())
}
