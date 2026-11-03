use crate::{NativePlugin, WasmLimits, WasmPlugin, validate_native_file};
use phistudio_sdk::{Backend, Manifest, Plugin, validate_package_path};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub const MANIFEST_NAME: &str = "manifest.json";
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Package {
    root: PathBuf,
    pub manifest: Manifest,
}

pub(crate) enum PreparedPlugin {
    Wasm(Box<WasmPlugin>),
    Native,
}
impl PreparedPlugin {
    pub(crate) fn instantiate(self, package: &Package) -> Result<Box<dyn Plugin>, String> {
        match self {
            Self::Wasm(instance) => Ok(instance),
            Self::Native => {
                let Backend::Native { entry } = &package.manifest.backend else {
                    return Err("Prepared plugin backend changed".into());
                };
                Ok(Box::new(NativePlugin::new(confined_path(
                    &package.root,
                    entry,
                )?)?))
            }
        }
    }
}

impl Package {
    pub fn open(root: &Path) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        if !root.is_dir() {
            return Err("Plugin package must be a directory".into());
        }
        let bytes = read_confined(&root, MANIFEST_NAME, MAX_MANIFEST_BYTES)?;
        let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        manifest.validate()?;
        if matches!(manifest.backend, Backend::Builtin) {
            return Err("External packages cannot declare the builtin backend".into());
        }
        Ok(Self { root, manifest })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn asset(&self, path: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
        read_confined(&self.root, path, max_bytes)
    }
    pub fn load_wasm(&self, limits: WasmLimits) -> Result<WasmPlugin, String> {
        let Backend::Wasm { entry } = &self.manifest.backend else {
            return Err("Package is not a Wasm plugin".into());
        };
        WasmPlugin::load(&self.asset(entry, limits.module_bytes)?, limits)
    }
    /// Wasm compiles now; native execution remains deferred until Activate.
    pub fn load(&self, limits: WasmLimits) -> Result<Box<dyn Plugin>, String> {
        self.prepare(limits)?.instantiate(self)
    }
    pub(crate) fn prepare(&self, limits: WasmLimits) -> Result<PreparedPlugin, String> {
        match &self.manifest.backend {
            Backend::Wasm { .. } => Ok(PreparedPlugin::Wasm(Box::new(self.load_wasm(limits)?))),
            Backend::Native { entry } => {
                validate_native_file(&confined_path(&self.root, entry)?)?;
                Ok(PreparedPlugin::Native)
            }
            Backend::Builtin => Err("External packages cannot use the builtin backend".into()),
        }
    }
}

fn confined_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    validate_package_path(name)?;
    let path = root.join(name).canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(root) || !path.is_file() {
        return Err("Plugin asset leaves its package or is not a file".into());
    }
    Ok(path)
}

fn read_confined(root: &Path, name: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
    let path = confined_path(root, name)?;
    let mut bytes = vec![];
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > max_bytes {
        return Err("Plugin asset exceeds its size limit".into());
    }
    Ok(bytes)
}

/// A bad package does not hide the remaining installed plugins.
pub struct DiscoveredPackage {
    pub path: PathBuf,
    pub package: Result<Package, String>,
}

pub fn discover(root: &Path) -> Result<Vec<DiscoveredPackage>, std::io::Error> {
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut paths = std::fs::read_dir(root)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    Ok(paths
        .into_iter()
        .filter(|p| {
            p.is_dir()
                && !p
                    .file_name()
                    .is_some_and(|s| s.to_string_lossy().starts_with('.'))
        })
        .map(|path| {
            let package = Package::open(&path);
            DiscoveredPackage { path, package }
        })
        .collect())
}
