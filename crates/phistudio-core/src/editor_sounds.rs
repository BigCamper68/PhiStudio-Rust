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
    PlaceTap,
    PlaceHold,
    PlaceFlick,
    PlaceDrag,
    PlaceEvent,
}
impl EditorSoundEvent {
    pub const ALL: [Self; 24] = [
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
        Self::PlaceTap,
        Self::PlaceHold,
        Self::PlaceFlick,
        Self::PlaceDrag,
        Self::PlaceEvent,
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
            Self::PlaceTap => "Place Tap notes",
            Self::PlaceHold => "Place Hold notes",
            Self::PlaceFlick => "Place Flick notes",
            Self::PlaceDrag => "Place Drag notes",
            Self::PlaceEvent => "Place line events",
        }
    }
    pub fn default_tone(self) -> EditorSoundTone {
        match self {
            Self::Click | Self::Value | Self::Seek => EditorSoundTone::Tick,
            Self::Toggle
            | Self::Select
            | Self::Place
            | Self::Tool
            | Self::PlaceTap
            | Self::PlaceHold
            | Self::PlaceFlick
            | Self::PlaceDrag
            | Self::PlaceEvent => EditorSoundTone::Pop,
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
/// Action groups also own independent repeat limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorSoundCategory {
    Interface,
    Objects,
    Navigation,
    History,
}
impl EditorSoundCategory {
    pub const ALL: [Self; 4] = [
        Self::Interface,
        Self::Objects,
        Self::Navigation,
        Self::History,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Interface => "Interface",
            Self::Objects => "Notes and objects",
            Self::Navigation => "Navigation and playback",
            Self::History => "History and files",
        }
    }
}
impl EditorSoundEvent {
    pub fn category(self) -> EditorSoundCategory {
        use EditorSoundCategory as C;
        match self {
            Self::Click | Self::Toggle | Self::Value | Self::Expand => C::Interface,
            Self::Select
            | Self::Deselect
            | Self::Edit
            | Self::Place
            | Self::Delete
            | Self::MoveResize
            | Self::PlaceTap
            | Self::PlaceHold
            | Self::PlaceFlick
            | Self::PlaceDrag
            | Self::PlaceEvent => C::Objects,
            Self::Tool | Self::Seek | Self::Playback => C::Navigation,
            Self::Copy | Self::Paste | Self::Undo | Self::Redo | Self::Save | Self::Error => {
                C::History
            }
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
    // Accepted for migration from the original single-file preferences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub files: Vec<String>,
    pub random: bool,
}
impl Default for EditorSoundRule {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 1.,
            pitch: 1.,
            tone: None,
            file: None,
            files: Vec::new(),
            random: true,
        }
    }
}
impl EditorSoundRule {
    pub fn paths(&self) -> impl Iterator<Item = &String> {
        self.file.iter().chain(self.files.iter())
    }
    fn normalize(&mut self) {
        self.volume = super::preferences::bounded(self.volume, 0., 1., 1.);
        self.pitch = super::preferences::bounded(self.pitch, 0.5, 2., 1.);
        if let Some(file) = self.file.take() {
            self.files.insert(0, file);
        }
        self.files
            .retain(|p| !p.trim().is_empty() && p.len() <= 1024);
        let mut unique = std::collections::BTreeSet::new();
        self.files.retain(|p| unique.insert(p.clone()));
        self.files.truncate(4);
    }
}
/// One portable palette. Master enable/volume remain installation preferences.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorSoundPack {
    pub id: String,
    pub name: String,
    pub style: EditorSoundStyle,
    pub pitch: f32,
    pub cooldown_ms: u32,
    pub category_intervals: BTreeMap<EditorSoundCategory, u32>,
    pub events: BTreeMap<EditorSoundEvent, EditorSoundRule>,
}
impl Default for EditorSoundPack {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "My sounds".into(),
            style: EditorSoundStyle::Soft,
            pitch: 1.,
            cooldown_ms: 40,
            category_intervals: BTreeMap::new(),
            events: BTreeMap::new(),
        }
    }
}
impl EditorSoundPack {
    pub fn builtin(style: EditorSoundStyle) -> Self {
        Self {
            id: format!("builtin-{}", style.label().to_ascii_lowercase()),
            name: style.label().into(),
            style,
            ..Default::default()
        }
    }
    pub fn is_builtin_id(id: &str) -> bool {
        EditorSoundStyle::ALL
            .into_iter()
            .any(|s| Self::builtin(s).id == id)
    }
    pub fn rule(&self, event: EditorSoundEvent) -> EditorSoundRule {
        self.events
            .get(&event)
            .or_else(|| {
                matches!(
                    event,
                    EditorSoundEvent::PlaceTap
                        | EditorSoundEvent::PlaceHold
                        | EditorSoundEvent::PlaceFlick
                        | EditorSoundEvent::PlaceDrag
                        | EditorSoundEvent::PlaceEvent
                )
                .then(|| self.events.get(&EditorSoundEvent::Place))
                .flatten()
            })
            .cloned()
            .unwrap_or_default()
    }
    pub fn interval(&self, event: EditorSoundEvent) -> u32 {
        self.category_intervals
            .get(&event.category())
            .copied()
            .unwrap_or(self.cooldown_ms)
            .max(
                if matches!(event, EditorSoundEvent::Value | EditorSoundEvent::Seek) {
                    90
                } else {
                    20
                },
            )
    }
    pub fn normalized(mut self) -> Self {
        self.name = self
            .name
            .trim()
            .chars()
            .filter(|c| !c.is_control())
            .take(64)
            .collect();
        if self.name.is_empty() {
            self.name = "My sounds".into();
        }
        self.pitch = super::preferences::bounded(self.pitch, 0.5, 2., 1.);
        self.cooldown_ms = self.cooldown_ms.clamp(20, 500);
        for interval in self.category_intervals.values_mut() {
            *interval = (*interval).clamp(20, 500);
        }
        for rule in self.events.values_mut() {
            rule.normalize();
        }
        self
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "SavedSounds")]
pub struct EditorSoundOptions {
    pub enabled: bool,
    pub volume: f32,
    pub mute_during_playback: bool,
    pub selected_pack: String,
    pub packs: Vec<EditorSoundPack>,
}
impl Default for EditorSoundOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 0.35,
            mute_during_playback: true,
            selected_pack: EditorSoundPack::builtin(EditorSoundStyle::Soft).id,
            packs: Vec::new(),
        }
    }
}
impl EditorSoundOptions {
    pub const MAX_PACKS: usize = 32;
    pub fn pack(&self) -> std::borrow::Cow<'_, EditorSoundPack> {
        if let Some(pack) = self.packs.iter().find(|p| p.id == self.selected_pack) {
            return std::borrow::Cow::Borrowed(pack);
        }
        let style = EditorSoundStyle::ALL
            .into_iter()
            .find(|s| EditorSoundPack::builtin(*s).id == self.selected_pack)
            .unwrap_or_default();
        std::borrow::Cow::Owned(EditorSoundPack::builtin(style))
    }
    pub fn rule(&self, event: EditorSoundEvent) -> EditorSoundRule {
        self.pack().rule(event)
    }
    /// Give every copy/import a fresh local identity, regardless of its source ID.
    pub fn add_pack(&mut self, mut pack: EditorSoundPack) -> bool {
        if self.packs.len() >= Self::MAX_PACKS {
            return false;
        }
        let mut serial = 1;
        while self
            .packs
            .iter()
            .any(|p| p.id == format!("custom-{serial}"))
        {
            serial += 1;
        }
        pack.id = format!("custom-{serial}");
        self.selected_pack = pack.id.clone();
        self.packs.push(pack.normalized());
        true
    }
    pub fn selected_pack_mut(&mut self) -> Option<&mut EditorSoundPack> {
        self.packs.iter_mut().find(|p| p.id == self.selected_pack)
    }
    pub fn normalized(mut self) -> Self {
        self.volume = super::preferences::bounded(self.volume, 0., 1., 0.35);
        self.packs.truncate(Self::MAX_PACKS);
        let mut ids = std::collections::BTreeSet::new();
        self.packs.retain(|p| {
            !p.id.is_empty()
                && p.id.len() <= 128
                && !EditorSoundPack::is_builtin_id(&p.id)
                && ids.insert(p.id.clone())
        });
        self.packs = self
            .packs
            .into_iter()
            .map(EditorSoundPack::normalized)
            .collect();
        if !EditorSoundPack::is_builtin_id(&self.selected_pack)
            && !self.packs.iter().any(|p| p.id == self.selected_pack)
        {
            self.selected_pack = EditorSoundPack::builtin(EditorSoundStyle::Soft).id;
        }
        self
    }
}
// Missing `packs` identifies the original schema; its complete action map becomes
// one custom pack so upgrading never discards individual sound assignments.
#[derive(Deserialize)]
#[serde(default)]
struct SavedSounds {
    enabled: bool,
    volume: f32,
    mute_during_playback: bool,
    selected_pack: Option<String>,
    packs: Option<Vec<EditorSoundPack>>,
    style: EditorSoundStyle,
    pitch: f32,
    cooldown_ms: u32,
    events: BTreeMap<EditorSoundEvent, EditorSoundRule>,
}
impl Default for SavedSounds {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 0.35,
            mute_during_playback: true,
            selected_pack: None,
            packs: None,
            style: EditorSoundStyle::Soft,
            pitch: 1.,
            cooldown_ms: 40,
            events: BTreeMap::new(),
        }
    }
}
impl From<SavedSounds> for EditorSoundOptions {
    fn from(saved: SavedSounds) -> Self {
        let mut options = Self {
            enabled: saved.enabled,
            volume: saved.volume,
            mute_during_playback: saved.mute_during_playback,
            selected_pack: saved
                .selected_pack
                .unwrap_or_else(|| EditorSoundPack::builtin(saved.style).id),
            packs: saved.packs.clone().unwrap_or_default(),
        };
        if saved.packs.is_none()
            && (!saved.events.is_empty() || saved.pitch != 1. || saved.cooldown_ms != 40)
        {
            options.add_pack(EditorSoundPack {
                name: "My sounds".into(),
                style: saved.style,
                pitch: saved.pitch,
                cooldown_ms: saved.cooldown_ms,
                events: saved.events,
                ..Default::default()
            });
        }
        options.normalized()
    }
}
#[cfg(test)]
mod tests;
