//! Adapter to the actual Phira prpr parser and renderer. No alternate chart
//! evaluator lives here: line/note animations go through upstream prpr.
//! Noise Fields extend Preview with the supplied Phigros BlockRender passes.
#[cfg(test)]
mod authoring_tests;
mod cover;
mod inspection;
mod lifecycle;
mod media;
mod noise_fields;
mod sound_gain;
pub use sound_gain::render_hitsound_gain;
mod playback;
mod render;
pub use cover::{CoverImage, audio_file_length, load_project_cover, load_texture_thumbnail};
pub use media::{MediaInspection, ResourceAudioClip, inspect_media};
mod anchors;
mod assets;
mod autoplay;
mod background;
mod compatibility;
mod curve;
mod editor_audio;
mod editor_options;
mod effects;
pub use picking::LineState;
pub use skin::inspect_skin;
mod hud;
mod picking;
mod playtest;
pub use playtest::PlaytestReport;
mod resolution;
mod resources;
mod skin;
mod transport;
pub use curve::{ControlCurve, EventCurve, slice_event};
pub use skin::{ARCHIVE_SHA256 as SKIN_ARCHIVE_SHA256, EditorSkin, SkinStyle, SkinTexture};
mod grid_atlas;
mod video;
pub use grid_atlas::GridAtlas;
pub use video::MovieAudio;

use anyhow::Result;
use glyph_brush::ab_glyph::FontArc;
use macroquad::prelude::*;
use phistudio_core::{ChartDocument, ProjectOptions};
use prpr::fs::FileSystem;
use prpr::judge::{Judge, JudgeStatus};
use prpr::{
    core::{BOLD_FONT, Chart, PGR_FONT, Resource},
    ui::{TextPainter, Ui},
};
use std::path::{Path, PathBuf};

pub const PHIRA_REVISION: &str = "0afd16621dd66557841a8cffe6813afff63335ec";
/// Original Phira text face, also used as the editor's Unicode fallback.
pub static TEXT_FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/phira.ttf");
static EMOJI_FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/NotoEmoji-Regular.ttf");

/// Called directly by Android's lifecycle callback, even with rendering stopped.
pub fn suspend_audio() {
    transport::suspend_audio();
    media::suspend();
}

pub struct Preview {
    noise_fields: noise_fields::NoiseFields,
    anchors: anchors::Anchors,
    offline_step: Option<f32>,
    resource_music: Option<std::sync::Arc<std::sync::Mutex<sasa::Music>>>,
    maximum_target_edge: u32,
    input_pump: Option<prpr::scene::Main>,
    playtest: Option<playtest::PlaySession>,
    playtest_viewport: Option<(i32, i32, i32, i32)>,
    shader_warning: Option<String>,
    shader_key: Option<effects::ShaderKey>,
    sound_schedule: editor_audio::SoundSchedule,
    pick_visible_lines: std::collections::BTreeSet<usize>,
    pick_visible_ids: Vec<usize>,
    picking: Option<(RenderTarget, Material)>,
    source_ids: Vec<Option<phistudio_core::NoteId>>,
    editor_options: phistudio_core::EditorOptions,
    active_line: usize,
    package_force_aspect: bool,
    package_dim: f32,
    pause_bounds: [f32; 4],
    chart: Chart,
    judge: Judge,
    resource: Resource,
    transport: transport::Transport,
    text: TextPainter,
    chart_text: TextPainter,
    last_time: f64,
    target: RenderTarget,
    background_key: Option<(Option<PathBuf>, Option<String>)>,
    music_key: Option<(Option<PathBuf>, Option<String>)>,
    music_offset: f64,
    music_length: Option<f64>,
    audio_available: bool,
    audio_warning: Option<String>,
    music_warning: Option<String>,
    illustration_warning: Option<String>,
    needs_seek: bool,
    clear_particles: bool,
    last_render_wall: f64,
    has_background: bool,
    resource_warning: Option<String>,
}

pub use effects::shader_preset;
