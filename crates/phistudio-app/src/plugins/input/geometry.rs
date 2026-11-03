use super::*;
use phistudio_sdk::{TimelineViewport, Viewport};

impl Region {
    pub(super) fn point(&self, pos: Pos2, pressure: Option<f32>) -> InputPoint {
        let local = pos - self.rect.min;
        let normalized = [
            f64::from(local.x / self.rect.width()),
            f64::from(local.y / self.rect.height()),
        ];
        let clock = self.surface.viewport.timeline.as_ref();
        let raw_beat =
            clock.map(|t| t.beat + f64::from(t.baseline - local.y) / f64::from(t.pixels_per_beat));
        let beat = raw_beat
            .and_then(|b| phistudio_core::Beat::from_grid(b.max(0.), clock?.division).ok())
            .map(|b| b.triple());
        let chart_x = if self.surface.kind == InputSurfaceKind::Notes {
            self.grid.map(|g| g.note_x(pos.x))
        } else if self.surface.kind == InputSurfaceKind::Track {
            clock.map(|t| t.x_min + normalized[0] * (t.x_max - t.x_min))
        } else {
            None
        };
        InputPoint {
            local: [f64::from(local.x), f64::from(local.y)],
            normalized,
            chart_x,
            raw_beat,
            beat,
            pressure,
        }
    }
}
impl Workbench {
    pub(super) fn plugin_input_layout_current(&self, capture: &Capture) -> bool {
        use crate::advanced_ui::RightMode;
        let surface = &capture.region.surface;
        if let Some(clock) = &surface.viewport.timeline {
            let division = if self.options.snap_time {
                self.timeline.division
            } else {
                1920
            };
            if clock.beat != self.grid_beat()
                || clock.pixels_per_beat != self.timeline.zoom
                || clock.division != division
                || clock.lanes != self.timeline.lanes
                || (matches!(
                    surface.kind,
                    InputSurfaceKind::Notes | InputSurfaceKind::Track
                ) && (clock.x_min != self.timeline.camera_x - 675.
                    || clock.x_max != self.timeline.camera_x + 675.))
            {
                return false;
            }
        }
        if let Some(area) = capture.grid_area {
            let mode_matches = matches!(
                (surface.kind, self.advanced.right_mode),
                (
                    InputSurfaceKind::Notes,
                    RightMode::Events | RightMode::Storyboard
                ) | (InputSurfaceKind::Events, RightMode::Events)
                    | (InputSurfaceKind::Storyboard, RightMode::Storyboard)
                    | (
                        InputSurfaceKind::ShaderEffects | InputSurfaceKind::ShaderParameters,
                        RightMode::Shaders
                    )
                    | (
                        InputSurfaceKind::NoiseFields | InputSurfaceKind::NoiseAnimation,
                        RightMode::NoiseFields
                    )
            );
            if area != self.timeline.area
                || !mode_matches
                || surface.metadata["layer"] != json!(self.events.layer)
                || capture
                    .region
                    .grid
                    .is_some_and(|g| g.snap_x_enabled != self.options.snap_x)
            {
                return false;
            }
        }
        true
    }
    pub(super) fn plugin_grid_region(&mut self, grid: Grid, pos: Pos2) -> Option<Region> {
        use crate::advanced_ui::RightMode;
        let events = grid.event_rect.contains(pos);
        if !events && !grid.rect.contains(pos) {
            return None;
        }
        let grid = self.input_line_grid(grid, pos)?;
        let (kind, id) = match (self.advanced.right_mode, events) {
            (RightMode::Events, false) | (RightMode::Storyboard, false) => {
                (InputSurfaceKind::Notes, "notes")
            }
            (RightMode::Events, true) => (InputSurfaceKind::Events, "events"),
            (RightMode::Storyboard, true) => (InputSurfaceKind::Storyboard, "storyboard"),
            (RightMode::Shaders, false) => (InputSurfaceKind::ShaderEffects, "shader-effects"),
            (RightMode::Shaders, true) => (InputSurfaceKind::ShaderParameters, "shader-parameters"),
            (RightMode::NoiseFields, false) => (InputSurfaceKind::NoiseFields, "noise-fields"),
            (RightMode::NoiseFields, true) => (InputSurfaceKind::NoiseAnimation, "noise-animation"),
        };
        let rect = if events { grid.event_rect } else { grid.rect };
        let notes = kind == InputSurfaceKind::Notes;
        let timeline = TimelineViewport {
            beat: grid.beat,
            pixels_per_beat: grid.zoom,
            baseline: grid.baseline() - rect.top(),
            x_min: if notes { grid.camera_x - 675. } else { 0. },
            x_max: if notes { grid.camera_x + 675. } else { 1. },
            division: grid.division,
            lanes: grid.lanes,
        };
        Some(Region {
            rect,
            grid: Some(grid),
            line: self.line,
            surface: InputSurface {
                kind,
                id: None,
                instance: format!("grid-{id}"),
                viewport: Viewport {
                    size: [rect.width(), rect.height()],
                    timeline: Some(timeline),
                },
                metadata: json!({"layer":self.events.layer}),
            },
        })
    }
}
