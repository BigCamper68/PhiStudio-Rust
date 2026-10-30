//! Grid gestures use the actual down position and a frozen chart transform.
//! UI pointer deltas are not a gesture origin. A swipe in a placement tool pans;
//! only a tap places a simple note. This follows Java PhiStudio's router.
mod geometry;
pub use geometry::{Grid, geometry};
mod routing;
pub use routing::handle;

mod press;
use press::begin;
mod drag;
use drag::update;
mod release;
use release::finish;
mod selection;
use crate::workbench::{Tool, Workbench};
use egui::{Event, Pos2, Rect, TouchPhase, pos2, vec2};
use phistudio_core::{
    Beat, Command, EventId, Note, NoteId, NoteKind,
    tools::{Selection, Transform},
};
pub(crate) use selection::note_hits;
use selection::{add, box_selection, erase_at, pick, pick_for_drag, selection_move};
use std::collections::BTreeMap;

const SLOP: f32 = 8.;

/// Shared interval editing for Storyboard and shader tracks. Input coordinates
/// are already snapped in the track's clock; the pointer router owns the slop.
pub(crate) struct IntervalDrag {
    pub original: [Beat; 2],
    pub current: [Beat; 2],
    pub origin: Beat,
    pub earliest: Beat,
    pub handle: Option<bool>,
}
impl IntervalDrag {
    pub fn update(&mut self, point: Beat, division: i32) {
        let tick = Beat::new(0, 1, division).unwrap();
        match self.handle {
            Some(true) => {
                self.current[0] = point
                    .min(self.current[1].checked_sub(tick).unwrap_or(Beat::ZERO))
                    .max(Beat::ZERO)
            }
            Some(false) => {
                self.current[1] =
                    point.max(self.current[0].checked_add(tick).unwrap_or(self.current[0]))
            }
            None => {
                if let Ok(delta) = point.checked_sub(self.origin) {
                    let start = self.original[0]
                        .checked_add(delta)
                        .unwrap_or(Beat::ZERO)
                        .max(
                            self.original[0]
                                .checked_sub(self.earliest)
                                .unwrap_or(Beat::ZERO),
                        );
                    if let Ok(end) = start
                        .checked_sub(self.original[0])
                        .and_then(|v| self.original[1].checked_add(v))
                    {
                        self.current = [start, end];
                    }
                }
            }
        }
    }
}

pub enum Intent {
    Place(NoteKind),
    Select(Option<NoteId>, bool),
    SelectEvent(Option<EventId>, bool),
    SelectStory(Option<crate::advanced_ui::StoryId>, bool),
    SelectShader(Option<usize>),
    Seek,
    Pan,
}
pub enum Gesture {
    SelectionScale {
        selection: Selection,
        grid: Grid,
        origin: Pos2,
        anchor: f64,
        factor: f64,
        moved: bool,
    },
    Box {
        origin: Pos2,
        current: Pos2,
        grid: Grid,
        additive: bool,
    },
    Erase {
        stories: std::collections::BTreeSet<crate::advanced_ui::StoryId>,
        selection: Selection,
        grid: Grid,
        last: Pos2,
    },
    SelectionMove {
        selection: Selection,
        stories: Vec<crate::advanced_ui::StoryId>,
        origin: Pos2,
        grid: Grid,
        beats: Beat,
        x: f64,
        earliest: Beat,
        x_range: [f64; 2],
        note_surface: bool,
        moved: bool,
    },
    Event(crate::event_grid::EventGesture),
    Story(Box<crate::story_grid::StoryGesture>),
    Shader(Box<crate::shader_ui::grid::ShaderGesture>),
    Tap {
        origin: Pos2,
        grid: Grid,
        intent: Intent,
        moved: bool,
    },
    Move {
        id: NoteId,
        original: Note,
        current: Note,
        origin: Pos2,
        grid: Grid,
        moved: bool,
    },
    Create {
        start: Beat,
        end: Beat,
        x: f64,
        kind: NoteKind,
        grid: Grid,
    },
    Resize {
        id: NoteId,
        current: Note,
        start_handle: bool,
        origin: Pos2,
        grid: Grid,
        moved: bool,
    },
}

struct Pinch {
    grid: Grid,
    center: Pos2,
    distance: f32,
}
#[derive(Default)]
pub struct GridInput {
    pub(crate) cycle: Option<crate::picking::TapCycle>,
    touches: BTreeMap<u64, Pos2>,
    owns_touches: bool,
    pinch: Option<Pinch>,
    pointer_active: bool,
    pub(crate) panel_offset: f32,
    pub(crate) panel_navigation: bool,
}
impl GridInput {
    /// Keep playback intent while the transport is temporarily paused.
    pub fn is_scrubbing(&self) -> bool {
        self.pointer_active || self.pinch.is_some()
    }
    pub fn cancel(&mut self) {
        *self = Self::default();
    }
}
