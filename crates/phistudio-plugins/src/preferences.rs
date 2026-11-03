use phistudio_sdk::MAX_MESSAGE_BYTES;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::Path,
};

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct PluginPreference {
    pub enabled: bool,
    #[serde(default)]
    pub settings: BTreeMap<String, Value>,
}
#[derive(Serialize, Deserialize)]
pub(crate) struct Preferences {
    pub version: u32,
    pub plugins: BTreeMap<String, PluginPreference>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            plugins: BTreeMap::new(),
        }
    }
}
pub(crate) fn read<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
        Err(e) => return Err(e.to_string()),
    };
    let mut bytes = vec![];
    file.take(MAX_MESSAGE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err("Plugin state exceeds the size limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
pub(crate) fn write(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("Plugin state needs a parent directory")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err("Plugin state exceeds the size limit".into());
    }
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temp.write_all(&bytes).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
