//! Adapters for extension track windows and the editor's preview surfaces.
use super::*;
use phistudio_sdk::Viewport;

impl Workbench {
    pub(in crate::plugins) fn plugin_track_input(
        &mut self,
        ui: &egui::Ui,
        origin: &Origin,
        id: &str,
        rect: Rect,
        viewport: Viewport,
    ) {
        if self
            .plugins
            .input
            .tool
            .as_ref()
            .is_none_or(|t| t.origin != *origin || !t.surfaces.contains(&InputSurfaceKind::Track))
        {
            return;
        }
        let events = ui.input(|i| i.events.clone());
        let grid = viewport.timeline.as_ref().map(|clock| Grid {
            rect,
            event_rect: Rect::NOTHING,
            beat: clock.beat,
            zoom: clock.pixels_per_beat,
            division: clock.division,
            lanes: clock.lanes,
            snap_x_enabled: self.options.snap_x,
            scroll_speed: self.options.scroll_speed,
            note_width: self.options.grid_note_width,
            camera_x: (clock.x_min + clock.x_max) / 2.,
        });
        let region = Region {
            rect,
            grid,
            line: self.line,
            surface: InputSurface {
                kind: InputSurfaceKind::Track,
                id: Some(id.into()),
                instance: format!("track-{id}"),
                viewport,
                metadata: Value::Null,
            },
        };
        self.plugin_region_input(&events, region, &|pos| {
            ui.clip_rect().contains(pos) && ui.ctx().layer_id_at(pos) == Some(ui.layer_id())
        });
    }
    pub(in crate::plugins) fn cancel_plugin_track_input(&mut self, origin: &Origin, id: &str) {
        if self.plugins.input.capture.as_ref().is_some_and(|c| {
            c.scope.origin() == origin
                && c.region.surface.kind == InputSurfaceKind::Track
                && c.region.surface.id.as_deref() == Some(id)
        }) {
            self.cancel_plugin_input();
        }
    }
    pub(crate) fn plugin_preview_input(
        &mut self,
        ui: &egui::Ui,
        rect: Rect,
        exit: Rect,
    ) -> Option<Vec<Event>> {
        if self.noise.preview_original.is_some() || self.noise.sample_target.is_some() {
            return None;
        }
        self.plugins
            .input
            .tool
            .as_ref()
            .filter(|t| t.surfaces.contains(&InputSurfaceKind::Preview))?;
        let events = ui.input(|i| i.events.clone());
        let region = Region {
            rect,
            grid: None,
            line: self.line,
            surface: InputSurface {
                kind: InputSurfaceKind::Preview,
                id: None,
                instance: if self.preview.fullscreen {
                    "preview-fullscreen"
                } else {
                    "preview-inline"
                }
                .into(),
                viewport: Viewport {
                    size: [rect.width(), rect.height()],
                    timeline: None,
                },
                metadata: Value::Null,
            },
        };
        Some(self.plugin_region_input(&events, region, &|pos| {
            !exit.contains(pos)
                && ui.clip_rect().contains(pos)
                && ui.ctx().layer_id_at(pos) == Some(ui.layer_id())
        }))
    }
}
