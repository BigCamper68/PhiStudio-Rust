//! Paint compact settings and the actual grouped sound manager on the GPU.
use super::*;
use crate::scroll::TouchScroll as _;
use phistudio_core::{EditorSoundEvent, EditorSoundPack};
use std::path::Path;

pub(super) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &Path,
) -> Result<()> {
    let original = app.editor.document().as_json().clone();
    let revision = app.editor.revision();
    let mut sounds = crate::editor_sounds::Engine::new(preview.create_interface_audio()?);
    let mut pack = EditorSoundPack {
        name: "Studio pack".into(),
        ..Default::default()
    };
    for index in 0..2 {
        let path = out.join(format!("variant-{index}.wav"));
        let samples = (0..7200)
            .map(|i| {
                let t = i as f32 / 48000.;
                ((t * (440. + index as f32 * 220.) * std::f32::consts::TAU).sin()
                    * (1. - t / 0.15)
                    * 0.7
                    * i16::MAX as f32) as i16
            })
            .collect::<Vec<_>>();
        let size = (samples.len() * 2) as u32;
        let mut bytes = Vec::new();
        bytes.extend(b"RIFF");
        bytes.extend((36 + size).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16_u32.to_le_bytes());
        bytes.extend(1_u16.to_le_bytes());
        bytes.extend(1_u16.to_le_bytes());
        bytes.extend(48000_u32.to_le_bytes());
        bytes.extend(96000_u32.to_le_bytes());
        bytes.extend(2_u16.to_le_bytes());
        bytes.extend(16_u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(size.to_le_bytes());
        for sample in samples {
            bytes.extend(sample.to_le_bytes());
        }
        std::fs::write(&path, bytes)?;
        pack.events
            .entry(EditorSoundEvent::MoveResize)
            .or_default()
            .files
            .push(path.to_string_lossy().into_owned());
    }
    app.options.editor_sounds.add_pack(pack);
    app.sounds.select_for_probe(EditorSoundEvent::MoveResize);
    for language in [Language::English, Language::Russian] {
        app.options.language = language;
        let mut manager_button = None;
        let mut horizontal_overflow = 0.;
        for frame in 0..12 {
            if frame == 2 {
                let pos = egui::pos2(80., 24.);
                gui.probe_input(
                    vec![
                        egui::Event::PointerMoved(pos),
                        button(pos, true),
                        button(pos, false),
                    ],
                    false,
                );
            }
            set_default_camera();
            clear_background(BLACK);
            gui.run(|ctx, _| {
                let _language = crate::i18n::Scope::enter(language);
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.style_mut().wrap = Some(true);
                    let width = ui.available_width();
                    let area = egui::ScrollArea::vertical()
                        .touch_show_y(ui, |ui| app.editor_sounds_ui(ui));
                    horizontal_overflow = area.content_size.x - width;
                });
            });
            sounds.update(app);
            preview.render(0., preview.target_viewport());
            gui.draw();
            unsafe { macroquad::miniquad::gl::glFinish() };
            if frame < 11 {
                next_frame().await;
            }
        }
        ensure!(
            horizontal_overflow <= 1.,
            "Compact sound settings overflow by {horizontal_overflow}"
        );
        get_screen_data().export_png(
            out.join(format!("editor-sounds-{}.png", language.code()))
                .to_str()
                .unwrap(),
        );
        next_frame().await;
        // Use the same manager entry point as Settings.
        app.sounds.open = true;
        for frame in 0..12 {
            set_default_camera();
            clear_background(BLACK);
            gui.run(|ctx, _| {
                let _language = crate::i18n::Scope::enter(language);
                app.editor_sounds_window(ctx);
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label("PhiStudio Rust");
                });
                manager_button =
                    crate::window::visible_rect(ctx, egui::Id::new("Editor Sounds Manager"));
            });
            sounds.update(app);
            preview.render(0., preview.target_viewport());
            gui.draw();
            unsafe { macroquad::miniquad::gl::glFinish() };
            if frame < 11 {
                next_frame().await;
            }
        }
        let rect =
            manager_button.ok_or_else(|| anyhow::anyhow!("Sound manager was not rendered"))?;
        ensure!(
            rect.right() <= screen_width() + 1. && rect.bottom() <= screen_height() + 1.,
            "Sound manager exceeds the viewport: {rect:?}"
        );
        get_screen_data().export_png(
            out.join(format!("sound-manager-{}.png", language.code()))
                .to_str()
                .unwrap(),
        );
        for frame in 0..5 {
            set_default_camera();
            clear_background(BLACK);
            gui.run(|ctx, _| {
                let _language = crate::i18n::Scope::enter(language);
                crate::scroll::begin_frame(ctx);
                app.editor_sounds_window(ctx);
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label("PhiStudio Rust");
                });
                let zones = ctx
                    .data_mut(|d| {
                        d.get_temp::<Vec<crate::scroll::Zone>>(egui::Id::new(
                            "phistudio-scroll-zones",
                        ))
                    })
                    .unwrap_or_default();
                if let Some(zone) = zones
                    .iter()
                    .filter(|z| z.axis == 1)
                    .max_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))
                    && let Some(mut state) = egui::scroll_area::State::load(ctx, zone.id)
                {
                    state.offset.y = zone.max_offset;
                    state.store(ctx, zone.id);
                }
            });
            sounds.update(app);
            preview.render(0., preview.target_viewport());
            gui.draw();
            unsafe { macroquad::miniquad::gl::glFinish() };
            if frame < 4 {
                next_frame().await;
            }
        }
        get_screen_data().export_png(
            out.join(format!("sound-manager-{}-variants.png", language.code()))
                .to_str()
                .unwrap(),
        );
        app.sounds.open = false;
        next_frame().await;
    }
    ensure!(
        app.editor.document().as_json() == &original && app.editor.revision() == revision,
        "Sound settings changed chart history"
    );
    std::fs::write(
        out.join("result.txt"),
        "PASS: compact settings, grouped sound manager, decoded waveforms and unchanged chart history\n",
    )?;
    Ok(())
}
fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}
