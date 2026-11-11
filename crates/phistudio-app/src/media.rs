//! Ownership of SAF snapshots ends when a draft is closed or successfully created.
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MediaKind {
    EditorSound(phistudio_core::EditorSoundEvent),
    SoundPack,
    CurveTemplates,
    Shader,
    Resource,
    LineTexture,
    Skin,
    Audio,
    HitSound,
    Illustration,
}
impl MediaKind {
    pub fn checking_message(self) -> &'static str {
        match self {
            Self::Shader => "Checking GLSL shader…",
            Self::Resource | Self::LineTexture => "Checking project resource…",
            Self::Skin => "Checking skin…",
            Self::Audio => "Checking audio…",
            Self::HitSound => "Checking hitsound…",
            Self::EditorSound(_) => "Checking editor sound…",
            Self::SoundPack => "Importing sound pack…",
            Self::CurveTemplates => "Importing curve templates…",
            Self::Illustration => "Checking illustration…",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Shader => "GLSL shader",
            Self::Resource => "Project resource",
            Self::LineTexture => "Image / texture",
            Self::Skin => "Skin",
            Self::Audio => "Audio",
            Self::HitSound => "Hitsound",
            Self::EditorSound(_) => "Editor sound",
            Self::SoundPack => "Sound pack",
            Self::CurveTemplates => "Curve templates",
            Self::Illustration => "Illustration",
        }
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub fn protocol(self) -> &'static str {
        match self {
            Self::SoundPack | Self::CurveTemplates => "resource",
            Self::Shader => "shader",
            Self::Resource => "resource",
            Self::LineTexture => "line-texture",
            Self::Skin => "skin",
            Self::Audio => "audio",
            Self::HitSound | Self::EditorSound(_) => "hitsound",
            Self::Illustration => "illustration",
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct MediaFile(Arc<MediaSource>);
#[derive(Debug)]
struct MediaSource {
    path: PathBuf,
    name: String,
    owned: bool,
    _provider: Option<phistudio_plugins::Artifact>,
    _export: Option<phistudio_core::ProjectExport>,
}
impl Drop for MediaSource {
    fn drop(&mut self) {
        if self.owned {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
impl MediaFile {
    pub fn new(path: PathBuf, name: String, owned: bool) -> Self {
        Self(Arc::new(MediaSource {
            path,
            name,
            owned,
            _provider: None,
            _export: None,
        }))
    }
    pub fn from_provider(artifact: phistudio_plugins::Artifact) -> Self {
        Self(Arc::new(MediaSource {
            path: artifact.path().to_owned(),
            name: artifact.name().to_owned(),
            owned: false,
            _provider: Some(artifact),
            _export: None,
        }))
    }
    pub fn from_export(package: phistudio_core::ProjectExport) -> Self {
        Self(Arc::new(MediaSource {
            path: package.path().to_owned(),
            name: package.name.clone(),
            owned: false,
            _provider: None,
            _export: Some(package),
        }))
    }
    pub fn path(&self) -> &Path {
        &self.0.path
    }
    pub fn name(&self) -> &str {
        &self.0.name
    }
    pub fn validate(self, kind: MediaKind) -> Result<Self, String> {
        match kind {
            MediaKind::SoundPack => {
                return Err("Sound packs must be imported through the sound library".into());
            }
            MediaKind::CurveTemplates => {
                return Err("Curve templates must be imported through the curve library".into());
            }
            MediaKind::EditorSound(_) => {
                phistudio_prpr::load_interface_sound(self.path())
                    .map_err(|e| format!("Could not load editor sound: {e:#}"))?;
            }
            MediaKind::Shader => {
                use std::io::Read;
                let mut source = String::new();
                std::fs::File::open(self.path())
                    .map_err(|e| e.to_string())?
                    .take(phistudio_core::shaders::MAX_SOURCE_BYTES as u64 + 1)
                    .read_to_string(&mut source)
                    .map_err(|e| e.to_string())?;
                phistudio_core::shaders::validate_shader_source("import.glsl", &source)
                    .map_err(|e| e.to_string())?;
            }
            MediaKind::LineTexture => {
                if !matches!(
                    phistudio_core::resources::ResourceKind::from_name(self.name()),
                    Some(
                        phistudio_core::resources::ResourceKind::Image
                            | phistudio_core::resources::ResourceKind::Gif
                    )
                ) {
                    return Err("Choose an image or GIF texture".into());
                }
                phistudio_prpr::load_texture_thumbnail(self.path())
                    .map_err(|e| format!("Could not load resource: {e:#}"))?;
            }
            MediaKind::Resource => {
                phistudio_prpr::inspect_media(self.path(), self.name())
                    .map_err(|e| format!("Could not load resource: {e:#}"))?;
            }
            MediaKind::Skin => {
                if std::fs::metadata(self.path())
                    .map_err(|e| e.to_string())?
                    .len()
                    > 256 * 1024 * 1024
                {
                    return Err("Skin archive exceeds 256 MiB".into());
                }
            }
            MediaKind::HitSound => {
                if std::fs::metadata(self.path())
                    .map_err(|e| e.to_string())?
                    .len()
                    > phistudio_core::MAX_HITSOUND_BYTES
                {
                    return Err("Hitsound exceeds 8 MiB".into());
                }
                let length = phistudio_prpr::audio_file_length(self.path())
                    .map_err(|e| format!("Could not load hitsound: {e:#}"))?;
                if !length.is_finite() || length > 10. {
                    return Err("Use a hitsound no longer than 10 seconds".into());
                }
            }
            MediaKind::Audio => {
                phistudio_prpr::audio_file_length(self.path())
                    .map_err(|e| format!("Could not load audio: {e:#}"))?;
            }
            MediaKind::Illustration => {
                phistudio_prpr::load_project_cover(self.path())
                    .map_err(|e| format!("Could not load illustration: {e:#}"))?;
            }
        }
        Ok(self)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn wav_bytes() -> Vec<u8> {
        let data_len = 960_u32;
        let mut bytes = Vec::new();
        bytes.extend(b"RIFF");
        bytes.extend((36 + data_len).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16_u32.to_le_bytes());
        bytes.extend(1_u16.to_le_bytes());
        bytes.extend(1_u16.to_le_bytes());
        bytes.extend(48000_u32.to_le_bytes());
        bytes.extend(96000_u32.to_le_bytes());
        bytes.extend(2_u16.to_le_bytes());
        bytes.extend(16_u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(data_len.to_le_bytes());
        bytes.resize(44 + data_len as usize, 0);
        bytes
    }
    #[test]
    fn real_audio_and_image_are_validated_and_owned_snapshots_live_until_the_last_reference() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("snapshot");
        std::fs::write(&path, wav_bytes()).unwrap();
        let media = MediaFile::new(path.clone(), "Music.wav".into(), true)
            .validate(MediaKind::Audio)
            .unwrap();
        let retained = media.clone();
        drop(media);
        assert!(path.exists());
        drop(retained);
        assert!(!path.exists());
        let image = root.path().join("image");
        std::fs::write(
            &image,
            include_bytes!("../../../assets/skins/phigros/click.png"),
        )
        .unwrap();
        assert!(
            MediaFile::new(image.clone(), "image.png".into(), false)
                .validate(MediaKind::Illustration)
                .is_ok()
        );
        assert!(image.exists());
    }
    #[test]
    fn invalid_files_cannot_be_selected_and_only_temporary_snapshots_are_removed() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("bad.wav");
        std::fs::write(&original, b"not audio").unwrap();
        assert!(
            MediaFile::new(original.clone(), "bad.wav".into(), false)
                .validate(MediaKind::Audio)
                .is_err()
        );
        assert!(original.exists());
        let snapshot = root.path().join("temporary");
        std::fs::write(&snapshot, b"not an image").unwrap();
        assert!(
            MediaFile::new(snapshot.clone(), "bad.png".into(), true)
                .validate(MediaKind::Illustration)
                .is_err()
        );
        assert!(!snapshot.exists());
    }
}
