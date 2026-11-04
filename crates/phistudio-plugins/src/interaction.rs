//! A host-owned gesture lifetime survives callbacks and service continuations.
use crate::{Origin, Runtime};
use phistudio_sdk::*;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone, Debug)]
pub struct Interaction {
    data: Arc<InteractionData>,
}
#[derive(Debug)]
struct InteractionData {
    origin: Origin,
    tool: String,
    version: DocumentVersion,
    surface: InputSurface,
    id: u64,
    state: Mutex<InteractionState>,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct InteractionState {
    started: bool,
    pub ended: bool,
    cancel_sent: bool,
    pub cancelled: bool,
    pub committed: bool,
    pointer: Option<InputPointer>,
    start: Option<InputPoint>,
}
impl PartialEq for Interaction {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
    }
}
impl Eq for Interaction {}
impl Interaction {
    pub fn id(&self) -> u64 {
        self.data.id
    }
    pub fn origin(&self) -> &Origin {
        &self.data.origin
    }
    pub fn version(&self) -> DocumentVersion {
        self.data.version
    }
    pub fn cancel(&self) {
        if let Ok(mut state) = self.data.state.lock() {
            state.cancelled = true;
        }
    }
    pub fn is_active(&self) -> bool {
        self.data.state.lock().is_ok_and(|s| !s.cancelled)
    }
    pub fn committed(&self) -> bool {
        self.data.state.lock().is_ok_and(|s| s.committed)
    }
    pub(crate) fn tool(&self) -> &str {
        &self.data.tool
    }
    pub(crate) fn lock(&self) -> Result<MutexGuard<'_, InteractionState>, String> {
        self.data
            .state
            .lock()
            .map_err(|_| "Gesture state interrupted".into())
    }
    pub(crate) fn admit(
        &self,
        input: &InputBatch,
        context: &EditorContext,
    ) -> Result<(MutexGuard<'_, InteractionState>, InteractionState), String> {
        input.validate()?;
        if input.gesture != self.data.id
            || input.surface != self.data.surface
            || context.version != self.data.version
        {
            return Err("Input batch does not match its gesture scope".into());
        }
        let state = self.lock()?;
        let mut next = state.clone();
        if input.phase() == Some(InputPhase::Cancel) {
            if input.events.len() != 1 || next.cancel_sent {
                return Err("Gesture cancellation is already queued or malformed".into());
            }
            next.cancelled = true;
            next.cancel_sent = true;
        } else {
            if next.cancelled {
                return Err("Gesture was cancelled".into());
            }
            for event in &input.events {
                match event.phase {
                    InputPhase::Begin if !next.started && !next.ended => {
                        next.started = true;
                        next.pointer = Some(input.pointer);
                        next.start = Some(input.start.clone());
                    }
                    InputPhase::Update if next.started && !next.ended => {}
                    InputPhase::End if next.started && !next.ended => next.ended = true,
                    _ => return Err("Input phases do not match the gesture lifecycle".into()),
                }
            }
            if next.pointer != Some(input.pointer) || next.start.as_ref() != Some(&input.start) {
                return Err("Input pointer or start position changed during a gesture".into());
            }
        }
        Ok((state, next))
    }
}
impl Runtime {
    pub fn begin_interaction(
        &mut self,
        origin: &Origin,
        tool: &str,
        version: DocumentVersion,
        surface: InputSurface,
    ) -> Result<Interaction, String> {
        surface.validate()?;
        let registry = self
            .registry
            .lock()
            .map_err(|_| "Plugin registry interrupted")?;
        let info = registry
            .current(origin)
            .ok_or("Input tool is no longer active")?;
        if !info.granted.contains(&Capability::Input) || !info.contributions.iter().any(|c|
            matches!(c,Contribution::Tool {id,surfaces,..} if id==tool && surfaces.contains(&surface.kind))) {
            return Err("Input tool or surface is not registered".into());
        }
        if surface.kind == InputSurfaceKind::Track
            && !info.contributions.iter().any(
                |c| matches!(c, Contribution::Track { id, .. } | Contribution::TimelineTrack { id, .. } if Some(id) == surface.id.as_ref()),
            )
        {
            return Err("Input track is not owned by the tool's plugin".into());
        }
        let id = self.next_interaction;
        self.next_interaction = self
            .next_interaction
            .checked_add(1)
            .ok_or("Gesture sequence exhausted")?;
        Ok(Interaction {
            data: Arc::new(InteractionData {
                origin: origin.clone(),
                tool: tool.into(),
                version,
                surface,
                id,
                state: Mutex::new(InteractionState::default()),
            }),
        })
    }
}
