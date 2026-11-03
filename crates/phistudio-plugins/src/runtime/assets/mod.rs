//! Owner-scoped, immutable resource snapshots. Disk work stays on the worker.
mod images;
mod io;
use super::{HostCall, Registry, Runtime, Worker, dispatch};
use crate::{Origin, limits::serialized_size};
use phistudio_sdk::*;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Clone, Default, PartialEq, Eq)]
struct Project {
    document: DocumentVersion,
    assets: ProjectAssetsVersion,
    root: Option<PathBuf>,
}

struct Snapshot {
    descriptor: AssetFile,
    owner: Origin,
    required: Vec<Origin>,
    project: Option<Project>,
    file: tempfile::TempPath,
    image: Option<Arc<PreparedImage>>,
}
impl Snapshot {
    fn current(&self, registry: &Registry, project: &Project) -> bool {
        registry.valid_scope(&self.owner, &self.required)
            && self
                .project
                .as_ref()
                .is_none_or(|p| same_assets(p, project))
    }
}

/// Decoded on the plugin worker. The application only uploads these pixels.
pub struct PreparedImage {
    size: [usize; 2],
    rgba: Vec<u8>,
}
impl PreparedImage {
    pub fn size(&self) -> [usize; 2] {
        self.size
    }
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

#[derive(Default)]
pub(super) struct Assets {
    project: Project,
    next: u64,
    files: BTreeMap<u64, Snapshot>,
}
impl Assets {
    pub(super) fn clear(&mut self) {
        self.files.clear();
    }
    fn bytes(&self) -> u64 {
        self.files.values().map(|f| f.descriptor.size).sum()
    }
    fn image_bytes(&self) -> u64 {
        self.files
            .values()
            .filter_map(|f| f.image.as_ref())
            .map(|i| i.rgba.len() as u64)
            .sum()
    }
    fn owned(&self, file: u64, owner: &Origin) -> Result<&Snapshot, String> {
        self.files
            .get(&file)
            .filter(|s| &s.owner == owner)
            .ok_or_else(|| "Asset handle does not belong to this activation".into())
    }
}

fn same_assets(a: &Project, b: &Project) -> bool {
    a.assets == b.assets && a.root == b.root
}

impl Runtime {
    /// Publishes trusted UI metadata without performing filesystem work.
    pub fn set_project(&self, document: DocumentVersion, asset_revision: u64, root: Option<&Path>) {
        if let Ok(mut state) = self.assets.lock() {
            state.project.document = document;
            state.project.assets = ProjectAssetsVersion {
                session: document.session,
                revision: asset_revision,
            };
            if state.project.root.as_deref() != root {
                state.project.root = root.map(Path::to_path_buf);
            }
        }
    }

    /// Resolves only images prepared by this activation and still authorized.
    /// Handles are never shared across owners, activations or project changes.
    pub fn image(&self, owner: &Origin, file: u64) -> Option<Arc<PreparedImage>> {
        let registry = self.registry.lock().ok()?;
        let state = self.assets.lock().ok()?;
        let snapshot = state.owned(file, owner).ok()?;
        snapshot
            .current(&registry, &state.project)
            .then(|| snapshot.image.clone())?
    }
}

impl Worker {
    pub(super) fn prune_assets(&self, registry: &Mutex<Registry>, job: u64) {
        if let Ok(registry) = registry.lock()
            && let Ok(mut assets) = self.assets.lock()
        {
            let project = assets.project.clone();
            assets.files.retain(|_, s| {
                std::iter::once(&s.owner).chain(&s.required).all(|o| {
                    self.host.is_current(o)
                        && !registry.revoked.get(&o.plugin).is_some_and(|id| *id > job)
                }) && s.project.as_ref().is_none_or(|p| same_assets(p, &project))
            });
        }
    }

    pub(super) fn asset_service(
        &self,
        call: &HostCall,
        registry: &Mutex<Registry>,
        job: u64,
    ) -> ServiceOutcome {
        if !call.request.service.version.matches(&Version::new(1, 0, 0)) {
            return ServiceOutcome::error(
                "service_unavailable",
                "Asset service version unavailable",
            );
        }
        if !self.host.granted(&call.origin.plugin, Capability::Services) {
            return ServiceOutcome::error("permission_denied", "Services is required");
        }
        self.prune_assets(registry, job);
        match self.asset_call(call, registry, job) {
            Ok(value) => ServiceOutcome::Success { value },
            Err(error) => ServiceOutcome::error("asset_error", error),
        }
    }

