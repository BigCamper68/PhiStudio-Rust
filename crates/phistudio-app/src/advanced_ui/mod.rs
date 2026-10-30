//! Additional RPE authoring windows; all mutations use the shared history.
mod history;
mod storyboard;
mod windows;
use crate::{
    scroll::{TouchCombo as _, TouchScroll as _},
    workbench::Workbench,
};
use phistudio_core::{Beat, ChartMetadata, Command, NoteId, StoryEvent, StoryKind};
use std::collections::BTreeMap;
#[path = "../clone_ui.rs"]
mod clone_ui;
pub(crate) use clone_ui::CloneDraft;
#[derive(Clone, Copy)]
pub(crate) struct Anchor {
    pub line: usize,
    pub beat: f64,
    pub x: f64,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum RightMode {
    #[default]
    Events,
    Storyboard,
    Shaders,
}
pub(crate) use phistudio_core::story_tools::StoryId;
#[derive(Default)]
pub(crate) struct AdvancedState {
    pub shortcut_capture: Option<String>,
    pub shortcut_error: Option<(String, &'static str)>,
    pub shortcuts_window: bool,
    pub metadata: Option<(u64, ChartMetadata)>,
    pub story: Option<StoryDraft>,
    pub right_mode: RightMode,
    pub story_selected: Option<StoryId>,
    pub story_selection: Vec<StoryId>,
    pub clone: Option<CloneDraft>,
    pub history: bool,
    pub anchors: BTreeMap<u64, Anchor>,
    pub line_filter: usize,
    pub line_states: Vec<phistudio_prpr::LineState>,
    pub visible_lines: std::collections::BTreeSet<usize>,
    pub monitor: bool,
    pub playback_rate: f64,
}
pub(crate) struct StoryDraft {
    line: usize,
    kind: StoryKind,
    index: Option<usize>,
    start: String,
    end: String,
    pub(crate) event: StoryEvent,
    revision: u64,
}
pub(crate) fn window(title: &str, ctx: &egui::Context) -> crate::window::Window<'static> {
    crate::i18n::window(title)
        .default_pos([40., 48.])
        .default_width(430.)
        .resize(|r| {
            r.max_size([
                (ctx.screen_rect().width() - 24.).max(180.),
                (ctx.screen_rect().height() - 100.).max(120.),
            ])
        })
}
impl Workbench {
    pub(crate) fn close_advanced_window(&mut self) -> bool {
        if self.close_color_picker() {
            return true;
        }
        let a = &mut self.advanced;
        if a.shortcut_capture.take().is_some() {
            return true;
        }
        if let Some(d) = &mut a.clone
            && d.cancel_chooser()
        {
            return true;
        }
        a.clone.take().is_some()
            || a.metadata.take().is_some()
            || std::mem::take(&mut a.shortcuts_window)
            || std::mem::take(&mut a.monitor)
            || std::mem::take(&mut a.history)
    }
}
pub(crate) fn story_curve(event: &StoryEvent) -> phistudio_core::LineEvent {
    phistudio_core::LineEvent {
        start_time: event.start_time,
        end_time: event.end_time,
        easing_type: event.easing_type,
        easing_left: event.easing_left,
        easing_right: event.easing_right,
        bezier: event.bezier,
        bezier_points: event.bezier_points,
        ..Default::default()
    }
}
