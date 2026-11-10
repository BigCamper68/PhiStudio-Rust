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
    for language in [Language::English, Language::Russian] {
        for (name, status) in [
            (
                "available",
                Status::Available(Release {
                    version: semver::Version::new(99, 0, 0),
                    url: format!("{}/tag/v99.0.0", service::RELEASES),
                }),
            ),
            (
                "current",
                Status::Current(Release {
                    version: semver::Version::parse(service::installed_version())?,
                    url: service::RELEASES.into(),
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
                            egui::ScrollArea::vertical().touch_show_y(ui, |ui| app.updates_ui(ui));
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
