//! Editable chart data and commands, independent of graphics and mobile APIs.
mod advanced;
mod array_edit;
mod formula;
pub mod shaders;
pub mod skins;
pub mod vcs;
pub use advanced::{ChartMetadata, StoryEvent, StoryKind};
pub use formula::Formula;
mod authoring;
mod beat;
mod conversion;
mod document;
mod events;
mod export;
mod groups;
mod history;
mod json_io;
mod library;
pub mod line_management;
mod line_properties;
mod note_extras;
pub mod objects;
mod package;
pub mod patterns;
mod preferences;
mod project_assets;
mod project_management;
mod project_options;
mod recovery;
pub mod resources;
mod storage;
pub mod story_tools;
mod timing;
pub mod tools;
pub mod trajectories;
pub mod versions;
pub mod video;
mod xy;

pub use authoring::EditTarget;
pub use beat::Beat;
pub use conversion::{ChartFormat, DecodedChart, decode_chart, pec_height_signature};
pub use document::{ChartDocument, ChartError, LineSummary, Note, NoteKind};
pub use events::{EventId, EventType, LineEvent};
pub use export::{ProjectExport, prepare_project_export};
pub use groups::GroupAction;
pub use history::{Command, Editor, NoteId};
pub use library::{
    ProjectLibrary, ProjectSummary, create_project, list_projects, open_project,
    refresh_project_record, rename_project,
};
pub use line_properties::{AttachedUi, LineProperties, rotation_inheritance};
pub use note_extras::{
    ControlKind, ControlPoint, NoteSoundGain, SOUND_GAIN_KEY, builtin_hitsound, validate_hitsound,
};
pub use package::{
    ChartChoice, ImportedProject, import_project, load_project_options,
    load_project_options_with_assets,
};
pub use preferences::{
    AudioGuideOptions, EditorOptions, EditorPreferences, Language, MultiLinePreferences,
    NumericDragOptions, PreviewAspect, ThemeColors, load_editor_preferences,
    save_editor_preferences,
};
pub use project_assets::{
    MAX_HITSOUND_BYTES, hitsound_path, import_hitsound_asset, list_hitsound_assets,
    write_generated_hitsound,
};
pub use project_management::{
    NewProjectMedia, TrashedProject, create_project_with_media, restore_project, trash_project,
};
pub use project_options::ProjectOptions;
pub use recovery::{RecoverySnapshot, clear_recovery, load_recovery, save_recovery};
pub use storage::{ProjectLocation, last_project, load_chart, remember_project, save_chart};
pub use timing::{BpmChange, TempoMap};
pub use xy::linked_selection;

pub mod tool_presets;

pub mod clipboard_history;

pub mod text_composition;

pub mod branches;
