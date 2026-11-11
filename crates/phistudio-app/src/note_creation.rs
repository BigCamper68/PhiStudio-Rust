//! Stage fresh authoring with reusable native sounds and one history operation.
mod worker;
use crate::{files::FileRequest, workbench::Workbench};
use phistudio_core::{
    Beat, Note, NoteCreationDefaults, NoteCreationPolicy, NoteCreationSounds, NoteKind,
    tools::ToolEdit,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub(crate) enum Action {
    Tool {
        edit: ToolEdit,
        policy: NoteCreationPolicy,
        placement: Option<NoteKind>,
    },
    Text(Box<phistudio_core::text_composition::Plan>),
}
impl Action {
    fn count(&self) -> usize {
        match self {
            Self::Tool { edit, .. } => edit.added_note_count(),
            Self::Text(plan) => plan.notes,
        }
    }
    fn prepare(
        self,
        defaults: &NoteCreationDefaults,
        sounds: &NoteCreationSounds,
        canceled: impl Fn() -> bool,
    ) -> Result<Self, String> {
        match self {
            Self::Tool {
                edit,
                policy,
                placement,
            } => Ok(Self::Tool {
                edit: edit
                    .with_note_defaults(defaults, sounds, policy, canceled)
                    .map_err(|e| e.to_string())?,
                policy,
                placement,
            }),
            Self::Text(plan) => Ok(Self::Text(Box::new(
                (*plan)
                    .with_note_defaults(defaults, sounds, canceled)
                    .map_err(|e| e.to_string())?,
            ))),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SoundKey {
    session: u64,
    assets: u64,
    pub(crate) root: Option<PathBuf>,
    pub(crate) skin: Option<PathBuf>,
    pub(crate) reference: Option<String>,
    pub(crate) volume: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Target {
    id: u64,
    revision: u64,
    key: SoundKey,
    defaults: NoteCreationDefaults,
}
struct Pending {
    target: Target,
    cancel: Arc<AtomicBool>,
}
#[derive(Default)]
pub(crate) struct State {
    ready: Option<(SoundKey, NoteCreationSounds)>,
    pending: Option<Pending>,
    next_id: u64,
}
impl State {
    pub fn clear_project(&mut self) {
        if let Some(pending) = self.pending.take() {
            pending.cancel.store(true, Ordering::Relaxed);
        }
        self.ready = None;
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.clear_project();
    }
}
pub(crate) struct Request {
    pub target: Target,
    action: Action,
    cached: Option<NoteCreationSounds>,
    cancel: Arc<AtomicBool>,
}
pub(crate) struct Prepared {
    pub action: Action,
    pub sounds: NoteCreationSounds,
}
impl Workbench {
    pub(crate) fn create_tool_notes(
        &mut self,
        edit: Result<ToolEdit, phistudio_core::ChartError>,
        policy: NoteCreationPolicy,
    ) -> bool {
        match edit {
            Ok(edit) => self.create_notes(Action::Tool {
                edit,
                policy,
                placement: None,
            }),
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }
    pub(crate) fn note_default_scope(&self) -> Option<PathBuf> {
        self.current_workspace()
            .or_else(|| self.asset_root().map(ToOwned::to_owned))
    }
    pub(crate) fn note_creation_key(&self) -> Result<SoundKey, String> {
        let defaults = &self.options.new_note_defaults;
        defaults.validate().map_err(|e| e.to_string())?;
        let reference = defaults
            .sound_reference(self.note_default_scope().as_deref())
            .map(str::to_owned);
        let skin = self
            .options
            .skin
            .as_ref()
            .and_then(|id| self.skins.entries.iter().find(|s| &s.id == id))
            .map(|s| s.assets.clone());
        if defaults.volume < 1. && self.options.skin.is_some() && skin.is_none() {
            return Err("The selected note skin is not ready".into());
        }
        Ok(SoundKey {
            session: self.session,
            assets: self.asset_revision,
            root: self.asset_root().map(ToOwned::to_owned),
            skin,
            reference,
            volume: defaults.volume,
        })
    }
    pub(crate) fn place_note_with_defaults(
        &mut self,
        line: usize,
        kind: NoteKind,
        start: Beat,
        end: Beat,
        x: f64,
    ) {
        let note = Note {
            kind,
            start_time: start,
            end_time: end,
            x,
            ..Default::default()
        };
        match phistudio_core::tools::create_note(self.editor.document(), line, note) {
            Ok(edit) => {
                self.create_notes(Action::Tool {
                    edit,
                    policy: NoteCreationPolicy::Regular,
                    placement: Some(kind),
                });
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    pub(crate) fn create_notes(&mut self, action: Action) -> bool {
        if self.note_creation.pending.is_some() || self.files.busy || self.files.request.is_some() {
            self.error = Some("Wait for the current file operation before creating notes".into());
            return false;
        }
        if action.count() == 0 {
            return self.apply_created_notes(action);
        }
        let key = match self.note_creation_key() {
            Ok(key) => key,
            Err(error) => {
                self.error = Some(error);
                return false;
            }
        };
        let cached = if key.volume == 1.
            && key
                .reference
                .as_ref()
                .is_none_or(|r| phistudio_core::builtin_hitsound(r))
        {
            Some(NoteCreationSounds {
                reference: key.reference.clone(),
                ..Default::default()
            })
        } else {
            self.note_creation
                .ready
                .as_ref()
                .filter(|(old, _)| old == &key)
                .map(|(_, sounds)| sounds.clone())
        };
        if let Some(sounds) = &cached
            && action.count() <= 128
        {
            return match action.prepare(&self.options.new_note_defaults, sounds, || false) {
                Ok(action) => self.apply_created_notes(action),
                Err(error) => {
                    self.error = Some(error);
                    false
                }
            };
        }
        if cached.is_none() && key.root.is_none() {
            self.error =
                Some("Save this chart as a project before changing note sound volume".into());
            return false;
        }
        self.note_creation.next_id = self.note_creation.next_id.wrapping_add(1);
        let target = Target {
            id: self.note_creation.next_id,
            revision: self.editor.revision(),
            key,
            defaults: self.options.new_note_defaults.clone(),
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.note_creation.pending = Some(Pending {
            target: target.clone(),
            cancel: cancel.clone(),
        });
        self.files.request = Some(FileRequest::PrepareNoteCreation(Box::new(Request {
            target,
            action,
            cached,
            cancel,
        })));
        self.transport.playing = false;
        self.error = None;
        true
    }
    pub(crate) fn receive_note_creation(
        &mut self,
        target: Target,
        result: Result<Prepared, String>,
    ) {
        if self
            .note_creation
            .pending
            .as_ref()
            .is_none_or(|p| p.target != target)
        {
            return;
        }
        let Some(pending) = self.note_creation.pending.take() else {
            return;
        };
        if pending.cancel.load(Ordering::Relaxed)
            || self.show_projects
            || self.close_requested
            || self.editor.revision() != target.revision
            || self.options.new_note_defaults != target.defaults
            || self.note_creation_key().as_ref().ok() != Some(&target.key)
        {
            return;
        }
        match result {
            Ok(prepared) => {
                self.note_creation.ready = Some((target.key, prepared.sounds));
                self.apply_created_notes(prepared.action);
            }
            Err(error) => self.error = Some(error),
        }
    }
    fn apply_created_notes(&mut self, action: Action) -> bool {
        match action {
            Action::Tool {
                edit, placement, ..
            } => {
                let applied = self.apply_tool_with_memory(Ok(edit), placement.is_none());
                if applied && let Some(kind) = placement {
                    let event = match kind {
                        NoteKind::Tap => phistudio_core::EditorSoundEvent::PlaceTap,
                        NoteKind::Hold => phistudio_core::EditorSoundEvent::PlaceHold,
                        NoteKind::Flick => phistudio_core::EditorSoundEvent::PlaceFlick,
                        NoteKind::Drag => phistudio_core::EditorSoundEvent::PlaceDrag,
                    };
                    self.sounds.emit(event);
                }
                applied
            }
            Action::Text(plan) => {
                let anchor = self.anchor();
                let line = self.editor.document().line_count();
                match plan.apply(&mut self.editor) {
                    Ok(selection) => {
                        self.remember_anchor(anchor);
                        self.line = line;
                        self.transport.playing = false;
                        self.set_selection(selection);
                        self.error = None;
                        true
                    }
                    Err(error) => {
                        self.error = Some(error.to_string());
                        false
                    }
                }
            }
        }
    }
}
pub(crate) use worker::prepare;
#[cfg(test)]
mod tests;
