//! Versioned editor preferences, separate from chart/package data and recovery.
use crate::ChartError;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreviewAspect {
    #[default]
    Project,
    Wide,
    ThreeTwo,
    FourThree,
    SixteenTen,
    FiveFour,
    Square,
    UltraWide,
    Portrait,
}
impl PreviewAspect {
    pub const ALL: [Self; 9] = [
        Self::Project,
        Self::Wide,
        Self::ThreeTwo,
        Self::FourThree,
        Self::SixteenTen,
        Self::FiveFour,
        Self::Square,
        Self::UltraWide,
        Self::Portrait,
    ];
    pub fn ratio(self) -> Option<f32> {
        Some(match self {
            Self::Project => return None,
            Self::Wide => 16. / 9.,
            Self::ThreeTwo => 3. / 2.,
            Self::FourThree => 4. / 3.,
            Self::SixteenTen => 16. / 10.,
            Self::FiveFour => 5. / 4.,
            Self::Square => 1.,
            Self::UltraWide => 21. / 9.,
            Self::Portrait => 9. / 16.,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Wide => "16:9",
            Self::ThreeTwo => "3:2",
            Self::FourThree => "4:3",
            Self::SixteenTen => "16:10",
            Self::FiveFour => "5:4",
            Self::Square => "1:1",
            Self::UltraWide => "21:9",
            Self::Portrait => "9:16",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorOptions {
    pub fullscreen_app: bool,
    pub check_updates_on_start: bool,
    pub release_notes_seen: Option<String>,
    pub editor_sounds: crate::EditorSoundOptions,
    pub audio_guide: AudioGuideOptions,
    pub audio_notes: crate::tools::AudioNoteOptions,
    pub language: Language,
    pub theme: ThemeColors,
    pub skin: Option<String>,
    pub preview_shaders: bool,
    pub preview_behind_grid: bool,
    pub preview_grid_brightness: f32,
    pub playtest_countdown: u32,
    pub playtest_from_cursor: bool,
    pub seek_notes_only: bool,
    pub lock_horizontal: bool,
    pub shortcuts: std::collections::BTreeMap<String, String>,
    pub preview_aspect: PreviewAspect,
    pub preview_note_scale: f32,
    pub mark_active_line_id: bool,
    pub highlight_current_line: bool,
    pub preview_hud: bool,
    pub preview_particles: bool,
    pub preview_accuracy: bool,
    pub preview_double_hint: bool,
    pub preview_dim: Option<f32>,
    pub snap_time: bool,
    pub snap_x: bool,
    pub noise_preview_grid: bool,
    pub scroll_speed: f32,
    pub numeric_drag: NumericDragOptions,
    pub grid_note_width: f32,
    pub minor_grid: bool,
    pub beat_labels: bool,
    pub recovery_seconds: u32,
    pub ui_scale: f32,
    pub xy_link: bool,
    pub history_jump: bool,
    pub auto_connect_events: bool,
    pub auto_copy_selection: bool,
    pub hitsound_delay_ms: f32,
    pub drag_delay_ms: f32,
}
impl Default for EditorOptions {
    fn default() -> Self {
        Self {
            fullscreen_app: false,
            check_updates_on_start: true,
            release_notes_seen: None,
            editor_sounds: crate::EditorSoundOptions::default(),
            audio_guide: AudioGuideOptions::default(),
            audio_notes: crate::tools::AudioNoteOptions::default(),
            language: Language::English,
            theme: ThemeColors::default(),
            skin: None,
            lock_horizontal: false,
            preview_shaders: true,
            preview_behind_grid: false,
            preview_grid_brightness: 0.5,
            playtest_countdown: 3,
            playtest_from_cursor: false,
            seek_notes_only: false,
            shortcuts: Default::default(),
            preview_aspect: PreviewAspect::Project,
            preview_note_scale: 1.,
            mark_active_line_id: true,
            highlight_current_line: true,
            preview_hud: true,
            preview_particles: true,
            preview_accuracy: false,
            preview_double_hint: true,
            preview_dim: None,
            snap_time: true,
            snap_x: true,
            noise_preview_grid: true,
            scroll_speed: 1.,
            numeric_drag: NumericDragOptions::default(),
            grid_note_width: 46.,
            minor_grid: true,
            beat_labels: true,
            recovery_seconds: 3,
            ui_scale: 1.,
            xy_link: false,
            history_jump: false,
            auto_connect_events: false,
            auto_copy_selection: false,
            hitsound_delay_ms: 0.,
            drag_delay_ms: 0.,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    #[serde(rename = "zh-CN", alias = "zh-Hans")]
    SimplifiedChinese,
    #[serde(rename = "zh-TW", alias = "zh-Hant", alias = "zh-Hant-TW")]
    TraditionalChinese,
    #[serde(rename = "ja", alias = "ja-JP")]
    Japanese,
    #[serde(rename = "ko", alias = "ko-KR")]
    Korean,
    #[serde(rename = "ru", alias = "ru-RU")]
    Russian,
    #[serde(rename = "es", alias = "es-ES", alias = "es-MX")]
    Spanish,
    #[default]
    #[serde(rename = "en", alias = "en-US", other)]
    English,
}
impl Language {
    pub const ALL: [Self; 7] = [
        Self::English,
        Self::SimplifiedChinese,
        Self::TraditionalChinese,
        Self::Japanese,
        Self::Korean,
        Self::Russian,
        Self::Spanish,
    ];
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::SimplifiedChinese => "zh-CN",
            Self::TraditionalChinese => "zh-TW",
            Self::Japanese => "ja",
            Self::Korean => "ko",
            Self::Russian => "ru",
            Self::Spanish => "es",
        }
    }
    pub fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::SimplifiedChinese => "简体中文",
            Self::TraditionalChinese => "繁體中文",
            Self::Japanese => "日本語",
            Self::Korean => "한국어",
            Self::Russian => "Русский",
            Self::Spanish => "Español",
        }
    }
    pub fn phira_locale(self) -> &'static str {
        match self {
            Self::English => "en-US",
            Self::SimplifiedChinese => "zh-CN",
            Self::TraditionalChinese => "zh-TW",
            Self::Japanese => "ja-JP",
            Self::Korean => "ko-KR",
            Self::Russian => "ru-RU",
            // The pinned prpr has no Spanish resources. Use a registered locale
            // explicitly so it cannot fall back to the device's unrelated locale.
            Self::Spanish => "en-US",
        }
    }
}
impl EditorOptions {
    pub fn normalized(mut self) -> Self {
        self.release_notes_seen = self
            .release_notes_seen
            .filter(|v| !v.is_empty() && v.len() <= 128);
        self.editor_sounds = self.editor_sounds.normalized();
        self.audio_guide = self.audio_guide.normalized();
        self.audio_notes = self.audio_notes.normalized();
        self.playtest_countdown = self.playtest_countdown.clamp(1, 3);
        self.skin = self.skin.filter(|s| crate::skins::valid_id(s));
        self.xy_link = false; // Retired UI control; manual Synchronize X/Y remains available.
        self.shortcuts.retain(|k, v| k.len() <= 64 && v.len() <= 64);
        self.hitsound_delay_ms = bounded(self.hitsound_delay_ms, -200., 200., 0.);
        self.drag_delay_ms = bounded(self.drag_delay_ms, -200., 200., 0.);
        self.preview_note_scale = bounded(self.preview_note_scale, 0.8, 1.2, 1.);
        self.preview_grid_brightness = bounded(self.preview_grid_brightness, 0., 1., 0.5);
        self.preview_dim = self
            .preview_dim
            .filter(|v| v.is_finite())
            .map(|v| v.clamp(0., 1.));
        self.scroll_speed = bounded(self.scroll_speed, 0.25, 3., 1.);
        self.numeric_drag = self.numeric_drag.normalized();
        self.grid_note_width = bounded(self.grid_note_width, 24., 90., 46.);
        self.recovery_seconds = self.recovery_seconds.clamp(1, 60);
        self.ui_scale = bounded(self.ui_scale, 0.75, 1.5, 1.);
        self
    }
}

/// Numeric fields share these editor-only gesture preferences. Typed values and
/// formulas keep their own precision; snapping applies only to horizontal drags.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NumericDragOptions {
    pub enabled: bool,
    pub sensitivity: f64,
    pub snap: bool,
    pub step: f64,
}
impl Default for NumericDragOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            sensitivity: 1.,
            snap: false,
            step: 5.,
        }
    }
}
impl NumericDragOptions {
    pub fn normalized(mut self) -> Self {
        self.sensitivity = if self.sensitivity.is_finite() {
            self.sensitivity.clamp(0.01, 100.)
        } else {
            1.
        };
        self.step = if self.step.is_finite() && self.step > 0. {
            self.step.clamp(0.000001, 1e9)
        } else {
            5.
        };
        self
    }
}

