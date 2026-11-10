//! Portable JSON packs embed audio bytes; imported references never become paths.
use super::MAX_PACK_FILES;
use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use phistudio_core::{EditorSoundPack, ProjectExport};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::Path,
};
const MAX_ARCHIVE: usize = 24 * 1024 * 1024;
const MAX_AUDIO: usize = 16 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
struct Archive {
    format: String,
    version: u32,
    pack: EditorSoundPack,
    sounds: BTreeMap<String, String>,
}
fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    ensure!(
        std::fs::metadata(path)?.is_file(),
        "Choose a regular sound pack or audio file"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= limit,
        "Sound pack file exceeds its size limit"
    );
    Ok(bytes)
}
pub(crate) fn export(pack: EditorSoundPack) -> Result<ProjectExport> {
    let mut pack = pack.normalized();
    let mut sounds = BTreeMap::new();
    let mut ids: BTreeMap<String, String> = BTreeMap::new();
    let mut total = 0;
    for rule in pack.events.values_mut() {
        for path in &mut rule.files {
            let id = if let Some(id) = ids.get(path) {
                id.clone()
            } else {
                ensure!(
                    sounds.len() < MAX_PACK_FILES,
                    "A sound pack supports up to {MAX_PACK_FILES} unique files"
                );
                let bytes = read_bounded(Path::new(path), 2 * 1024 * 1024)?;
                phistudio_prpr::load_interface_sound(Path::new(path))
                    .context("Invalid custom sound")?;
                total += bytes.len();
                ensure!(total <= MAX_AUDIO, "Sound pack audio exceeds 16 MiB");
                let id = format!("sound-{}", sounds.len());
                sounds.insert(id.clone(), STANDARD.encode(bytes));
                ids.insert(path.clone(), id.clone());
                id
            };
            *path = id;
        }
    }
    pack.id.clear();
    let name = pack
        .name
        .chars()
        .take(48)
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let bytes = serde_json::to_vec(&Archive {
        format: "phistudio.soundpack".into(),
        version: 1,
        pack,
        sounds,
    })?;
    ensure!(
        bytes.len() <= MAX_ARCHIVE,
        "Sound pack archive exceeds 24 MiB"
    );
    Ok(ProjectExport::from_bytes(
        format!("{name}.phisounds.json"),
        &bytes,
    )?)
}
pub(crate) fn import(source: &Path, root: &Path) -> Result<EditorSoundPack> {
    let bytes = read_bounded(source, MAX_ARCHIVE)?;
    let archive: Archive = serde_json::from_slice(&bytes).context("Invalid sound pack JSON")?;
    ensure!(
        archive.format == "phistudio.soundpack" && archive.version == 1,
        "Unsupported sound pack format or version"
    );
    ensure!(
        archive.sounds.len() <= MAX_PACK_FILES,
        "A sound pack supports up to {MAX_PACK_FILES} unique files"
    );
    let mut pack = archive.pack;
    for rule in pack.events.values() {
        ensure!(
            rule.file.is_none() && rule.files.len() <= 4,
            "Sound pack actions support up to four variants"
        );
        for reference in &rule.files {
            ensure!(
                archive.sounds.contains_key(reference),
                "Missing sound pack audio: {reference}"
            );
        }
    }
    std::fs::create_dir_all(root)?;
    let staged = tempfile::Builder::new().prefix("pack-").tempdir_in(root)?;
    let mut paths = BTreeMap::new();
    let mut total = 0;
    let mut decoded = 0;
    for (index, (id, data)) in archive.sounds.into_iter().enumerate() {
        ensure!(
            data.len() <= (2_usize * 1024 * 1024).div_ceil(3) * 4,
            "Sound exceeds 2 MiB"
        );
        let bytes = STANDARD
            .decode(&data)
            .context("Invalid sound pack audio encoding")?;
        total += bytes.len();
        ensure!(
            !bytes.is_empty() && bytes.len() <= 2 * 1024 * 1024 && total <= MAX_AUDIO,
            "Sound pack audio exceeds its size limit"
        );
        let path = staged.path().join(format!("{index}.audio"));
        let mut file = std::fs::File::create(&path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        let clip =
            phistudio_prpr::load_interface_sound(&path).context("Invalid sound pack audio")?;
        decoded += std::mem::size_of_val(clip.frames());
        ensure!(
            decoded <= 64 * 1024 * 1024,
            "Decoded sound pack exceeds 64 MiB"
        );
        paths.insert(id, path.to_string_lossy().into_owned());
    }
    for rule in pack.events.values_mut() {
        for reference in &mut rule.files {
            *reference = paths
                .get(reference)
                .context("Missing audio reference")?
                .clone();
        }
    }
    // Ownership transfers only after every file and reference has been checked.
    let _ = staged.keep();
    Ok(pack.normalized())
}
#[cfg(test)]
mod tests;
