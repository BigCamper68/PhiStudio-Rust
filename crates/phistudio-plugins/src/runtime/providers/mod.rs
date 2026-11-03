//! Revocable provider operations own private files and completed artifacts.
mod files;
use super::*;
use crate::CallResult;
pub(super) use files::Files;
use std::sync::MutexGuard;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Provider {
    Format {
        id: String,
        operation: FormatOperation,
    },
    Resource {
        id: String,
        resource: ResourceKind,
    },
}
impl Provider {
    pub fn id(&self) -> &str {
        match self {
            Self::Format { id, .. } | Self::Resource { id, .. } => id,
        }
    }
    fn capability(&self) -> Capability {
        match self {
            Self::Format { .. } => Capability::Formats,
            Self::Resource { .. } => Capability::Resources,
        }
    }
    fn matches(&self, contribution: &Contribution) -> bool {
        match (self, contribution) {
            (
                Self::Format { id, operation },
                Contribution::Format {
                    id: other,
                    decode,
                    encode,
                    ..
                },
            ) => {
                id == other
                    && match operation {
                        FormatOperation::Decode => *decode,
                        FormatOperation::Encode => *encode,
                    }
            }
            (
                Self::Resource { id, resource },
                Contribution::Resource {
                    id: other,
                    resource: kind,
                    ..
                },
            ) => id == other && resource == kind,
            _ => false,
        }
    }
}

