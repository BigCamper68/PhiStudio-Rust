use super::*;

#[derive(Clone, Copy)]
pub struct Grid {
    pub rect: Rect,
    pub event_rect: Rect,
    pub beat: f64,
    pub zoom: f32,
    pub division: i32,
    pub lanes: usize,
    pub snap_x_enabled: bool,
    pub scroll_speed: f32,
    pub note_width: f32,
    pub camera_x: f64,
}

impl Grid {
    pub fn events(self) -> Self {
        Self {
            rect: self.event_rect,
            ..self
        }
    }
    pub fn full_rect(self) -> Rect {
        self.rect.union(self.event_rect)
    }
    pub fn baseline(self) -> f32 {
        self.rect.bottom() - 24.
    }
    pub fn y(self, beat: f64) -> f32 {
        self.baseline() - ((beat - self.beat) as f32) * self.zoom
    }
    pub fn x(self, x: f64) -> f32 {
        self.rect.center().x + (x - self.camera_x) as f32 / 1350. * self.rect.width()
    }
    pub fn raw_beat(self, y: f32) -> f64 {
        self.beat + ((self.baseline() - y) / self.zoom) as f64
    }
    pub fn beat(self, y: f32) -> Beat {
        Beat::from_grid(self.raw_beat(y).max(0.), self.division).unwrap_or(Beat::ZERO)
    }
    pub fn snap_x(self, x: f64) -> f64 {
        if !self.snap_x_enabled {
            return x;
        }
        let step = 1350. / (self.lanes.max(2) - 1) as f64;
        -675. + ((x + 675.) / step).round() * step
    }
    pub fn note_x(self, x: f32) -> f64 {
        self.snap_x(
            self.camera_x + (x - self.rect.center().x) as f64 / self.rect.width() as f64 * 1350.,
        )
    }
    pub fn note_rect(self, note: &Note) -> Rect {
        let start = self.y(note.start_time.as_f64());
        let end = if note.kind == NoteKind::Hold {
            self.y(note.end_time.as_f64())
        } else {
            start
        };
        Rect::from_min_max(
            pos2(
                self.x(note.x) - self.note_width * 19. / 46.,
                start.min(end) - 6.,
            ),
            pos2(
                self.x(note.x) + self.note_width * 19. / 46.,
                start.max(end) + 6.,
            ),
        )
    }
}

pub fn geometry(area: Rect, app: &Workbench) -> Grid {
    let full = Rect::from_min_max(area.min + vec2(38., 22.), area.max - vec2(14., 0.));
    let split = full.left() + full.width() * app.timeline.split.clamp(0.35, 0.7);
    Grid {
        rect: Rect::from_min_max(full.min, pos2(split - 6., full.bottom())),
        event_rect: Rect::from_min_max(pos2(split + 6., full.top()), full.max),
        beat: app.grid_beat(),
        zoom: app.timeline.zoom,
        division: if app.options.snap_time {
            app.timeline.division
        } else {
            1920
        },
        lanes: app.timeline.lanes,
        snap_x_enabled: app.options.snap_x,
        scroll_speed: app.options.scroll_speed,
        note_width: app.options.grid_note_width,
        camera_x: app.timeline.camera_x,
    }
}
