use super::rendering::TrackWindow;
use super::*;
use crate::scroll::TouchScroll as _;
use phistudio_sdk::{RenderKind, RenderTarget, TimelineViewport, Viewport};

impl Workbench {
    /// Reserve columns inside the native timeline. A track stays hidden until
    /// explicitly opened, including after Reload or a new activation generation.
    pub(crate) fn plugin_timeline(
        &mut self,
        ui: &mut egui::Ui,
        canceled: bool,
        resizing_sidebar: bool,
    ) {
        let frame = ui.available_rect_before_wrap();
        let tracks: Vec<_> = self
            .plugins
            .registry
            .plugins
            .iter()
            .flat_map(|p| {
                p.contributions.iter().filter_map(move |c| match c {
                    Contribution::TimelineTrack { id, title, width } => {
                        Some((p.origin.clone()?, id.clone(), title.clone(), *width))
                    }
                    _ => None,
                })
            })
            .filter(|(origin, id, _, _)| {
                self.plugins
                    .runtime
                    .as_ref()
                    .is_some_and(|r| r.is_current(origin))
                    && self
                        .plugins
                        .rendering
                        .tracks
                        .get(&(origin.plugin.clone(), id.clone()))
                        .is_some_and(|t| t.open && t.origin == *origin)
            })
            .collect();
        if tracks.is_empty() || frame.width() < 320. {
            crate::timeline::show(self, ui, canceled, resizing_sidebar);
            return;
        }
        let desired = tracks.iter().map(|t| t.3).sum::<f32>();
        let width = desired.min(frame.width() * 0.45);
        let main =
            egui::Rect::from_min_max(frame.min, egui::pos2(frame.right() - width, frame.bottom()));
        ui.allocate_ui_at_rect(main, |ui| {
            crate::timeline::show(self, ui, canceled, resizing_sidebar)
        });
        let track_rect = egui::Rect::from_min_max(egui::pos2(main.right(), frame.top()), frame.max);
        let editing = self.plugin_track_tool_active();
        ui.allocate_ui_at_rect(track_rect, |ui| {
            ui.set_clip_rect(track_rect.intersect(ui.clip_rect()));
            let scroll = egui::ScrollArea::horizontal()
                .id_source("plugin-timeline-columns")
                .max_width(width)
                .max_height(frame.height())
                .auto_shrink([false, false])
                .drag_to_scroll(false);
            let content = |ui: &mut egui::Ui| {
                let mut left = ui.min_rect().left();
                let content = egui::Rect::from_min_size(
                    egui::pos2(left, frame.top()),
                    egui::vec2(desired, frame.height()),
                );
                for (origin, id, title, requested) in tracks {
                    let column_width = requested;
                    let column = egui::Rect::from_min_size(
                        egui::pos2(left, frame.top()),
                        egui::vec2(column_width, frame.height()),
                    );
                    left += column_width;
                    let header =
                        egui::Rect::from_min_size(column.min, egui::vec2(column.width(), 32.));
                    let mut close = false;
                    ui.painter().rect_filled(header, 0., crate::theme::base());
                    ui.allocate_ui_at_rect(header, |ui| {
                        ui.set_clip_rect(header.intersect(ui.clip_rect()));
                        ui.horizontal(|ui| {
                            close = ui.button("×").clicked();
                            ui.label(&title);
                        });
                    });
                    if close {
                        if let Some(track) = self
                            .plugins
                            .rendering
                            .tracks
                            .get_mut(&(origin.plugin.clone(), id.clone()))
                        {
                            track.open = false;
                        }
                        self.cancel_plugin_track_input(&origin, &id);
                        continue;
                    }
                    let top = self.timeline.area.top().max(header.bottom());
                    let rect = egui::Rect::from_min_max(egui::pos2(column.left(), top), column.max);
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
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(rect, 0., crate::theme::base());
                    super::drawing::paint_clock(&painter, rect, &timeline);
                    let viewport = Viewport {
                        size: [rect.width(), rect.height()],
                        timeline: Some(timeline),
                    };
                    self.plugin_track_input(ui, &origin, &id, rect, viewport.clone());
                    self.paint_plugin_view(
                        &painter,
                        rect,
                        origin,
                        RenderTarget {
                            kind: RenderKind::Track,
                            id,
                        },
                        "embedded-track",
                        viewport,
                    );
                }
                ui.allocate_rect(content, egui::Sense::hover());
            };
            if editing {
                scroll.show(ui, content);
            } else {
                scroll.touch_show_x(ui, content);
            }
        });
        ui.allocate_rect(frame, egui::Sense::hover());
    }
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
