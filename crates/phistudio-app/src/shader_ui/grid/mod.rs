//! Shader tracks use the ordinary vertical grid and its ordered touch router.
mod hit_test;
use hit_test::{at, hit, hits, interval, row_column, rows, rows_for};
pub(crate) use hit_test::{cycle_hits, selected_hit};
mod gestures;
pub(crate) use gestures::{begin_dual, finish, select, tap, update};
mod paint;
mod selection;
use super::*;
use crate::{
    grid_input::{Gesture, Grid, Intent, IntervalDrag},
    workbench::Tool,
};
use egui::{Align2, Pos2, Stroke, pos2};
pub(crate) use paint::paint;
use phistudio_core::{StoryEvent, StoryKind};
pub(super) use selection::{Item, selection_key};
use std::collections::BTreeSet;

pub(crate) enum ShaderGesture {
    Edit(Box<Edit>),
    Navigate {
        grid: Grid,
        origin: Pos2,
        offset: f64,
        horizontal: Option<bool>,
    },
    Box {
        grid: Grid,
        keys: bool,
        origin: Pos2,
        current: Pos2,
        additive: bool,
    },
    Erase {
        ids: BTreeSet<Item>,
        keys: bool,
        grid: Grid,
        last: Pos2,
    },
}
impl ShaderGesture {
    pub(crate) fn edits_existing(&self) -> bool {
        matches!(self, Self::Edit(edit) if !edit.creating && edit.moved)
    }
}
pub(crate) struct Edit {
    effect: usize,
    keyframe: Option<(String, usize)>,
    creating: bool,
    original: Value,
    current: Value,
    origin: Pos2,
    grid: Grid,
    interval: IntervalDrag,
    moved: bool,
    lane: f64,
    group: Vec<(Item, Value)>,
}
