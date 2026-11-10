//! Actual GPU settings and notification frames without public network requests.
use super::*;
use crate::{gui::Gui, scroll::TouchScroll as _, workbench::Workbench};
use anyhow::{Result, ensure};
use macroquad::prelude::*;
use phistudio_core::Language;
use std::path::Path;

pub(crate) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut phistudio_prpr::Preview,
    out: &Path,
) -> Result<()> {
    let original = app.editor.document().as_json().clone();
    let installed = Release {
        version: semver::Version::parse(service::installed_version())?,
        url: format!("{}/tag/v{}", service::RELEASES,service::installed_version()),
        name: service::installed_version().into(),
        body: "## Editor improvements\n- Added release notes from GitHub.\n- Added a project and author card.\n\nThis is a cached copy of the installed release notes.".into(),
    };
    app.updates.notes.content = notes::Content::Ready(installed.clone());
    for language in [Language::English, Language::Russian] {
        for (name, status) in [
            (
                "available",
                Status::Available(Release {
                    version: semver::Version::new(99, 0, 0),
                    url: format!("{}/tag/v99.0.0", service::RELEASES),
                    name: "99.0.0".into(),
                    body: "- Improved the editor.".into(),
                }),
            ),
            (
                "current",
                Status::Current(Release {
                    version: semver::Version::parse(service::installed_version())?,
                    url: service::RELEASES.into(),
                    name: service::installed_version().into(),
                    body: "- Improved the editor.".into(),
                }),
            ),
            ("offline", Status::Failed(CheckError::Connection)),
            ("checking", Status::Checking),
        ] {
            app.updates.status = status;
            for frame in 0..4 {
                set_default_camera();
                clear_background(BLACK);
                let mut overflow = 0.;
                gui.run(|ctx, _| {
                    let _language = crate::i18n::Scope::enter(language);
                    app.update_banner(ctx);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.style_mut().wrap = Some(true);
                        let width = ui.available_width();
                        let area =
                            egui::ScrollArea::vertical().touch_show_y(ui, |ui| app.about_ui(ui));
                        overflow = area.content_size.x - width;
                    });
                });
                ensure!(
                    overflow <= 1.,
                    "Update settings overflow by {overflow} points"
                );
                preview.render(0., preview.target_viewport());
                gui.draw();
                unsafe { macroquad::miniquad::gl::glFinish() };
                if frame < 3 {
                    next_frame().await;
                }
            }
            get_screen_data().export_png(
                out.join(format!("updates-{}-{name}.png", language.code()))
                    .to_str()
                    .unwrap(),
            );
            next_frame().await;
        }
    }
    for language in [Language::English, Language::Russian] {
        app.options.language = language;
        for (name, ready) in [("notes", true), ("notes-offline", false)] {
            app.updates.notes.content = if ready {
                notes::Content::Ready(installed.clone())
            } else {
                notes::Content::Failed(CheckError::Connection)
            };
            app.updates.notes.requested = true;
            for frame in 0..4 {
                set_default_camera();
                clear_background(BLACK);
                gui.run(|ctx, _| {
                    let _language = crate::i18n::Scope::enter(language);
                    app.release_notes_window(ctx);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.label("PhiStudio Rust");
                    });
                });
                preview.render(0., preview.target_viewport());
                gui.draw();
                unsafe { macroquad::miniquad::gl::glFinish() };
                if frame < 3 {
                    next_frame().await;
                }
            }
            get_screen_data().export_png(
                out.join(format!("{}-{name}.png", language.code()))
                    .to_str()
                    .unwrap(),
            );
            app.release_notes_back();
            next_frame().await;
        }
    }
    app.options.language = Language::Russian;
    app.updates.notes.content = notes::Content::Ready(installed.clone());
    app.updates.status = Status::Current(installed);
    app.windows.settings = true;
    for frame in 0..6 {
        set_default_camera();
        clear_background(BLACK);
        gui.run(|ctx, canceled| {
            app.ui(ctx, canceled);
            let window = crate::window::visible_rect(ctx, egui::Id::new("Settings"));
            if let Some(window) = window {
                let zones = ctx
                    .data_mut(|d| {
                        d.get_temp::<Vec<crate::scroll::Zone>>(egui::Id::new(
                            "phistudio-scroll-zones",
                        ))
                    })
                    .unwrap_or_default();
                if let Some(zone) = zones.into_iter().find(|z| {
                    z.axis == 1 && z.layer.order == egui::Order::Middle && window.intersects(z.rect)
                }) && let Some(mut scroll) = egui::scroll_area::State::load(ctx, zone.id)
                {
                    scroll.offset.y = zone.max_offset;
                    scroll.store(ctx, zone.id);
                }
            }
        });
        preview.render(0., preview.target_viewport());
        gui.draw();
        unsafe { macroquad::miniquad::gl::glFinish() };
        if frame < 5 {
            next_frame().await;
        }
    }
    get_screen_data().export_png(out.join("settings-about-footer.png").to_str().unwrap());
    ensure!(
        app.editor.document().as_json() == &original,
        "Update UI modified the chart"
    );
    std::fs::write(
        out.join("result.txt"),
        "PASS: localized update settings, notifications and error states fit without modifying chart data\n",
    )?;
    Ok(())
}
