use crate::{DocumentVersion, valid_id};
use serde::{Deserialize, Serialize};

pub const ACTION_BEFORE_HOOK: &str = "host.action.before";
pub const MAX_EDITOR_SEEK_BEAT: f64 = 1_000_000_000.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorTool {
    Select,
    Box,
    Erase,
    Tap,
    Hold,
    Flick,
    Drag,
    Event,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorSelectionScope {
    Notes,
    Events,
    Both,
}

/// Native actions reuse the editor's existing handlers and history boundaries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorAction {
    Undo,
    Redo,
    Copy,
    Cut,
    Paste {
        mirrored: bool,
        original_time: bool,
    },
    Duplicate,
    DeleteSelection,
    ClearSelection,
    SelectScope {
        scope: EditorSelectionScope,
        invert: bool,
    },
    SetTool {
        tool: EditorTool,
    },
    SetPlayback {
        playing: bool,
    },
    Seek {
        beat: f64,
    },
}
impl EditorAction {
    pub fn validate(&self) -> Result<(), String> {
        if let Self::Seek { beat } = self
            && (!beat.is_finite() || !(0. ..=MAX_EDITOR_SEEK_BEAT).contains(beat))
        {
            return Err("Editor seek beat is outside its supported range".into());
        }
        Ok(())
    }
    pub fn edits_document(&self) -> bool {
        matches!(
            self,
            Self::Undo
                | Self::Redo
                | Self::Cut
                | Self::Paste { .. }
                | Self::Duplicate
                | Self::DeleteSelection
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorSelectionCounts {
    pub notes: usize,
    pub events: usize,
    pub story: usize,
}

/// Metadata only. The source caller and operation identity are immutable.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionIntercept {
    pub action: EditorAction,
    pub version: DocumentVersion,
    pub source: Option<String>,
    pub line: usize,
    pub beat: f64,
    pub playing: bool,
    pub selection: EditorSelectionCounts,
}
impl ActionIntercept {
    pub fn validate(&self) -> Result<(), String> {
        self.action.validate()?;
        if !self.beat.is_finite() || self.source.as_deref().is_some_and(|id| !valid_id(id)) {
            return Err("Invalid editor action context".into());
        }
        Ok(())
    }
    pub fn validate_replacement(&self, original: &Self) -> Result<(), String> {
        self.validate()?;
        if std::mem::discriminant(&self.action) != std::mem::discriminant(&original.action)
            || self.version != original.version
            || self.source != original.source
            || self.line != original.line
            || self.beat != original.beat
            || self.playing != original.playing
            || self.selection != original.selection
        {
            return Err("Action hooks cannot change the action kind or editor context".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorActionRequest {
    pub version: DocumentVersion,
    pub action: EditorAction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorStatusQuery {
    pub version: DocumentVersion,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorStatus {
    pub version: DocumentVersion,
    pub line: usize,
    pub beat: f64,
    pub playing: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub selection: EditorSelectionCounts,
    pub clipboard_available: bool,
    pub tool: EditorTool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacements_only_change_parameters_of_the_same_action() {
        let original = ActionIntercept {
            action: EditorAction::Seek { beat: 3.5 },
            version: DocumentVersion {
                session: 1,
                revision: 2,
            },
            source: Some("test.caller".into()),
            line: 0,
            beat: 1.,
            playing: false,
            selection: EditorSelectionCounts::default(),
        };
        let mut replacement = original.clone();
        replacement.action = EditorAction::Seek { beat: 4. };
        replacement.validate_replacement(&original).unwrap();
        for beat in [-1., f64::INFINITY, f64::NAN, MAX_EDITOR_SEEK_BEAT + 1.] {
            replacement.action = EditorAction::Seek { beat };
            assert!(replacement.validate_replacement(&original).is_err());
        }
        replacement = original.clone();
        replacement.source = None;
        assert!(replacement.validate_replacement(&original).is_err());
        replacement = original.clone();
        replacement.version.revision += 1;
        assert!(replacement.validate_replacement(&original).is_err());
        for (before, after) in [
            (EditorAction::Copy, EditorAction::Cut),
            (EditorAction::Undo, EditorAction::Redo),
        ] {
            let before = ActionIntercept {
                action: before,
                ..original.clone()
            };
            let after = ActionIntercept {
                action: after,
                ..before.clone()
            };
            assert!(after.validate_replacement(&before).is_err());
        }
        let value = serde_json::to_value(&original).unwrap();
        assert_eq!(
            serde_json::from_value::<ActionIntercept>(value).unwrap(),
            original
        );
    }
}
