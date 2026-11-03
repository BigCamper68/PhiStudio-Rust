//! Cached descriptions are painted on the UI thread; callbacks stay on the worker.
use super::*;
use phistudio_sdk::{DrawLayer, RenderFrame, RenderKind, RenderTarget, Viewport};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Key {
    pub(super) plugin: String,
    pub(super) target: RenderTarget,
    pub(super) instance: String,
}
pub(super) struct Ticket {
    key: Key,
    origin: Origin,
    frame: RenderFrame,
    version: DocumentVersion,
    line: usize,
    requested: Instant,
}
impl Ticket {
    pub(super) fn current(&self, runtime: Option<&Runtime>) -> bool {
        runtime.is_some_and(|r| r.is_current(&self.origin))
    }
}
pub(super) struct Cache {
    pub(super) origin: Origin,
    pub(super) required: Vec<Origin>,
    pub(super) viewport: Viewport,
    pub(super) version: DocumentVersion,
    pub(super) line: usize,
    layers: Vec<DrawLayer>,
    pub(super) passes: Vec<phistudio_sdk::PreviewPass>,
    pub(super) created: Instant,
}
struct View {
    origin: Origin,
    viewport: Viewport,
    context: EditorContext,
    seen: Instant,
    submitted: Option<Instant>,
    pending: Option<u64>,
    cache: Option<Cache>,
}
pub(super) struct TrackWindow {
    pub origin: Origin,
    pub open: bool,
}
#[derive(Default)]
pub(super) struct Rendering {
    views: BTreeMap<Key, View>,
    jobs: BTreeMap<u64, Ticket>,
    sequence: u64,
    pub tracks: BTreeMap<(String, String), TrackWindow>,
}
impl Rendering {
    pub(super) fn touch_effect_view(
        &mut self,
        key: Key,
        origin: Origin,
        viewport: Viewport,
        context: EditorContext,
        now: Instant,
    ) {
        let view = self.views.entry(key).or_insert_with(|| View {
            origin: origin.clone(),
            viewport: viewport.clone(),
            context: context.clone(),
            seen: now,
            submitted: None,
            pending: None,
            cache: None,
        });
        if view.origin != origin {
            view.cache = None;
            view.pending = None;
            view.submitted = None;
        }
        view.origin = origin;
        view.viewport = viewport;
        view.context = context;
        view.seen = now;
    }
    pub(super) fn retain_effect_views(&mut self, desired: &BTreeSet<Key>) {
        self.views
            .retain(|key, _| key.target.kind != RenderKind::PreviewEffect || desired.contains(key));
    }
    pub(super) fn effect_cache(&self, key: &Key) -> Option<&Cache> {
        self.views.get(key)?.cache.as_ref()
    }
    pub(super) fn complete(&mut self, job: u64) -> Option<Ticket> {
        let ticket = self.jobs.remove(&job)?;
        if let Some(view) = self.views.get_mut(&ticket.key)
            && view.pending == Some(job)
        {
            view.pending = None;
        }
        Some(ticket)
    }
    pub(super) fn retain(&mut self, runtime: &Runtime) {
        self.views.retain(|_, v| {
            runtime.is_current(&v.origin) && v.seen.elapsed() < Duration::from_secs(1)
        });
        self.tracks.retain(|_, v| runtime.is_current(&v.origin));
    }
    pub(super) fn apply(&mut self, ticket: &Ticket, owned: OwnedEffect) {
        let (token, layers, passes) = match owned.effect {
            Effect::Draw { frame, layers }
                if ticket.frame.target.kind != RenderKind::PreviewEffect =>
            {
                (frame, layers, vec![])
            }
            Effect::PreviewEffects { frame, passes }
                if ticket.frame.target.kind == RenderKind::PreviewEffect =>
            {
                (frame, vec![], passes)
            }
            _ => return,
        };
        let origin = owned.origin;
        let Some(view) = self.views.get_mut(&ticket.key) else {
            return;
        };
        if token != ticket.frame.token
            || ticket.requested.elapsed() > Duration::from_millis(500)
            || origin != ticket.origin
            || origin != view.origin
            || view.context.version != ticket.version
            || view.context.line != ticket.line
            || !same_layout(&view.viewport, &ticket.frame.viewport)
        {
            return;
        }
        view.cache = Some(Cache {
            origin,
            required: owned.required,
            viewport: ticket.frame.viewport.clone(),
            version: ticket.version,
            line: ticket.line,
            layers,
            passes,
            created: ticket.requested,
        });
    }
}
fn same_layout(a: &Viewport, b: &Viewport) -> bool {
    if a.size != b.size {
        return false;
    }
    match (&a.timeline, &b.timeline) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.pixels_per_beat == b.pixels_per_beat
                && a.baseline == b.baseline
                && a.x_min == b.x_min
                && a.x_max == b.x_max
                && a.division == b.division
                && a.lanes == b.lanes
        }
        _ => false,
    }
}
impl Workbench {
    pub(super) fn pump_plugin_renders(&mut self) {
        if self.plugins.busy() || self.plugins.pending.len() >= 8 {
            return;
        }
        let Some(runtime) = &self.plugins.runtime else {
            return;
        };
        let version = self.plugin_version();
        let mut ready: Vec<_> = self
            .plugins
            .rendering
            .views
            .iter()
            .filter(|(_, v)| {
                v.pending.is_none()
                    && v.context.version == version
                    && v.seen.elapsed() < Duration::from_millis(200)
                    && v.submitted.is_none_or(|t| {
                        t.elapsed()
                            >= Duration::from_millis(if v.context.playing { 33 } else { 100 })
                    })
                    && runtime.is_current(&v.origin)
            })
            .map(|(key, v)| (v.submitted, key.clone()))
            .collect();
        // The oldest request wins, including views never serviced before.
        ready.sort();
        for (_, key) in ready.into_iter().take(4) {
            if self.plugins.pending.len() >= 8 {
                break;
            }
            let Some(view) = self.plugins.rendering.views.get(&key) else {
                continue;
            };
            let Some(token) = self.plugins.rendering.sequence.checked_add(1) else {
                self.error = Some("Plugin render sequence exhausted".into());
                break;
            };
            self.plugins.rendering.sequence = token;
            let frame = RenderFrame {
                token,
                instance: key.instance.clone(),
                target: key.target.clone(),
                viewport: view.viewport.clone(),
            };
            let context = view.context.clone();
            let ticket = Ticket {
                key: key.clone(),
                origin: view.origin.clone(),
                version: context.version,
                line: context.line,
                frame: frame.clone(),
                requested: Instant::now(),
            };
            match self.plugins.submit(
                Action::Render {
                    plugin: key.plugin.clone(),
                    frame,
                    context,
                },
                Some(ticket.version),
            ) {
                Ok(job) => {
                    self.plugins.background.insert(job);
                    if let Some(view) = self.plugins.rendering.views.get_mut(&key) {
                        view.pending = Some(job);
                        view.submitted = Some(Instant::now());
                    }
                    self.plugins.rendering.jobs.insert(job, ticket);
                }
                Err(error) => {
                    self.error = Some(error);
                    break;
                }
            }
        }
    }
    pub(super) fn paint_plugin_view(
        &mut self,
        painter: &egui::Painter,
        rect: egui::Rect,
        origin: Origin,
        target: RenderTarget,
        instance: &str,
        viewport: Viewport,
    ) {
        if !rect.is_positive() || !rect.intersect(painter.clip_rect()).is_positive() {
            return;
        }
        let context = self.plugin_metadata_context();
        let key = Key {
            plugin: origin.plugin.clone(),
            target,
            instance: instance.into(),
        };
        let view = self
            .plugins
            .rendering
            .views
            .entry(key)
            .or_insert_with(|| View {
                origin: origin.clone(),
                viewport: viewport.clone(),
                context: context.clone(),
                seen: Instant::now(),
                submitted: None,
                pending: None,
                cache: None,
            });
        if view.origin != origin {
            view.origin = origin;
            view.cache = None;
            view.pending = None;
            view.submitted = None;
        }
        view.viewport = viewport.clone();
        view.context = context;
        view.seen = Instant::now();
        let fresh = view.cache.as_ref().filter(|c| {
            c.created.elapsed() < Duration::from_millis(500)
                && c.version == view.context.version
                && c.line == view.context.line
                && same_layout(&c.viewport, &viewport)
        });
        if let Some(cache) = fresh
            && self.plugins.runtime.as_ref().is_some_and(|r| {
                r.is_current(&cache.origin) && cache.required.iter().all(|p| r.is_current(p))
            })
        {
            let images = self
                .plugins
                .runtime
                .as_ref()
                .map(|runtime| {
                    self.plugins
                        .images
                        .layers(painter.ctx(), runtime, &cache.origin, &cache.layers)
                })
                .unwrap_or_default();
            super::drawing::paint(painter, rect, &viewport, &cache.layers, &images);
        }
        painter
            .ctx()
            .request_repaint_after(Duration::from_millis(if view.context.playing {
                33
            } else {
                100
            }));
    }
    pub(crate) fn plugin_preview_overlay(
        &mut self,
        painter: &egui::Painter,
        rect: egui::Rect,
        instance: &str,
    ) {
        let contributions: Vec<_> = self
            .plugins
            .registry
            .plugins
            .iter()
            .flat_map(|p| {
                p.contributions.iter().filter_map(move |c| match c {
                    Contribution::Preview { id } => Some((p.origin.clone()?, id.clone())),
                    _ => None,
                })
            })
            .collect();
        for (origin, id) in contributions {
            if self
                .plugins
                .runtime
                .as_ref()
                .is_none_or(|r| !r.is_current(&origin))
            {
                continue;
            }
            self.paint_plugin_view(
                painter,
                rect,
                origin,
                RenderTarget {
                    kind: RenderKind::Preview,
                    id,
                },
                instance,
                Viewport {
                    size: [rect.width(), rect.height()],
                    timeline: None,
                },
            );
        }
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