#[derive(Clone)]
pub struct Operation {
    data: Arc<Data>,
}
struct Data {
    id: u64,
    origin: Origin,
    provider: Provider,
    bridge: Option<FormatBridge>,
    plugin_data: PluginDataExport,
    version: DocumentVersion,
    registry: Arc<Mutex<Registry>>,
    state: Mutex<State>,
}
#[derive(Default)]
pub(super) struct State {
    pub(super) started: bool,
    finished: bool,
    cancelled: bool,
    required: Vec<Origin>,
}
impl std::fmt::Debug for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Operation")
            .field("id", &self.id())
            .field("origin", self.origin())
            .finish()
    }
}
impl PartialEq for Operation {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
    }
}
impl Eq for Operation {}
impl Operation {
    pub(crate) fn belongs_to(&self, registry: &Arc<Mutex<Registry>>) -> bool {
        Arc::ptr_eq(&self.data.registry, registry)
    }
    pub fn id(&self) -> u64 {
        self.data.id
    }
    pub fn origin(&self) -> &Origin {
        &self.data.origin
    }
    pub fn provider(&self) -> &Provider {
        &self.data.provider
    }
    pub fn format_bridge(&self) -> Option<FormatBridge> {
        self.data.bridge
    }
    pub fn plugin_data_export(&self) -> PluginDataExport {
        self.data.plugin_data
    }
    pub fn version(&self) -> DocumentVersion {
        self.data.version
    }
    pub fn cancel(&self) {
        if let Ok(mut state) = self.data.state.lock() {
            state.cancelled = true;
        }
    }
    fn valid(&self, registry: &Registry, state: &State) -> bool {
        !state.cancelled && registry.valid_scope(self.origin(), &state.required)
    }
    pub(crate) fn current_in(&self, registry: &Registry, running: bool) -> bool {
        self.data
            .state
            .lock()
            .is_ok_and(|s| self.valid(registry, &s) && (!running || (s.started && !s.finished)))
    }
    pub fn is_current(&self) -> bool {
        self.data
            .registry
            .lock()
            .is_ok_and(|r| self.data.state.lock().is_ok_and(|s| self.valid(&r, &s)))
    }
    pub fn is_running(&self) -> bool {
        self.data.registry.lock().is_ok_and(|r| {
            self.data
                .state
                .lock()
                .is_ok_and(|s| self.valid(&r, &s) && s.started && !s.finished)
        })
    }
    pub(super) fn admit(
        &self,
        context: &EditorContext,
        registry: &Registry,
    ) -> Result<MutexGuard<'_, State>, String> {
        let state = self
            .data
            .state
            .lock()
            .map_err(|_| "Provider operation interrupted")?;
        if context.version != self.version()
            || !self.valid(registry, &state)
            || state.started
            || state.finished
        {
            return Err("Provider operation was started, cancelled or changed version".into());
        }
        Ok(state)
    }
    fn require(&self, required: &[Origin]) -> Result<(), String> {
        let registry = self
            .data
            .registry
            .lock()
            .map_err(|_| "Plugin registry interrupted")?;
        let mut state = self
            .data
            .state
            .lock()
            .map_err(|_| "Provider operation interrupted")?;
        if !self.valid(&registry, &state)
            || state.finished
            || !required.iter().all(|p| registry.current(p).is_some())
        {
            return Err("Provider operation or a participant is no longer active".into());
        }
        for origin in required {
            if origin != self.origin() && !state.required.contains(origin) {
                state.required.push(origin.clone());
            }
        }
        if state.required.len() > crate::MAX_CONTRIBUTIONS {
            return Err("Provider operation exceeds participant limit".into());
        }
        Ok(())
    }
    fn finish(&self, required: &[Origin]) -> Result<(), String> {
        self.require(required)?;
        let mut state = self
            .data
            .state
            .lock()
            .map_err(|_| "Provider operation interrupted")?;
        if state.finished || state.cancelled {
            return Err("Provider operation has already finished".into());
        }
        state.finished = true;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Artifact {
    _directory: tempfile::TempDir,
    path: PathBuf,
    operation: Operation,
    name: String,
}
impl Artifact {
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn operation(&self) -> &Operation {
        &self.operation
    }
    /// Stage beside the destination; only a current activation may publish it.
    pub fn write_to(&self, destination: &std::path::Path) -> Result<(), String> {
        use std::io::{Read, Write};
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));
        let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        let mut input = std::fs::File::open(self.path()).map_err(|e| e.to_string())?;
        let mut buffer = vec![0; MAX_FILE_CHUNK_BYTES];
        let mut size = 0u64;
        loop {
            if !self.operation.is_current() {
                return Err("Provider result was cancelled or revoked".into());
            }
            let read = input.read(&mut buffer).map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            size += read as u64;
            if size > MAX_PROVIDER_FILE_BYTES {
                return Err("Provider artifact exceeds its size limit".into());
            }
            staged
                .write_all(&buffer[..read])
                .map_err(|e| e.to_string())?;
        }
        staged.as_file().sync_all().map_err(|e| e.to_string())?;
        let registry = self
            .operation
            .data
            .registry
            .lock()
            .map_err(|_| "Plugin registry interrupted")?;
        let state = self
            .operation
            .data
            .state
            .lock()
            .map_err(|_| "Provider operation interrupted")?;
        if !state.finished || !self.operation.valid(&registry, &state) {
            return Err("Provider result was cancelled or revoked".into());
        }
        staged
            .persist(destination)
            .map_err(|e| e.error.to_string())?;
        Ok(())
    }
}
impl PackageSource for Artifact {
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Runtime {
    pub fn begin_provider(
        &mut self,
        origin: &Origin,
        provider: Provider,
        version: DocumentVersion,
    ) -> Result<Operation, String> {
        self.begin_provider_with_plugin_data(origin, provider, version, PluginDataExport::Preserve)
    }
    pub fn begin_provider_with_plugin_data(
        &mut self,
        origin: &Origin,
        provider: Provider,
        version: DocumentVersion,
        plugin_data: PluginDataExport,
    ) -> Result<Operation, String> {
        if plugin_data != PluginDataExport::Preserve
            && !matches!(
                provider,
                Provider::Format {
                    operation: FormatOperation::Encode,
                    ..
                }
            )
        {
            return Err("Plugin data export policies apply only to format encoding".into());
        }
        let registry = self
            .registry
            .lock()
            .map_err(|_| "Plugin registry interrupted")?;
        let info = registry
            .current(origin)
            .ok_or("Provider is no longer active")?;
        let registered = info.contributions.iter().find(|c| provider.matches(c));
        if !info.granted.contains(&provider.capability()) || registered.is_none() {
            return Err("Provider is not registered or granted".into());
        }
        let bridge = registered.and_then(|c| {
            if let Contribution::Format { bridge, .. } = c {
                Some(*bridge)
            } else {
                None
            }
        });
        if matches!(
            provider,
            Provider::Format {
                operation: FormatOperation::Encode,
                ..
            }
        ) && !info.granted.contains(&Capability::DocumentRead)
        {
            return Err("Format export requires DocumentRead".into());
        }
        if bridge == Some(FormatBridge::Project)
            && matches!(
                provider,
                Provider::Format {
                    operation: FormatOperation::Encode,
                    ..
                }
            )
            && !info.granted.contains(&Capability::ProjectRead)
        {
            return Err("Project format export requires ProjectRead".into());
        }
        let id = self.next_operation;
        self.next_operation = id
            .checked_add(1)
            .ok_or("Provider operation sequence exhausted")?;
        Ok(Operation {
            data: Arc::new(Data {
                id,
                origin: origin.clone(),
                provider,
                bridge,
                plugin_data,
                version,
                registry: self.registry.clone(),
                state: Mutex::new(State::default()),
            }),
        })
    }
    /// Revoke immediately and return the cleanup request ID to drain via poll.
    pub fn cancel_provider(&mut self, operation: &Operation) -> Result<u64, String> {
        if !operation.belongs_to(&self.registry) {
            return Err("Provider operation belongs to another runtime".into());
        }
        operation.cancel();
        self.submit(Action::CancelProvider(operation.clone()))
    }
}

impl Worker {
    pub(super) fn prune_providers(&mut self) {
        self.provider_jobs
            .retain(|_, files| files.operation.is_running());
    }
    pub(super) fn start_provider(
        &mut self,
        operation: Operation,
        source: Box<dyn PackageSource>,
        name: String,
        mut context: EditorContext,
    ) -> Result<CallResult, String> {
        self.provider_jobs
            .retain(|_, files| files.operation.is_running());
        if self.provider_jobs.len() >= 8 {
            return Err("Too many provider operations are active".into());
        }
        let (files, descriptor) =
            Files::new(operation.clone(), source, name, self.root.as_deref())?;
        self.provider_jobs.insert(operation.id(), files);
        context.chart = None;
        context.selection = Value::Null;
        self.host
            .provider(
                operation.origin().plugin.as_str(),
                operation.provider(),
                descriptor,
                context,
            )
            .map_err(|e| e.to_string())
    }
    pub(super) fn provider_io(
        &mut self,
        origin: &Origin,
        required: &[Origin],
        request: &ServiceCall,
        operation: Option<&Operation>,
    ) -> ServiceOutcome {
        let result = (|| {
            let operation = operation.ok_or("File access requires a provider operation")?;
            if origin != operation.origin() && !required.contains(operation.origin()) {
                return Err("Provider file owner does not match the caller".into());
            }
            operation.require(required)?;
            if !request.service.version.matches(&Version::new(1, 0, 0)) {
                return Err("Provider file service version is unavailable".into());
            }
            let files = self
                .provider_jobs
                .get_mut(&operation.id())
                .ok_or("Provider files are no longer available")?;
            files.call(request)
        })();
        match result {
            Ok(value) => ServiceOutcome::Success { value },
            Err(error) => ServiceOutcome::error("file_error", error),
        }
    }
    pub(super) fn complete_provider(
        &mut self,
        operation: &Operation,
        owned: &OwnedEffect,
        file: u64,
        name: String,
    ) -> Result<Artifact, String> {
        if owned.origin != *operation.origin() || file != 2 || !valid_file_name(&name) {
            return Err("Provider completion has an invalid owner, file or name".into());
        }
        operation.require(&owned.required)?;
        let files = self
            .provider_jobs
            .get_mut(&operation.id())
            .ok_or("Provider files are unavailable")?;
        files.flush()?;
        operation.finish(&owned.required)?;
        let files = self
            .provider_jobs
            .remove(&operation.id())
            .ok_or("Provider files are unavailable")?;
        let path = files.directory.path().join("output");
        Ok(Artifact {
            _directory: files.directory,
            path,
            operation: operation.clone(),
            name,
        })
    }
    pub(super) fn fail_provider(
        &mut self,
        operation: &Operation,
        owned: &OwnedEffect,
    ) -> Result<(), String> {
        if owned.origin != *operation.origin() {
            return Err("Only the provider can finish its operation".into());
        }
        operation.finish(&owned.required)?;
        self.provider_jobs.remove(&operation.id());
        Ok(())
    }
}
