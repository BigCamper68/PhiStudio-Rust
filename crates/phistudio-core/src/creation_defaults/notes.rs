use crate::{ChartError, Note, NoteKind, NoteSoundGain, note_extras};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DefaultNoteSound {
    pub reference: String,
    pub project: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NoteCreationDefaults {
    pub above: u8,
    pub is_fake: u8,
    pub alpha: u16,
    pub size: f64,
    pub speed: f64,
    pub visible_time: f64,
    pub y_offset: f64,
    pub judge_area: Option<f64>,
    pub tint_hit_effects: Option<[u8; 3]>,
    pub volume: f64,
    pub sound: Option<DefaultNoteSound>,
}
impl Default for NoteCreationDefaults {
    fn default() -> Self {
        let note = Note::default();
        Self {
            above: note.above,
            is_fake: note.is_fake,
            alpha: note.alpha,
            size: note.size,
            speed: note.speed,
            visible_time: note.visible_time,
            y_offset: note.y_offset,
            judge_area: None,
            tint_hit_effects: None,
            volume: 1.,
            sound: None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteCreationPolicy {
    Regular,
    /// Audio generators expose their own explicit style controls.
    Audio,
    /// Text owns placement, size, timing, side and stationary fake-note behavior.
    Text,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoteCreationSounds {
    pub reference: Option<String>,
    pub gains: [Option<NoteSoundGain>; 3],
}
impl NoteCreationSounds {
    pub fn index(kind: NoteKind) -> usize {
        match kind {
            NoteKind::Tap | NoteKind::Hold => 0,
            NoteKind::Flick => 1,
            NoteKind::Drag => 2,
        }
    }
}
impl NoteCreationDefaults {
    pub fn normalized(mut self) -> Self {
        let defaults = Self::default();
        for (value, fallback) in [
            (&mut self.size, defaults.size),
            (&mut self.speed, defaults.speed),
            (&mut self.visible_time, defaults.visible_time),
            (&mut self.y_offset, defaults.y_offset),
        ] {
            *value = if value.is_finite() {
                value.clamp(-1e9, 1e9)
            } else {
                fallback
            };
        }
        self.above = u8::from(self.above == 1);
        self.is_fake = u8::from(self.is_fake != 0);
        self.judge_area = self
            .judge_area
            .filter(|v| v.is_finite() && *v >= 0. && (*v as f32).is_finite());
        self.volume = if self.volume.is_finite() {
            self.volume.clamp(0., 1.)
        } else {
            1.
        };
        self.sound = self.sound.and_then(|mut sound| {
            sound.reference = note_extras::validate_hitsound(&sound.reference).ok()?;
            if note_extras::builtin_hitsound(&sound.reference) {
                sound.project = None;
            } else if sound.project.as_ref().is_none_or(|p| !p.is_absolute()) {
                return None;
            }
            Some(sound)
        });
        self
    }
    pub fn validate(&self) -> Result<(), ChartError> {
        if &self.clone().normalized() != self {
            return Err(ChartError::Invalid(
                "New-note defaults contain invalid values".into(),
            ));
        }
        Ok(())
    }
    pub fn sound_reference(&self, project: Option<&Path>) -> Option<&str> {
        self.sound
            .as_ref()
            .filter(|s| s.project.is_none() || s.project.as_deref() == project)
            .map(|s| s.reference.as_str())
    }
    pub fn apply_style(&self, note: &mut Note, policy: NoteCreationPolicy) {
        if policy == NoteCreationPolicy::Regular {
            note.above = self.above;
            note.is_fake = self.is_fake;
            note.size = self.size;
            note.speed = self.speed;
            note.visible_time = self.visible_time;
            note.y_offset = self.y_offset;
            note.alpha = self.alpha;
        } else if policy == NoteCreationPolicy::Text {
            note.alpha = self.alpha;
        } else if policy == NoteCreationPolicy::Audio {
            note.visible_time = self.visible_time;
        }
        if policy == NoteCreationPolicy::Regular || note.judge_area.is_none() {
            note.judge_area = self.judge_area;
        }
        if policy == NoteCreationPolicy::Regular || note.tint_hit_effects.is_none() {
            note.tint_hit_effects = self.tint_hit_effects;
        }
    }
    pub fn decorate(
        &self,
        raw: &Value,
        sounds: &NoteCreationSounds,
        policy: NoteCreationPolicy,
    ) -> Result<Value, ChartError> {
        self.validate()?;
        let mut note: Note = serde_json::from_value(raw.clone())?;
        self.apply_style(&mut note, policy);
        let mut raw = note.merge_into(Some(raw))?;
        let object = raw
            .as_object_mut()
            .ok_or_else(|| ChartError::Invalid("Note must be an object".into()))?;
        object.shift_remove(note_extras::SOUND_GAIN_KEY);
        object.shift_remove("hitsound");
        if self.volume < 1. {
            let gain = sounds.gains[NoteCreationSounds::index(note.kind)]
                .as_ref()
                .ok_or_else(|| ChartError::Invalid("Default note sounds are not ready".into()))?;
            gain.validate()?;
            if gain.volume != self.volume || gain.source != sounds.reference {
                return Err(ChartError::Invalid(
                    "Default note sound preparation is out of date".into(),
                ));
            }
            object.insert("hitsound".into(), Value::String(gain.rendered.clone()));
            object.insert(
                note_extras::SOUND_GAIN_KEY.into(),
                serde_json::to_value(gain)?,
            );
        } else if let Some(reference) = &sounds.reference {
            object.insert(
                "hitsound".into(),
                Value::String(note_extras::validate_hitsound(reference)?),
            );
        }
        Ok(raw)
    }
}
