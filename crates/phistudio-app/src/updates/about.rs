//! Bundled project identity. JPEG decoding happens once on a worker.
use super::*;
use crate::{
    i18n::{format, tr},
    workbench::Workbench,
};
use std::sync::mpsc::{self, Receiver};

#[derive(Default)]
pub(super) struct Identity {
    logo: Option<egui::TextureHandle>,
    avatar: Option<egui::TextureHandle>,
    loading: Option<Receiver<Result<egui::ColorImage, String>>>,
    started: bool,
}
impl Identity {
    fn load(&mut self, ctx: &egui::Context) {
        if self.logo.is_none() {
            let pixels = include_bytes!("../../../../assets/icons/phistudio-64.rgba");
            self.logo = Some(ctx.load_texture(
                "about-project-logo",
                egui::ColorImage::from_rgba_unmultiplied([64, 64], pixels),
                egui::TextureOptions::LINEAR,
            ));
        }
        if !self.started {
            self.started = true;
            let (sender, receiver) = mpsc::channel();
            self.loading = Some(receiver);
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = sender.send(decode_avatar());
                ctx.request_repaint();
            });
        }
        if let Some(receiver) = &self.loading {
            match receiver.try_recv() {
                Ok(Ok(image)) => {
                    self.avatar = Some(ctx.load_texture(
                        "about-author-avatar",
                        image,
                        egui::TextureOptions::LINEAR,
                    ));
                    self.loading = None;
                }
                Ok(Err(error)) => {
                    eprintln!("Could not load author avatar: {error}");
                    self.loading = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.loading = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }
}
pub(super) fn decode_avatar() -> Result<egui::ColorImage, String> {
    let bytes = include_bytes!("../../../../assets/about/bigcamper68.jpg");
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| e.to_string())?
        .resize(128, 128, image::imageops::FilterType::Triangle)
        .to_rgba8();
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}
fn card() -> egui::Frame {
    egui::Frame::none()
        .fill(crate::theme::raised())
        .rounding(8.)
        .inner_margin(12.)
}
impl Workbench {
    pub(crate) fn about_ui(&mut self, ui: &mut egui::Ui) {
        // Other settings can make their horizontal scroll content wider than the viewport.
        let width = ui.available_width().min(ui.clip_rect().width()).max(1.);
        ui.vertical(|ui| {
            ui.set_width(width);
            self.about_contents(ui);
        });
    }
    fn about_contents(&mut self, ui: &mut egui::Ui) {
        self.updates.identity.load(ui.ctx());
        if self.managed_storage {
            self.updates.notes.ensure_metadata();
        }
        ui.strong(tr("About the project"));
        ui.add_space(6.);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.style_mut().wrap = Some(true);
            ui.visuals_mut().widgets.inactive.bg_stroke =
                egui::Stroke::new(1_f32, crate::theme::border());
            ui.horizontal(|ui| {
                if let Some(logo) = &self.updates.identity.logo {
                    ui.image(logo.id(), egui::vec2(56., 56.));
                } else {
                    ui.allocate_exact_size(egui::vec2(56., 56.), egui::Sense::hover());
                }
                ui.vertical(|ui| {
                    ui.strong("PhiStudio Rust");
                    ui.label(format("Version {0}", &[service::installed_build().into()]));
                    if let Some(release) = self.updates.notes.release()
                        && release.name != release.version.to_string()
                        && release.name != format!("v{}", release.version)
                    {
                        ui.small(&release.name);
                    }
                    ui.hyperlink_to(tr("Project on GitHub"), service::PROJECT);
                });
            });
            ui.add_space(8.);
            self.updates_ui(ui);
            if ui.button(tr("Release notes")).clicked() {
                self.updates.notes.request(true, false);
            }
        });
        ui.add_space(8.);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.style_mut().wrap = Some(true);
            ui.horizontal(|ui| {
                if let Some(avatar) = &self.updates.identity.avatar {
                    ui.image(avatar.id(), egui::vec2(56., 56.));
                } else {
                    ui.allocate_exact_size(egui::vec2(56., 56.), egui::Sense::hover());
                }
                ui.vertical(|ui| {
                    ui.hyperlink_to("BigCamper68", "https://github.com/BigCamper68");
                    ui.label(tr("Author of PhiStudio Rust"));
                });
            });
        });
    }
}
