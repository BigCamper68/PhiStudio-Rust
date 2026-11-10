//! Editor feedback preferences, kept separate from chart and gameplay audio.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorSoundEvent {
    Click,
    Toggle,
    Value,
    Expand,
    Select,
    Deselect,
    Tool,
    Seek,
    Playback,
    Edit,
    Place,
    Delete,
    Copy,
    Paste,
    Undo,
    Redo,
    Save,
    Error,
    MoveResize,
}
impl EditorSoundEvent {
    pub const ALL: [Self; 19] = [
        Self::Click,
        Self::Toggle,
        Self::Value,
        Self::Expand,
        Self::Select,
        Self::Deselect,
        Self::Tool,
        Self::Seek,
        Self::Playback,
        Self::Edit,
        Self::Place,
        Self::Delete,
        Self::Copy,
        Self::Paste,
        Self::Undo,
        Self::Redo,
        Self::Save,
        Self::Error,
        Self::MoveResize,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Click => "Buttons",
            Self::Toggle => "Toggles",
            Self::Value => "Value changes",
            Self::Expand => "Menus and sections",
            Self::Select => "Select objects",
            Self::Deselect => "Deselect objects",
            Self::Tool => "Switch tools",
            Self::Seek => "Seek and scrub",
            Self::Playback => "Play and pause",
            Self::Edit => "Edit objects",
            Self::Place => "Place objects",
            Self::Delete => "Delete objects",
            Self::Copy => "Copy objects",
            Self::Paste => "Paste objects",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::Save => "Save",
            Self::Error => "Errors",
            Self::MoveResize => "Move and resize objects",
        }
    }
    pub fn default_tone(self) -> EditorSoundTone {
        match self {
            Self::Click | Self::Value | Self::Seek => EditorSoundTone::Tick,
            Self::Toggle | Self::Select | Self::Place | Self::Tool => EditorSoundTone::Pop,
            Self::Deselect | Self::Edit | Self::Copy => EditorSoundTone::Wood,
            Self::Expand | Self::Paste | Self::Undo | Self::Redo => EditorSoundTone::Glass,
            Self::Playback | Self::Save => EditorSoundTone::Chime,
            Self::Delete | Self::Error => EditorSoundTone::Bass,
            Self::MoveResize => EditorSoundTone::Slide,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorSoundStyle {
    #[default]
    Soft,
    Mechanical,
    Arcade,
}
impl EditorSoundStyle {
    pub const ALL: [Self; 3] = [Self::Soft, Self::Mechanical, Self::Arcade];
    pub fn label(self) -> &'static str {
        match self {
            Self::Soft => "Soft",
            Self::Mechanical => "Mechanical",
            Self::Arcade => "Arcade",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorSoundTone {
    Tick,
    Pop,
    Wood,
    Glass,
    Chime,
    Bass,
    Slide,
}
impl EditorSoundTone {
    pub const ALL: [Self; 7] = [
        Self::Tick,
        Self::Pop,
        Self::Wood,
        Self::Glass,
        Self::Chime,
        Self::Bass,
        Self::Slide,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Tick => "Tick",
            Self::Pop => "Pop",
            Self::Wood => "Wood",
            Self::Glass => "Glass",
            Self::Chime => "Chime",
            Self::Bass => "Bass",
            Self::Slide => "Slide",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorSoundRule {
    pub enabled: bool,
    pub volume: f32,
    pub pitch: f32,
    pub tone: Option<EditorSoundTone>,
    pub file: Option<String>,
}
impl Default for EditorSoundRule {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 1.,
            pitch: 1.,
            tone: None,
            file: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorSoundOptions {
    pub enabled: bool,
    pub volume: f32,
    pub pitch: f32,
    pub style: EditorSoundStyle,
    pub mute_during_playback: bool,
    pub cooldown_ms: u32,
    pub events: BTreeMap<EditorSoundEvent, EditorSoundRule>,
}
impl Default for EditorSoundOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 0.35,
            pitch: 1.,
            style: EditorSoundStyle::Soft,
            mute_during_playback: true,
            cooldown_ms: 40,
            events: BTreeMap::new(),
        }
    }
}
impl EditorSoundOptions {
    pub fn rule(&self, event: EditorSoundEvent) -> EditorSoundRule {
        self.events.get(&event).cloned().unwrap_or_default()
    }
    pub fn normalized(mut self) -> Self {
        self.volume = super::preferences::bounded(self.volume, 0., 1., 0.35);
        self.pitch = super::preferences::bounded(self.pitch, 0.5, 2., 1.);
        self.cooldown_ms = self.cooldown_ms.clamp(20, 500);
        for rule in self.events.values_mut() {
            rule.volume = super::preferences::bounded(rule.volume, 0., 1., 1.);
            rule.pitch = super::preferences::bounded(rule.pitch, 0.5, 2., 1.);
            rule.file = rule
                .file
                .take()
                .filter(|p| !p.trim().is_empty() && p.len() <= 1024);
        }
        self
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_preferences_keep_defaults_and_sound_rules_round_trip() {
        let mut preferences: crate::EditorPreferences = serde_json::from_str("{}").unwrap();
        assert_eq!(
            preferences.options.editor_sounds,
            EditorSoundOptions::default()
        );
        preferences.options.editor_sounds.events.insert(
            EditorSoundEvent::Place,
            EditorSoundRule {
                file: Some("/private/sounds/place.wav".into()),
                volume: 0.6,
                pitch: 1.2,
                ..Default::default()
            },
        );
        preferences.options.editor_sounds.events.insert(
            EditorSoundEvent::MoveResize,
            EditorSoundRule {
                tone: Some(EditorSoundTone::Slide),
                volume: 0.7,
                ..Default::default()
            },
        );
        let root = tempfile::tempdir().unwrap();
        crate::save_editor_preferences(root.path(), &preferences).unwrap();
        assert_eq!(
            crate::load_editor_preferences(root.path()).unwrap(),
            preferences
        );
    }
    #[test]
    fn sound_preferences_bound_invalid_values() {
        let mut options = EditorSoundOptions {
            volume: f32::NAN,
            pitch: f32::INFINITY,
            cooldown_ms: 0,
            ..Default::default()
        };
        options.events.insert(
            EditorSoundEvent::Click,
            EditorSoundRule {
                volume: -1.,
                pitch: 100.,
                file: Some("x".repeat(1025)),
                ..Default::default()
            },
        );
        let options = options.normalized();
        assert_eq!(
            (options.volume, options.pitch, options.cooldown_ms),
            (0.35, 1., 20)
        );
        let rule = options.rule(EditorSoundEvent::Click);
        assert_eq!((rule.volume, rule.pitch, rule.file), (0., 2., None));
    }
}
