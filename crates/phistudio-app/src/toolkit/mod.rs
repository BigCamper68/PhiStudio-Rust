mod clipboard;
mod edit;
mod generators;
mod selection;
mod shortcuts;
mod transactions;
use crate::{
    theme::*,
    workbench::{Tool, Workbench},
};
use egui::{self, RichText};
use phistudio_core::{
    Beat, ChartError, EventId, EventType, LineEvent, NoteId, NoteKind,
    tools::{self, ChartClipboard, Selection, ToolEdit, Transform, ValueField, ValueMode},
};
use std::sync::Arc;

mod ui;

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub(crate) enum Page {
    #[default]
    Selection,
    Transform,
    Values,
    Notes,
    Events,
    Motion,
    Text,
    Configurations,
    Clipboard,
    Story,
}
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub(crate) enum Scope {
    #[default]
    Notes,
    Events,
    Both,
}
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Default)]
enum Profile {
    #[default]
    Ramp,
    Constant,
    Sequence,
    Sine,
    Random,
}

struct NotePreview {
    defaults: phistudio_core::NoteCreationDefaults,
    pattern: tools::NotePattern,
    easing: i32,
    result: Result<Arc<Vec<phistudio_core::Note>>, String>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct ToolsState {
    #[serde(skip)]
    pub selection: Selection,
    #[serde(skip)]
    pub clipboard: Option<Arc<phistudio_core::objects::ObjectClipboard>>,
    #[serde(skip)]
    pub show: bool,
    #[serde(skip)]
    pub additive: bool,
    pub scope: Scope,
    pub scale_anchor: crate::selection_scaling::Anchor,
    pub(crate) page: Page,
    pub(crate) last_page: Page,
    #[serde(skip)]
    nav_context: Option<bool>,
    start: String,
    end: String,
    filter_kind: Option<NoteKind>,
    filter_side: Option<u8>,
    filter_fake: Option<bool>,
    filter_track: Option<EventType>,
    x_range: [f64; 2],
    shift: String,
    shift_x: f64,
    factor: String,
    mirror: f64,
    target_line: usize,
    target_layer: usize,
    repeat_count: usize,
    repeat_spacing: String,
    note_kind: NoteKind,
    hold_length: String,
    field: ValueField,
    mode: ValueMode,
    profile: Profile,
    values: [f64; 2],
    sequence: String,
    cycles: f64,
    seed: u64,
    disturbance: f64,
    easing: i32,
    parts: usize,
    curve_window: [f64; 2],
    curve_points: [f64; 4],
    curve_bezier: bool,
    pattern: tools::Pattern,
    step: String,
    pattern_x: [f64; 2],
    controls: [f64; 2],
    include_ends: bool,
    #[serde(skip)]
    note_preview: Option<NotePreview>,
    multiply_sequence: bool,
}
impl Default for ToolsState {
    fn default() -> Self {
        Self {
            selection: Selection::default(),
            clipboard: None,
            show: false,
            additive: false,
            scope: Scope::Notes,
            scale_anchor: Default::default(),
            page: Page::Selection,
            last_page: Page::Selection,
            nav_context: None,
            start: "0".into(),
            end: "4".into(),
            filter_kind: None,
            filter_side: None,
            filter_fake: None,
            filter_track: None,
            x_range: [-675., 675.],
            shift: "0".into(),
            shift_x: 0.,
            factor: "2".into(),
            mirror: 0.,
            target_line: 0,
            target_layer: 0,
            repeat_count: 1,
            repeat_spacing: "4".into(),
            note_kind: NoteKind::Drag,
            hold_length: "1".into(),
            field: ValueField::X,
            mode: ValueMode::Set,
            profile: Profile::Ramp,
            values: [-450., 450.],
            sequence: "1, -1".into(),
            cycles: 1.,
            seed: 1,
            disturbance: 0.,
            easing: 1,
            parts: 2,
            curve_window: [0., 1.],
            curve_points: [0.25, 0.25, 0.75, 0.75],
            curve_bezier: false,
            pattern: tools::Pattern::Line,
            step: "0:1/4".into(),
            pattern_x: [-450., 450.],
            controls: [-200., 200.],
            include_ends: true,
            note_preview: None,
            multiply_sequence: false,
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod pair_tests;

impl ToolsState {
    pub(crate) fn sanitize(&mut self) {
        self.easing = self.easing.clamp(1, 29);
        self.parts = self.parts.clamp(1, 20000);
        self.target_layer = self.target_layer.min(3);
    }
}

impl ToolsState {
    pub(crate) fn restore_context(&mut self, story: bool) {
        self.nav_context = Some(story);
    }
    pub(crate) fn page(&self) -> Page {
        self.page
    }
}
