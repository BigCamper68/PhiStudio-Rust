use super::rendering::TrackWindow;
use super::*;
use phistudio_sdk::{RenderKind, RenderTarget, TimelineViewport, Viewport};

impl Workbench {
    pub(super) fn plugin_track_windows(&mut self, ctx: &egui::Context) {
        let tracks: Vec<_> = self
            .plugins
            .registry
            .plugins
            .iter()
            .flat_map(|p| {
                p.contributions.iter().filter_map(move |c| match c {
                    Contribution::Track { id, title } => {
                        Some((p.origin.clone()?, id.clone(), title.clone()))
                    }
                    _ => None,
                })
            })
            .collect();
        for (origin, id, title) in tracks {
            if self
                .plugins
                .runtime
                .as_ref()
                .is_none_or(|r| !r.is_current(&origin))
            {
                continue;
            }
            let key = (origin.plugin.clone(), id.clone());
            let state = self
                .plugins
                .rendering
                .tracks
                .entry(key.clone())
                .or_insert_with(|| TrackWindow {
                    origin: origin.clone(),
                    open: false,
                });
            if state.origin != origin {
                state.origin = origin.clone();
                state.open = false;
            }
            if !state.open {
                continue;
            }
            let mut open = true;
            crate::i18n::window(&title)
                .id(egui::Id::new(("plugin-track", &key)))
                .open(&mut open)
                .default_width(320.)
                .default_height(240.)
                .default_pos([(ctx.screen_rect().right() - 356.).max(16.), 80.])
                .resize(|r| {
                    r.max_size([
                        (ctx.screen_rect().width() - 32.).max(180.),
                        (ctx.screen_rect().height() - 100.).max(100.),
                    ])
                })
                .show(ctx, |ui| {
                    let size =
                        egui::vec2(ui.available_width().max(1.), ui.available_height().max(48.));
                    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                    ui.painter().rect_filled(rect, 0., crate::theme::base());
                    let timeline = TimelineViewport {
                        beat: self.grid_beat(),
                        pixels_per_beat: self.timeline.zoom,
                        baseline: rect.height() - 24.,
                        x_min: self.timeline.camera_x - 675.,
                        x_max: self.timeline.camera_x + 675.,
                        division: if self.options.snap_time {
                            self.timeline.division
                        } else {
                            1920
                        },
                        lanes: self.timeline.lanes,
                    };
                    super::drawing::paint_clock(ui.painter(), rect, &timeline);
                    let viewport = Viewport {
                        size: [rect.width(), rect.height()],
                        timeline: Some(timeline),
                    };
                    self.plugin_track_input(ui, &origin, &id, rect, viewport.clone());
                    self.paint_plugin_view(
                        ui.painter(),
                        rect,
                        origin.clone(),
                        RenderTarget {
                            kind: RenderKind::Track,
                            id: id.clone(),
                        },
                        "track",
                        viewport,
                    );
                });
            if let Some(state) = self.plugins.rendering.tracks.get_mut(&key) {
                state.open = open;
            }
            if !open {
                self.cancel_plugin_track_input(&origin, &id);
            }
        }
    }
}
