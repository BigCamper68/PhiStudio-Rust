//! Decode visible project covers on one worker; texture uploads stay on the UI thread.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
};
struct Entry {
    texture: Option<egui::TextureHandle>,
    used: u64,
}
pub(crate) struct Covers {
    sender: Sender<(PathBuf, egui::Context)>,
    receiver: Receiver<(PathBuf, Option<phistudio_prpr::CoverImage>)>,
    entries: HashMap<PathBuf, Entry>,
    frame: u64,
}
impl Covers {
    pub fn new() -> Self {
        let (sender, requests) = mpsc::channel::<(PathBuf, egui::Context)>();
        let (results, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok((path, ctx)) = requests.recv() {
                let image = phistudio_prpr::load_project_cover(&path).ok();
                if results.send((path, image)).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Self {
            sender,
            receiver,
            entries: HashMap::new(),
            frame: 0,
        }
    }
    pub fn update(&mut self, ctx: &egui::Context) {
        self.frame += 1;
        for (path, image) in self.receiver.try_iter() {
            if let Some(entry) = self.entries.get_mut(&path) {
                entry.texture = image.map(|image| {
                    ctx.load_texture(
                        path.to_string_lossy(),
                        egui::ColorImage::from_rgba_unmultiplied(image.size, &image.rgba),
                        egui::TextureOptions::LINEAR,
                    )
                });
            }
        }
        while self.entries.len() > 128 {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(path, _)| path.clone())
                .unwrap();
            self.entries.remove(&oldest);
        }
    }
    pub fn texture(&mut self, ctx: &egui::Context, path: &Path) -> Option<egui::TextureId> {
        let entry = self.entries.entry(path.to_owned()).or_insert_with(|| {
            let _ = self.sender.send((path.to_owned(), ctx.clone()));
            Entry {
                texture: None,
                used: self.frame,
            }
        });
        entry.used = self.frame;
        entry.texture.as_ref().map(egui::TextureHandle::id)
    }
}
