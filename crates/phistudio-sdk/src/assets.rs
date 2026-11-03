//! Read-only snapshots for commands, services, panels and render callbacks.
use serde::{Deserialize, Serialize};

pub const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_ASSET_HANDLES: usize = 64;
pub const MAX_PLUGIN_ASSET_HANDLES: usize = 16;
pub const MAX_ASSET_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_ASSET_IMAGES_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetSource {
    Plugin,
    Project,
}

/// Resource revision, independent of chart edits. Changes invalidate snapshots.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectAssetsVersion {
    pub session: u64,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetOpen {
    pub source: AssetSource,
    pub path: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetFile {
    pub id: u64,
    pub source: AssetSource,
    pub path: String,
    pub size: u64,
    pub project: Option<ProjectAssetsVersion>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetHandle {
    pub file: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetRead {
    pub file: u64,
    pub offset: u64,
    pub limit: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetList {
    pub source: AssetSource,
    #[serde(default)]
    pub directory: String,
    pub offset: usize,
    pub limit: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetEntry {
    pub path: String,
    pub directory: bool,
    pub size: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetPage {
    pub project: Option<ProjectAssetsVersion>,
    pub offset: usize,
    pub total: usize,
    pub next_offset: Option<usize>,
    pub entries: Vec<AssetEntry>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetImage {
    /// Use this owner-scoped handle in DrawPrimitive::Image.
    pub file: u64,
    pub width: u32,
    pub height: u32,
}

pub fn validate_asset_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 4096
        || path.split('/').count() > 64
        || path.split('/').any(|part| !crate::valid_file_name(part))
    {
        return Err("Asset paths must be relative slash-separated file names".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn asset_names_are_portable_and_never_reinterpret_paths() {
        for path in [
            "cover.png",
            "textures/雪/line.bmp",
            ".hidden/data.bin",
            "My files/file 1.json",
        ] {
            validate_asset_path(path).unwrap();
        }
        for path in [
            "",
            ".",
            "..",
            "/cover.png",
            "../cover.png",
            "a//b",
            "a/./b",
            "a/../b",
            "a\\b",
            "C:/file",
            "control\n",
            "nul\0",
        ] {
            assert!(validate_asset_path(path).is_err(), "{path:?}");
        }
        assert!(validate_asset_path(&"a/".repeat(65)).is_err());
        assert!(validate_asset_path(&"a".repeat(256)).is_err());
    }

    #[test]
    fn asset_messages_keep_explicit_sources_and_resource_versions() {
        let request: AssetOpen =
            serde_json::from_value(json!({"source":"project","path":"cover.png"})).unwrap();
        assert_eq!(request.source, AssetSource::Project);
        assert!(serde_json::from_value::<AssetOpen>(json!({"path":"cover.png"})).is_err());
        assert!(
            serde_json::from_value::<AssetOpen>(
                json!({"source":"plugin","path":"cover.png","root":"/private"})
            )
            .is_err()
        );
        let file = AssetFile {
            id: 2,
            source: request.source,
            path: request.path,
            size: 150,
            project: Some(ProjectAssetsVersion {
                session: 3,
                revision: 7,
            }),
        };
        let restored: AssetFile =
            serde_json::from_value(serde_json::to_value(&file).unwrap()).unwrap();
        assert_eq!(restored, file);
    }
}
