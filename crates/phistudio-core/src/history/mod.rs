mod delta;
mod execute;
mod navigation;
use crate::{
    BpmChange, ChartDocument, ChartError, EditTarget, EventId, EventType, LineEvent,
    LineProperties, Note,
};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct NoteId {
    pub line: usize,
    pub index: usize,
}

#[derive(Clone, Debug)]
pub enum Command {
    EditShader {
        baseline: Value,
        action: crate::shaders::ShaderAction,
    },
    SetShaderSource {
        path: String,
        source: String,
    },
    EraseStoryboard(Vec<(usize, crate::StoryKind, usize)>),
    EditMetadata(crate::ChartMetadata),
    EditStoryboard {
        line: usize,
        kind: crate::StoryKind,
        index: Option<usize>,
        event: Option<crate::StoryEvent>,
    },
    SetNoteHitsound {
        ids: Vec<NoteId>,
        reference: Option<String>,
    },
    SetNoteVolume {
        id: NoteId,
        gain: Option<crate::NoteSoundGain>,
    },
    SetControlCurve {
        line: usize,
        kind: crate::ControlKind,
        points: Option<Vec<crate::ControlPoint>>,
    },
    EditGroups(crate::GroupAction),
    /// Clipboard/tool insertion retains every unknown field of the source.
    InsertNote {
        line: usize,
        value: Value,
    },
    InsertEvent {
        line: usize,
        layer: usize,
        kind: EventType,
        value: Value,
    },
    AddNote {
        line: usize,
        note: Note,
    },
    UpdateNote {
        id: NoteId,
        note: Note,
    },
    DeleteNote(NoteId),
    AddLine {
        name: String,
    },
    AddLines {
        count: usize,
    },
    DuplicateLine(usize),
    MoveLine {
        from: usize,
        to: usize,
    },
    RenameLine {
        line: usize,
        name: String,
    },
    RenameChart(String),
    AddEvent {
        line: usize,
        layer: usize,
        kind: EventType,
        event: LineEvent,
    },
    UpdateEvent {
        id: EventId,
        event: LineEvent,
    },
    DeleteEvent(EventId),
    AddBpm(BpmChange),
    UpdateBpm {
        index: usize,
        change: BpmChange,
    },
    DeleteBpm(usize),
    EditLine {
        index: usize,
        properties: LineProperties,
    },
    DeleteLine(usize),
}

#[derive(Debug)]
enum NotesShape {
    Missing,
    Null,
    Array,
}

#[derive(Debug)]
enum Delta {
    Lines(crate::line_management::LineDelta),
    LineField {
        line: usize,
        key: &'static str,
        before: Option<Value>,
        after: Option<Value>,
    },
    Shader(crate::shaders::ShaderDelta),
    Advanced(crate::advanced::AdvancedDelta),
    Extras(crate::note_extras::ExtraDelta),
    Groups(crate::groups::GroupDelta),
    Group(Vec<Delta>),
    Tools(crate::tools::ToolDelta),
    Authoring(crate::authoring::AuthoringDelta),
    Note {
        id: NoteId,
        before: Option<Value>,
        after: Option<Value>,
        notes_shape: NotesShape,
        count_before: Option<Value>,
        count_after: Value,
    },
    Line {
        index: usize,
        value: Value,
    },
    LineName {
        index: usize,
        before: Option<Value>,
        after: Value,
    },
    Title {
        before: Option<Value>,
        after: Value,
    },
}

#[derive(Debug)]
struct Entry {
    delta: Delta,
    label: &'static str,
    before: u64,
    after: u64,
}

/// History stores changed notes/fields, not a full chart snapshot per action.
/// Saved-state identity survives undo, redo and branching correctly.
#[derive(Debug)]
pub struct Editor {
    document: ChartDocument,
    history: Vec<Entry>,
    cursor: usize,
    state: u64,
    saved_state: u64,
    next_state: u64,
    revision: u64,
}

impl Editor {
    /// A recovered snapshot has no undo history, but still needs an explicit save.
    pub fn recovered(document: ChartDocument) -> Self {
        let mut editor = Self::new(document);
        editor.state = 1;
        editor.next_state = 2;
        editor
    }

    pub fn new(document: ChartDocument) -> Self {
        Self {
            document,
            history: Vec::new(),
            cursor: 0,
            state: 0,
            saved_state: 0,
            next_state: 1,
            revision: 0,
        }
    }
    pub fn document(&self) -> &ChartDocument {
        &self.document
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn is_dirty(&self) -> bool {
        self.state != self.saved_state
    }
    pub fn mark_saved(&mut self) {
        self.saved_state = self.state;
    }
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }
    pub fn can_redo(&self) -> bool {
        self.cursor < self.history.len()
    }
    pub fn current_state_id(&self) -> u64 {
        self.state
    }
    pub fn action_id(&self, redo: bool) -> Option<u64> {
        self.history
            .get(if redo {
                self.cursor
            } else {
                self.cursor.checked_sub(1)?
            })
            .map(|e| e.after)
    }
    pub fn history_rows(&self) -> Vec<(&str, bool)> {
        self.history
            .iter()
            .enumerate()
            .map(|(i, e)| (e.label, i < self.cursor))
            .collect()
    }
    pub fn history_cursor(&self) -> usize {
        self.cursor
    }
    pub fn undo_label(&self) -> Option<&str> {
        self.cursor.checked_sub(1).map(|i| self.history[i].label)
    }
    pub fn last_target(&self) -> Option<EditTarget> {
        self.cursor
            .checked_sub(1)
            .and_then(|i| match &self.history[i].delta {
                Delta::Authoring(d) => d.target(),
                _ => None,
            })
    }
}
