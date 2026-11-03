//! A driver may cancel a hook and every service continuation derived from it.
use super::*;
use std::sync::MutexGuard;

#[derive(Clone)]
pub struct Invocation {
    data: Arc<Data>,
}
struct Data {
    registry: Arc<Mutex<Registry>>,
    version: DocumentVersion,
    state: Mutex<State>,
}
pub(super) struct State {
    active: bool,
    pub(super) started: bool,
}
impl std::fmt::Debug for Invocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Invocation")
            .field("version", &self.version())
            .finish()
    }
}
impl PartialEq for Invocation {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
    }
}
impl Eq for Invocation {}
impl Invocation {
    pub fn version(&self) -> DocumentVersion {
        self.data.version
    }
    pub fn cancel(&self) {
        if let Ok(mut state) = self.data.state.lock() {
            state.active = false;
        }
    }
    pub fn is_active(&self) -> bool {
        self.data.state.lock().is_ok_and(|state| state.active)
    }
    pub(crate) fn belongs_to(&self, registry: &Arc<Mutex<Registry>>) -> bool {
        Arc::ptr_eq(&self.data.registry, registry)
    }
    pub(super) fn admit(&self, context: &EditorContext) -> Result<MutexGuard<'_, State>, String> {
        let state = self
            .data
            .state
            .lock()
            .map_err(|_| "Hook invocation interrupted")?;
        if !state.active || state.started || context.version != self.version() {
            return Err("Hook invocation was started, cancelled or changed version".into());
        }
        Ok(state)
    }
}
impl Runtime {
    pub fn invocation(&self, version: DocumentVersion) -> Invocation {
        Invocation {
            data: Arc::new(Data {
                registry: self.registry.clone(),
                version,
                state: Mutex::new(State {
                    active: true,
                    started: false,
                }),
            }),
        }
    }
}
