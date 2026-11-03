use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

pub const MAX_FILE_CHUNK_BYTES: usize = 256 * 1024;
pub const MAX_PROVIDER_FILE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_PACKAGE_ENTRIES: usize = 1024;
pub const MAX_PACKAGE_DATA_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_PACKAGE_PATH_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatOperation {
    Decode,
    Encode,
}

/// The editor-side representation exchanged with a custom format converter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatBridge {
    #[default]
    Chart,
    Project,
}
impl FormatBridge {
    pub fn is_chart(&self) -> bool {
        *self == Self::Chart
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageEntries {
    pub operation: u64,
    #[serde(default)]
    pub output: bool,
    pub offset: usize,
    pub limit: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageEntry {
    pub path: String,
    pub size: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackagePage {
    pub offset: usize,
    pub total: usize,
    pub next_offset: Option<usize>,
    pub entries: Vec<PackageEntry>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackagePath {
    pub operation: u64,
    pub path: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRename {
    pub operation: u64,
    pub file: u64,
    pub path: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRemove {
    pub operation: u64,
    pub file: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackagePack {
    pub operation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Image,
    Audio,
    Font,
    Shader,
    #[default]
    Data,
}

/// Addresses a private operation file, never an OS path or arbitrary project file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderFile {
    pub id: u64,
    pub name: String,
    pub size: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderFiles {
    pub operation: u64,
    pub input: ProviderFile,
    pub output: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRead {
    pub operation: u64,
    pub file: u64,
    pub offset: u64,
    pub limit: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileWrite {
    pub operation: u64,
    pub file: u64,
    /// None appends; Some overwrites bytes at an offset in the private output.
    #[serde(default)]
    pub offset: Option<u64>,
    pub data: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileResize {
    pub operation: u64,
    pub file: u64,
    pub size: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileChunk {
    pub offset: u64,
    pub size: u64,
    pub next_offset: Option<u64>,
    pub data: String,
}
pub fn encode_file_chunk(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > MAX_FILE_CHUNK_BYTES {
        return Err("File chunk exceeds 256 KiB".into());
    }
    Ok(STANDARD.encode(bytes))
}
pub fn decode_file_chunk(data: &str) -> Result<Vec<u8>, String> {
    if data.len() > MAX_FILE_CHUNK_BYTES.div_ceil(3) * 4 {
        return Err("Encoded file chunk exceeds its limit".into());
    }
    let bytes = STANDARD.decode(data).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FILE_CHUNK_BYTES {
        return Err("File chunk exceeds 256 KiB".into());
    }
    Ok(bytes)
}
pub fn valid_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name != "."
        && name != ".."
        && !name.ends_with(['.', ' '])
        && !name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
}
pub fn valid_source_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 1024 && !name.chars().any(char::is_control)
}
pub fn valid_file_extension(extension: &str) -> bool {
    !extension.is_empty()
        && extension.len() <= 32
        && extension.as_bytes()[0].is_ascii_alphanumeric()
        && !extension.contains("..")
        && extension
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"._-".contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Contribution, EditorContext, Request};
    use serde_json::json;
    #[test]
    fn chart_format_messages_remain_compatible_and_project_bridges_are_explicit() {
        let old = json!({"kind":"format","id":"legacy","title":"Legacy","extensions":["old"],"decode":true,"encode":true});
        let mut contribution: Contribution = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(serde_json::to_value(&contribution).unwrap(), old);
        if let Contribution::Format { bridge, .. } = &mut contribution {
            *bridge = FormatBridge::Project;
        }
        assert_eq!(
            serde_json::to_value(contribution).unwrap()["bridge"],
            "project"
        );
        let old = json!({"kind":"format","id":"legacy","operation":"decode","files":{"operation":1,"input":{"id":1,"name":"source.old","size":1},"output":2},"context":EditorContext::default()});
        let request: Request = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(serde_json::to_value(request).unwrap(), old);
    }
}