/// Passive visual guides only: never serialized into authored chart documents.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioGuideOptions {
    pub waveform: bool,
    pub attacks: bool,
    pub opacity: f32,
    pub gain: f32,
    pub sensitivity: f32,
    pub min_gap_ms: u32,
    pub band: usize,
}
impl Default for AudioGuideOptions {
    fn default() -> Self {
        Self {
            waveform: false,
            attacks: false,
            opacity: 0.22,
            gain: 1.,
            sensitivity: 0.5,
            min_gap_ms: 30,
            band: 0,
        }
    }
}
impl AudioGuideOptions {
    pub fn normalized(mut self) -> Self {
        self.opacity = bounded(self.opacity, 0.05, 0.6, 0.22);
        self.gain = bounded(self.gain, 0.25, 8., 1.);
        self.sensitivity = bounded(self.sensitivity, 0., 1., 0.5);
        self.min_gap_ms = self.min_gap_ms.clamp(20, 500);
        self.band = self.band.min(3);
        self
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeColors {
    pub colors: [[u8; 4]; 11],
}
impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            colors: [
                [12, 17, 29, 255],
                [20, 27, 42, 255],
                [29, 39, 57, 255],
                [45, 58, 78, 255],
                [145, 161, 182, 255],
                [233, 240, 249, 255],
                [115, 237, 202, 255],
                [103, 204, 255, 255],
                [127, 169, 255, 255],
                [255, 132, 165, 255],
                [255, 215, 127, 255],
            ],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MultiLinePreferences {
    pub all: bool,
    pub enabled: bool,
    pub events: bool,
    pub merged: bool,
    pub lines: Vec<usize>,
    pub widths: [f32; 2],
}
impl Default for MultiLinePreferences {
    fn default() -> Self {
        Self {
            all: false,
            enabled: false,
            events: false,
            merged: false,
            lines: Vec::new(),
            widths: [280., 360.],
        }
    }
}
impl MultiLinePreferences {
    pub fn normalized(mut self) -> Self {
        self.lines.truncate(4096);
        if self.all {
            self.lines.clear();
        }
        self.lines.retain(|line| *line < 100_000);
        self.lines.sort_unstable();
        self.lines.dedup();
        for (i, width) in self.widths.iter_mut().enumerate() {
            *width = bounded(*width, 80., 2400., [280., 360.][i]);
        }
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorPreferences {
    pub multi_line: MultiLinePreferences,
    pub version: u32,
    pub video: crate::video::VideoSettings,
    pub lanes: usize,
    pub division: i32,
    pub zoom: f32,
    pub timeline_split: f32,
    pub sidebar: bool,
    pub floating_preview: bool,
    pub floating_properties: bool,
    pub grid_window: bool,
    pub music_volume: f32,
    pub hitsound_volume: f32,
    pub options: EditorOptions,
}
impl Default for EditorPreferences {
    fn default() -> Self {
        Self {
            multi_line: Default::default(),
            version: 1,
            video: Default::default(),
            lanes: 11,
            division: 4,
            zoom: 96.,
            timeline_split: 0.54,
            sidebar: true,
            floating_preview: false,
            floating_properties: false,
            grid_window: false,
            music_volume: 1.,
            hitsound_volume: 1.,
            options: EditorOptions::default(),
        }
    }
}
impl EditorPreferences {
    pub fn normalized(mut self) -> Self {
        self.multi_line = self.multi_line.normalized();
        if ![3, 5, 7, 9, 11, 13, 17, 21, 25, 33].contains(&self.lanes) {
            self.lanes = 11;
        }
        if ![1, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64].contains(&self.division) {
            self.division = 4;
        }
        self.version = 1;
        self.zoom = bounded(self.zoom, 24., 280., 96.);
        self.timeline_split = bounded(self.timeline_split, 0.35, 0.7, 0.54);
        self.music_volume = bounded(self.music_volume, 0., 1., 1.);
        self.hitsound_volume = bounded(self.hitsound_volume, 0., 1., 1.);
        if self.video.plan(60.).is_err() {
            self.video = Default::default();
        }
        self.options = self.options.normalized();
        self
    }
}
pub(crate) fn bounded(value: f32, min: f32, max: f32, default: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}
// Sound pack libraries contain bounded paths and rules, never embedded audio.
const MAX_PREFERENCES_BYTES: usize = 8 * 1024 * 1024;

fn path(root: &Path) -> Result<PathBuf, ChartError> {
    let path = root.canonicalize()?.join("editor-settings.json");
    match fs::symlink_metadata(&path) {
        Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
            return Err(ChartError::Invalid("Invalid editor settings file".into()));
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(path)
}
pub fn load_editor_preferences(root: &Path) -> Result<EditorPreferences, ChartError> {
    let path = path(root)?;
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(EditorPreferences::default());
        }
        Err(e) => return Err(e.into()),
    }
    .take(MAX_PREFERENCES_BYTES as u64 + 1);
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    if bytes.len() > MAX_PREFERENCES_BYTES {
        return Err(ChartError::Invalid("Editor settings exceed 8 MiB".into()));
    }
    let preferences: EditorPreferences = serde_json::from_slice(&bytes)?;
    if preferences.version != 1 {
        return Err(ChartError::Invalid(
            "Unsupported editor settings version".into(),
        ));
    }
    Ok(preferences.normalized())
}
pub fn save_editor_preferences(
    root: &Path,
    preferences: &EditorPreferences,
) -> Result<(), ChartError> {
    crate::storage::write_json(&path(root)?, &preferences.clone().normalized())
}
