//! Paired Noise Field and animation timelines share the project transport.
mod commands;
pub(crate) mod grid;
pub(crate) mod layout;
mod navigation;
mod preview;
mod properties;
mod sample;
pub(crate) mod timeline;
use crate::{
    advanced_ui::RightMode,
    scroll::TouchCombo as _,
    theme::*,
    workbench::{Tool, Workbench},
};
use phistudio_core::{
    Beat, Command,
    noise_fields::{self as noise, Action, Format, Track},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub(crate) struct State {
    /// Leftmost field column, independent of pixel size and grid density.
    pub offset: f64,
    cache: Option<(u64, u64)>,
    rows: Vec<Value>,
    error: Option<String>,
    pub selected: Option<usize>,
    pub track: Track,
    pub animation: bool,
    draft: Option<Draft>,
    key_draft: Option<KeyDraft>,
    pub key_selected: Option<usize>,
    pub selection: BTreeSet<usize>,
    pub format: Format,
    subtract: bool,
    clipboard: Option<Clipboard>,
    pub sample_target: Option<SampleTarget>,
    sample_original: Option<Value>,
    sample_drag: Option<(Value, egui::Pos2)>,
    cycle: Option<(egui::Pos2, Vec<usize>, usize, u64)>,
    preview_drag: Option<(Value, egui::Pos2, usize)>,
    pub preview_original: Option<Value>,
    pub preview_handles: Vec<egui::Rect>,
    pub preview_toolbar: Option<egui::Rect>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            offset: Default::default(),
            cache: Default::default(),
            rows: Default::default(),
            error: Default::default(),
            selected: Default::default(),
            track: Default::default(),
            animation: Default::default(),
            draft: Default::default(),
            key_draft: Default::default(),
            key_selected: Default::default(),
            selection: Default::default(),
            format: Format::Next,
            subtract: Default::default(),
            clipboard: Default::default(),
            sample_target: Default::default(),
            sample_original: Default::default(),
            sample_drag: Default::default(),
            cycle: Default::default(),
            preview_drag: Default::default(),
            preview_original: Default::default(),
            preview_handles: Default::default(),
            preview_toolbar: Default::default(),
        }
    }
}

struct Draft {
    raw: Value,
    times: [String; 4],
}
struct KeyDraft {
    raw: Value,
    times: [String; 2],
}
#[derive(Clone)]
struct Clipboard {
    format: Format,
    track: Option<Track>,
    rows: Vec<Value>,
    origin: Beat,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SampleTarget {
    Position,
    Anchor,
}
impl Workbench {
    pub(crate) fn refresh_noise(&mut self) {
        let cache = (self.session, self.editor.revision());
        if self.noise.cache == Some(cache) {
            return;
        }
        let session_changed = self.noise.cache.is_some_and(|k| k.0 != self.session);
        if session_changed {
            self.noise = Default::default();
        }
        self.noise.cache = Some(cache);
        self.noise.draft = None;
        self.noise.key_draft = None;
        match noise::fields(self.editor.document()) {
            Ok(rows) => {
                self.noise.rows = rows.to_vec();
                self.noise.error = None;
            }
            Err(e) => {
                self.noise.rows.clear();
                self.noise.error = Some(e.to_string());
            }
        }
        self.noise.selected = self.noise.selected.filter(|i| *i < self.noise.rows.len());
        let count = self.noise_count();
        self.noise.selection.retain(|i| *i < count);
        let Some(index) = self.noise.selected else {
            return;
        };
        let raw = self.noise.rows[index].clone();
        let f = match noise::format(&raw) {
            Ok(f) => f,
            Err(e) => {
                self.noise.error = Some(e.to_string());
                return;
            }
        };
        if !Track::all(f).contains(&self.noise.track) {
            self.noise.track = Track::all(f)[0];
        }
        let mut times = std::array::from_fn(|_| String::new());
        for (i, key) in noise::PHASES.iter().enumerate() {
            if f == Format::Next && (i == 1 || i == 2) {
                continue;
            }
            times[i] = noise::beat(&raw, key, f, self.editor.document().tempo())
                .map(|b| b.to_string())
                .unwrap_or_default();
        }
        self.noise.draft = Some(Draft { raw, times });
        self.load_noise_key();
    }
    fn noise_count(&self) -> usize {
        if self.noise.animation {
            self.noise
                .selected
                .and_then(|i| self.noise.rows.get(i))
                .and_then(|v| noise::keys(v, self.noise.track).ok())
                .map_or(0, <[Value]>::len)
        } else {
            self.noise.rows.len()
        }
    }
    fn noise_format(&self) -> Format {
        self.noise
            .selected
            .and_then(|i| self.noise.rows.get(i))
            .and_then(|v| noise::format(v).ok())
            .unwrap_or(self.noise.format)
    }
    fn load_noise_key(&mut self) {
        let Some(index) = self.noise.key_selected else {
            return;
        };
        let Some(raw) = self
            .noise
            .draft
            .as_ref()
            .and_then(|d| noise::keys(&d.raw, self.noise.track).ok())
            .and_then(|v| v.get(index))
            .cloned()
        else {
            self.noise.key_selected = None;
            return;
        };
        let native = self.noise.track.native(self.noise_format());
        let tempo = self.editor.document().tempo();
        let times = if native {
            [
                noise::beat(&raw, "time", Format::Native, tempo)
                    .map(|b| b.to_string())
                    .unwrap_or_default(),
                String::new(),
            ]
        } else {
            [raw["startTime"].clone(), raw["endTime"].clone()].map(|v| {
                serde_json::from_value::<Beat>(v)
                    .map(|b| b.to_string())
                    .unwrap_or_default()
            })
        };
        self.noise.key_draft = Some(KeyDraft { raw, times });
    }
    pub(super) fn noise_tick(&self) -> Beat {
        Beat::new(
            0,
            1,
            if self.options.snap_time {
                self.timeline.division
            } else {
                1920
            },
        )
        .unwrap_or(Beat::ONE)
    }
    pub(crate) fn noise_placement_label(&self) -> &'static str {
        if self.noise.animation {
            "+ Keyframe"
        } else {
            "+ Field"
        }
    }
    pub(crate) fn noise_back(&mut self) -> bool {
        if self.advanced.right_mode != RightMode::NoiseFields {
            return false;
        }
        if self.noise.sample_target.is_some() {
            self.close_fullscreen_preview();
            return true;
        }
        if !self.flush_noise_draft() {
            self.noise.cache = None;
            self.noise.key_draft = None;
            self.error = None;
            self.refresh_noise();
        }
        if self.noise.animation {
            self.noise.animation = false;
            self.noise.selection.clear();
        } else {
            self.advanced.right_mode = RightMode::Events;
        }
        self.timeline.cancel_gesture();
        true
    }
}
#[cfg(test)]
mod tests;