    fn asset_call(
        &self,
        call: &HostCall,
        registry: &Mutex<Registry>,
        job: u64,
    ) -> Result<Value, String> {
        let active = || self.asset_call_active(call, registry, job);
        active()?;
        match call.request.method.as_str() {
            "open" => {
                let args: AssetOpen = arguments(call)?;
                validate_asset_path(&args.path)?;
                let (root, project) = self.asset_source(call, args.source)?;
                let allowance = {
                    let assets = self
                        .assets
                        .lock()
                        .map_err(|_| "Asset registry interrupted")?;
                    if assets.files.len() >= MAX_ASSET_HANDLES
                        || assets
                            .files
                            .values()
                            .filter(|s| s.owner == call.origin)
                            .count()
                            >= MAX_PLUGIN_ASSET_HANDLES
                    {
                        return Err("Close an asset before opening another handle".into());
                    }
                    MAX_ASSET_BYTES.saturating_sub(assets.bytes())
                };
                let current = || {
                    active()?;
                    self.check_project(call, project.as_ref())
                };
                let (file, size) =
                    io::snapshot(&root, &args.path, allowance, self.root.as_deref(), current)?;
                active()?;
                let _registry = registry.lock().map_err(|_| "Plugin registry interrupted")?;
                let mut assets = self
                    .assets
                    .lock()
                    .map_err(|_| "Asset registry interrupted")?;
                check_project(&assets.project, call, project.as_ref())?;
                assets.next = assets
                    .next
                    .checked_add(1)
                    .ok_or("Asset handle sequence exhausted")?;
                let descriptor = AssetFile {
                    id: assets.next,
                    source: args.source,
                    path: args.path,
                    size,
                    project: project.as_ref().map(|p| p.assets),
                };
                assets.files.insert(
                    descriptor.id,
                    Snapshot {
                        descriptor: descriptor.clone(),
                        owner: call.origin.clone(),
                        required: call.required.clone(),
                        project,
                        file,
                        image: None,
                    },
                );
                Ok(json!(descriptor))
            }
            "list" => {
                let args: AssetList = arguments(call)?;
                if args.limit == 0 || args.limit > 256 {
                    return Err("Asset list limit must be between 1 and 256".into());
                }
                let (root, project) = self.asset_source(call, args.source)?;
                let entries = io::list(&root, &args.directory, || {
                    active()?;
                    self.check_project(call, project.as_ref())
                })?;
                let mut page = AssetPage {
                    project: project.as_ref().map(|p| p.assets),
                    offset: args.offset,
                    total: entries.len(),
                    next_offset: None,
                    entries: vec![],
                };
                let mut remaining = MAX_FILE_CHUNK_BYTES - 512;
                for entry in entries.into_iter().skip(args.offset).take(args.limit) {
                    let Some(size) = serialized_size(&entry, remaining.saturating_sub(1)) else {
                        break;
                    };
                    remaining -= size + 1;
                    page.entries.push(entry);
                }
                let next = args.offset.saturating_add(page.entries.len());
                page.next_offset = (next < page.total).then_some(next);
                Ok(json!(page))
            }
            "read" => {
                let args: AssetRead = arguments(call)?;
                if args.limit == 0 || args.limit > MAX_FILE_CHUNK_BYTES {
                    return Err("Asset read limit must be between 1 byte and 256 KiB".into());
                }
                let (path, size, project) = self.asset_file(call, args.file)?;
                self.check_project(call, project.as_ref())?;
                let chunk = io::read(&path, size, args.offset, args.limit)?;
                active()?;
                self.check_project(call, project.as_ref())?;
                Ok(json!(chunk))
            }
            "image" => {
                let args: AssetHandle = arguments(call)?;
                self.prepare_image(call, registry, job, args.file)
            }
            "close" => {
                let args: AssetHandle = arguments(call)?;
                let mut assets = self
                    .assets
                    .lock()
                    .map_err(|_| "Asset registry interrupted")?;
                assets.owned(args.file, &call.origin)?;
                assets.files.remove(&args.file);
                Ok(Value::Null)
            }
            _ => Err("Unknown asset service method".into()),
        }
    }

    fn asset_source(
        &self,
        call: &HostCall,
        source: AssetSource,
    ) -> Result<(PathBuf, Option<Project>), String> {
        match source {
            AssetSource::Plugin => self
                .package_roots
                .get(&call.origin.plugin)
                .cloned()
                .map(|p| (p, None))
                .ok_or_else(|| "This plugin has no installed package assets".into()),
            AssetSource::Project => {
                if !self
                    .host
                    .granted(&call.origin.plugin, Capability::ProjectRead)
                {
                    return Err("ProjectRead is required to read project assets".into());
                }
                let assets = self
                    .assets
                    .lock()
                    .map_err(|_| "Asset registry interrupted")?;
                let project = &assets.project;
                if project.document != call.context.version {
                    return Err("Document changed before the project asset query ran".into());
                }
                let root = project
                    .root
                    .clone()
                    .ok_or("This project has no asset directory")?;
                Ok((root, Some(project.clone())))
            }
        }
    }

    fn asset_call_active(
        &self,
        call: &HostCall,
        registry: &Mutex<Registry>,
        job: u64,
    ) -> Result<(), String> {
        if !dispatch::current(&self.host, registry, &call.origin, &call.required, job)?
            || call.invocation().is_some_and(|i| !i.is_active())
            || call.interaction().is_some_and(|i| !i.is_active())
            || call.operation().is_some_and(|o| !o.is_running())
        {
            return Err("Asset request was cancelled or its activation changed".into());
        }
        Ok(())
    }
    fn check_project(&self, call: &HostCall, project: Option<&Project>) -> Result<(), String> {
        let assets = self
            .assets
            .lock()
            .map_err(|_| "Asset registry interrupted")?;
        check_project(&assets.project, call, project)
    }
    fn asset_file(
        &self,
        call: &HostCall,
        file: u64,
    ) -> Result<(PathBuf, u64, Option<Project>), String> {
        let assets = self
            .assets
            .lock()
            .map_err(|_| "Asset registry interrupted")?;
        let snapshot = assets.owned(file, &call.origin)?;
        Ok((
            snapshot.file.to_path_buf(),
            snapshot.descriptor.size,
            snapshot.project.clone(),
        ))
    }
}

fn check_project(
    current: &Project,
    call: &HostCall,
    project: Option<&Project>,
) -> Result<(), String> {
    if project.is_some_and(|p| !same_assets(p, current) || current.document != call.context.version)
    {
        return Err("Project or resources changed before the asset request finished".into());
    }
    Ok(())
}
fn arguments<T: DeserializeOwned>(call: &HostCall) -> Result<T, String> {
    serde_json::from_value(call.request.arguments.clone()).map_err(|e| e.to_string())
}
