//! Shader tracks use the ordinary vertical grid and its ordered touch router.
mod hit_test;
use hit_test::{at, column, hit, hits, interval, lanes, rows};
pub(crate) use hit_test::{cycle_hits, selected_hit};
mod gestures;
pub(crate) use gestures::{begin, finish, select, tap, update};
mod paint;
use super::*;
use crate::{
    grid_input::{Gesture, Grid, Intent, IntervalDrag},
    workbench::Tool,
};
use egui::{Align2, Pos2, Stroke, pos2};
pub(crate) use paint::paint;
use phistudio_core::{StoryEvent, StoryKind};
use std::collections::BTreeSet;

pub(crate) enum ShaderGesture {
    Edit(Box<Edit>),
    Erase {
        ids: BTreeSet<usize>,
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
}
