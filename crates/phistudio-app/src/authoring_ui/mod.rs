mod curves;

pub(crate) use curves::{draw_curve, easing_fields, shader_easing_fields};
mod events;
mod line;
mod properties;
mod tempo;
mod windows;
use crate::{
    scroll::{TouchCombo as _, TouchScroll as _},
    theme::*,
    workbench::Workbench,
};
use egui::{self, RichText};
use phistudio_core::{
    AttachedUi, Beat, BpmChange, Command, EditTarget, EventId, EventType, LineEvent, LineProperties,
};
use std::sync::Arc;

#[derive(Default)]
pub struct EventState {
    pub layer: usize,
    pub selected: Option<EventId>,
    draft: Option<EventDraft>,
    cache: std::collections::BTreeMap<(usize, usize), EventCache>,
}
struct EventCache {
    revision: u64,
    rows: Arc<Vec<(EventId, LineEvent)>>,
}
struct EventDraft {
    id: EventId,
    revision: u64,
    event: LineEvent,
    start: String,
    end: String,
}
pub struct LineDraft {
    pub(crate) index: usize,
    pub(crate) revision: u64,
    pub(crate) properties: LineProperties,
}
pub struct BpmDraft {
    index: Option<usize>,
    revision: u64,
    start: String,
    bpm: f64,
}

pub const EASINGS: [&str; 29] = [
    "Linear",
    "Out Sine",
    "In Sine",
    "Out Quad",
    "In Quad",
    "In Out Sine",
    "In Out Quad",
    "Out Cubic",
    "In Cubic",
    "Out Quart",
    "In Quart",
    "In Out Cubic",
    "In Out Quart",
    "Out Quint",
    "In Quint",
    "Out Expo",
    "In Expo",
    "Out Circ",
    "In Circ",
    "Out Back",
    "In Back",
    "In Out Circ",
    "In Out Back",
    "Out Elastic",
    "In Elastic",
    "Out Bounce",
    "In Bounce",
    "In Out Bounce",
    "In Out Elastic",
];
