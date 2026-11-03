use super::*;
use phistudio_prpr::{PreviewFilter, PreviewFilterFailure};
use phistudio_sdk::{MAX_ACTIVE_PREVIEW_PASSES, RenderKind, RenderTarget, Viewport};
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;

impl Workbench {
    fn plugin_effect_contributions(&self) -> Vec<(i32, Origin, String)> {
        if self.show_projects || self.render_error.is_some() || !self.options.preview_shaders {
            return vec![];
        }
        let mut contributions: Vec<_> = self
            .plugins
            .registry
            .plugins
            .iter()
            .flat_map(|p| {
                p.contributions.iter().filter_map(move |c| match c {
                    Contribution::PreviewEffect { id, priority } => {
                        Some((*priority, p.origin.clone()?, id.clone()))
                    }
                    _ => None,
                })
            })
            .collect();
        contributions.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.plugin.cmp(&b.1.plugin))
                .then(a.2.cmp(&b.2))
        });
        contributions
    }
    pub(super) fn maintain_plugin_effect_views(&mut self) {
        let context = self.plugin_metadata_context();
        let mut desired = BTreeSet::new();
        for (_, origin, id) in self.plugin_effect_contributions() {
            if self
                .plugins
                .runtime
                .as_ref()
                .is_none_or(|r| !r.is_current(&origin))
            {
                continue;
            }
            let key = super::rendering::Key {
                plugin: origin.plugin.clone(),
                target: RenderTarget {
                    kind: RenderKind::PreviewEffect,
                    id,
                },
                instance: "preview-effects".into(),
            };
            desired.insert(key.clone());
            let viewport = Viewport {
                size: self.preview.size.map(|v| v.max(1) as f32),
                timeline: None,
            };
            self.plugins.rendering.touch_effect_view(
                key,
                origin,
                viewport,
                context.clone(),
                Instant::now(),
            );
        }
        self.plugins.rendering.retain_effect_views(&desired);
    }
    /// These passes affect the shared prpr image, before egui overlays/UI.
    pub(crate) fn plugin_preview_effects(&self) -> Vec<PreviewFilter> {
        let mut filters = vec![];
        let Some(runtime) = &self.plugins.runtime else {
            return filters;
        };
        for (_, origin, id) in self.plugin_effect_contributions() {
            if !runtime.is_current(&origin) {
                continue;
            }
            let key = super::rendering::Key {
                plugin: origin.plugin.clone(),
                target: RenderTarget {
                    kind: RenderKind::PreviewEffect,
                    id: id.clone(),
                },
                instance: "preview-effects".into(),
            };
            let Some(cache) = self.plugins.rendering.effect_cache(&key) else {
                continue;
            };
            if cache.created.elapsed() >= Duration::from_millis(500)
                || cache.version != self.plugin_version()
                || cache.line != self.line
                || cache.origin != origin
                || cache.viewport.size != self.preview.size.map(|v| v.max(1) as f32)
                || cache.required.iter().any(|p| !runtime.is_current(p))
            {
                continue;
            }
            for pass in &cache.passes {
                if filters.len() >= MAX_ACTIVE_PREVIEW_PASSES {
                    return filters;
                }
                filters.push(PreviewFilter {
                    plugin: origin.plugin.clone(),
                    generation: origin.generation,
                    contribution: id.clone(),
                    pass: pass.clone(),
                });
            }
        }
        filters
    }
    pub(crate) fn plugin_preview_effect_failures(&mut self, failures: Vec<PreviewFilterFailure>) {
        self.plugins.effect_errors = failures
            .into_iter()
            .filter(|f| {
                self.plugins.runtime.as_ref().is_some_and(|r| {
                    r.is_current(&Origin {
                        plugin: f.key.plugin.clone(),
                        generation: f.key.generation,
                    })
                })
            })
            .map(|f| (f.key, f.message))
            .collect();
    }
}
