//! Development helpers share the installer's confinement and size limits.
use crate::{Package, WasmLimits, install_package, validate_native_file};
use phistudio_sdk::{Backend, PermissionRequest};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Compile/validate a portable module without activating it. Native binaries
/// receive structural checks only: inspection never loads their constructors.
pub fn validate_external_package(package: &Package) -> Result<(), String> {
    match &package.manifest.backend {
        Backend::Wasm { .. } => {
            package.load_wasm(WasmLimits::default())?;
            Ok(())
        }
        Backend::Native { entry } => validate_native_file(&package.root().join(entry)),
        Backend::Builtin => Err("External packages cannot use builtin code".into()),
    }
}

pub fn inspect_external_package(source: &Path) -> Result<serde_json::Value, String> {
    let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (package, ()) = install_package(source, temporary.path(), validate_external_package)?;
    Ok(
        serde_json::json!({"manifest":package.manifest,"permissions":PermissionRequest::from_manifest(&package.manifest),"validated":true,"activated":false}),
    )
}

/// Package a validated snapshot, so concurrent edits cannot bypass validation.
/// The destination is created exclusively; an existing package is never replaced.
pub fn pack_external_package(source: &Path, destination: &Path) -> Result<(), String> {
    let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (package, ()) = install_package(source, temporary.path(), validate_external_package)?;
    fn entries(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                entries(root, &entry.path(), out)?;
            } else {
                out.push(
                    entry
                        .path()
                        .strip_prefix(root)
                        .map_err(|e| e.to_string())?
                        .into(),
                );
            }
        }
        Ok(())
    }
    let mut files = vec![];
    entries(package.root(), package.root(), &mut files)?;
    files.sort();
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut output = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    {
        let mut zip = zip::ZipWriter::new(&mut output);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        for relative in files {
            let name = relative
                .to_str()
                .ok_or("Package paths must be UTF-8")?
                .replace('\\', "/");
            zip.start_file(name, options).map_err(|e| e.to_string())?;
            let mut input =
                fs::File::open(package.root().join(relative)).map_err(|e| e.to_string())?;
            std::io::copy(&mut input, &mut zip).map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
    }
    output.as_file().sync_all().map_err(|e| e.to_string())?;
    output
        .persist_noclobber(destination)
        .map_err(|e| e.error.to_string())?;
    Ok(())
}

pub fn create_plugin_template(destination: &Path, id: &str, sdk: &Path) -> Result<(), String> {
    if !phistudio_sdk::valid_id(id) {
        return Err("Plugin ID must be a valid lower-case dotted identifier".into());
    }
    if destination.exists() {
        return Err("Template destination already exists".into());
    }
    let sdk = sdk.canonicalize().map_err(|e| e.to_string())?;
    let sdk = sdk.to_str().ok_or("SDK path must be UTF-8")?;
    let name = id.replace(['.', '-'], "_");
    let manifest = serde_json::json!({"api":1,"id":id,"name":id,"version":"0.1.0","backend":{"kind":"wasm","entry":"plugin.wasm"},"capabilities":["ui"]});
    fs::create_dir(destination).map_err(|e| e.to_string())?;
    let result = (|| -> Result<(), String> {
        fs::create_dir(destination.join("src")).map_err(|e| e.to_string())?;
        fs::write(
            destination.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let cargo = format!(
            "[package]\nname = {name:?}\nversion = \"0.1.0\"\nedition = \"2024\"\nlicense = \"GPL-3.0-only\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\nphistudio-sdk = {{ path = {sdk:?} }}\n\n[workspace]\n"
        );
        fs::write(destination.join("Cargo.toml"), cargo).map_err(|e| e.to_string())?;
        fs::write(destination.join("src/lib.rs"), TEMPLATE).map_err(|e| e.to_string())?;
        fs::write(destination.join(".gitignore"), "/target/\n/plugin.wasm\n")
            .map_err(|e| e.to_string())?;
        fs::write(destination.join("README.md"),format!("# {id}\n\nBuild with `cargo build --target wasm32-unknown-unknown --release`.\nCopy `target/wasm32-unknown-unknown/release/{name}.wasm` to `plugin.wasm`,\nthen use `phistudio-plugin validate .` and `phistudio-plugin pack . ../{id}.zip`.\nThe SDK path points to the local checkout used to create this template.\n")).map_err(|e|e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

const TEMPLATE: &str = r#"use phistudio_sdk::*;
#[derive(Default)]
struct Example;
impl Plugin for Example {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![Contribution::Command {
                    id: "hello".into(), title: "Hello from plugin".into(),
                    shortcut: None, snapshot: false,
                }], ..Default::default()
            },
            Request::Command { .. } => Response {
                effects: vec![Effect::Notify { message: "Hello from plugin".into() }],
                ..Default::default()
            },
            _ => Response::default(),
        })
    }
}
phistudio_sdk::export_plugin!(|| Example);
"#;
