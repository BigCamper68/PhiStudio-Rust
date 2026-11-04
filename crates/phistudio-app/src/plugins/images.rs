//! The worker decodes pixels; this cache owns only UI graphics resources.
use phistudio_plugins::{Origin, Runtime};
use phistudio_sdk::{DrawLayer, DrawPrimitive, Widget};
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
mod tests;

pub(super) type Textures = BTreeMap<u64, egui::TextureId>;
#[derive(Default)]
pub(super) struct Images {
    textures: BTreeMap<(String, u64, u64), egui::TextureHandle>,
    frame: f64,
    uploads: usize,
}
impl Images {
    pub(super) fn retain(&mut self, runtime: &Runtime) {
        self.textures.retain(|(plugin, generation, file), _| {
            runtime
                .image(
                    &Origin {
                        plugin: plugin.clone(),
                        generation: *generation,
                    },
                    *file,
                )
                .is_some()
        });
    }
    pub(super) fn layers(
        &mut self,
        ctx: &egui::Context,
        runtime: &Runtime,
        owner: &Origin,
        layers: &[DrawLayer],
    ) -> Textures {
        let files = layers
            .iter()
            .flat_map(|l| &l.primitives)
            .filter_map(|p| {
                if let DrawPrimitive::Image { file, .. } = p {
                    Some(*file)
                } else {
                    None
                }
            })
            .collect();
        self.prepare(ctx, runtime, owner, files)
    }
    pub(super) fn widgets(
        &mut self,
        ctx: &egui::Context,
        runtime: &Runtime,
        owner: &Origin,
        widgets: &[Widget],
    ) -> Textures {
        fn collect(widgets: &[Widget], files: &mut BTreeSet<u64>) {
            for widget in widgets {
                match widget {
                    Widget::Row { children } | Widget::Column { children } => {
                        collect(children, files)
                    }
                    Widget::Tabs { tabs, selected, .. } => {
                        if let Some(tab) = tabs.iter().find(|t| &t.id == selected) {
                            collect(&tab.children, files);
                        }
                    }
                    Widget::Canvas { layers, .. } => {
                        files.extend(layers.iter().flat_map(|l| &l.primitives).filter_map(|p| {
                            if let DrawPrimitive::Image { file, .. } = p {
                                Some(*file)
                            } else {
                                None
                            }
                        }));
                    }
                    _ => {}
                }
            }
        }
        let mut files = BTreeSet::new();
        collect(widgets, &mut files);
        self.prepare(ctx, runtime, owner, files)
    }
    fn prepare(
        &mut self,
        ctx: &egui::Context,
        runtime: &Runtime,
        owner: &Origin,
        files: BTreeSet<u64>,
    ) -> Textures {
        let frame = ctx.input(|i| i.time);
        if frame != self.frame {
            self.frame = frame;
            self.uploads = 0;
        }
        let mut textures = Textures::new();
        for file in files {
            let Some(image) = runtime.image(owner, file) else {
                continue;
            };
            let key = (owner.plugin.clone(), owner.generation, file);
            if !self.textures.contains_key(&key) {
                if self.uploads >= 2 {
                    ctx.request_repaint();
                    continue;
                }
                let texture = ctx.load_texture(
                    format!("plugin-image-{}-{}-{file}", owner.plugin, owner.generation),
                    egui::ColorImage::from_rgba_unmultiplied(image.size(), image.rgba()),
                    egui::TextureOptions::LINEAR,
                );
                self.textures.insert(key.clone(), texture);
                self.uploads += 1;
            }
            if let Some(texture) = self.textures.get(&key) {
                textures.insert(file, texture.id());
            }
        }
        textures
    }
}
