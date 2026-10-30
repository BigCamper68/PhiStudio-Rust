//! Resource inspection decodes on the file worker; playback uses the native mixer.
use anyhow::{Result, ensure};
pub use sasa::AudioClip as ResourceAudioClip;
use std::{
    fs::File,
    io::{Cursor, Read},
    path::Path,
    sync::{Arc, Mutex, Weak},
};
pub struct MediaInspection {
    pub image: Option<crate::CoverImage>,
    pub audio: Option<ResourceAudioClip>,
    pub details: String,
}
pub fn inspect_media(path: &Path, name: &str) -> Result<MediaInspection> {
    let kind = phistudio_core::resources::ResourceKind::from_name(name)
        .ok_or_else(|| anyhow::anyhow!("Unsupported media format"))?;
    let mut bytes = vec![];
    File::open(path)?
        .take(128 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= 128 * 1024 * 1024,
        "Media must contain 1 byte–128 MiB"
    );
    let mut result = MediaInspection {
        image: None,
        audio: None,
        details: String::new(),
    };
    use phistudio_core::resources::ResourceKind;
    match kind {
        ResourceKind::Image | ResourceKind::Gif => {
            let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(16384);
            limits.max_image_height = Some(16384);
            limits.max_alloc = Some(128 * 1024 * 1024);
            reader.limits(limits);
            let image = reader.decode()?;
            result.details = format!(
                "{} × {}{}",
                image.width(),
                image.height(),
                if kind == ResourceKind::Gif {
                    " · GIF first frame"
                } else {
                    ""
                }
            );
            let image = image
                .resize(640, 360, image::imageops::FilterType::Triangle)
                .into_rgba8();
            result.image = Some(crate::CoverImage {
                size: [image.width() as usize, image.height() as usize],
                rgba: image.into_raw(),
            });
        }
        ResourceKind::Audio => {
            let clip = sasa::AudioClip::new(bytes)?;
            ensure!(
                clip.length().is_finite() && clip.length() > 0.,
                "Audio has no playable samples"
            );
            result.details = format!("{:.3} seconds", clip.length());
            result.audio = Some(clip);
        }
        ResourceKind::Font => {
            glyph_brush::ab_glyph::FontArc::try_from_vec(bytes)?;
            result.details = "Valid OpenType / TrueType font".into();
        }
    }
    Ok(result)
}
static ACTIVE: Mutex<Weak<Mutex<sasa::Music>>> = Mutex::new(Weak::new());
pub(crate) fn suspend() {
    if let Some(music) = ACTIVE.lock().ok().and_then(|m| m.upgrade())
        && let Ok(mut music) = music.lock()
    {
        let _ = music.pause();
    }
}
impl crate::Preview {
    pub fn audition_resource(&mut self, clip: Option<ResourceAudioClip>) -> Result<()> {
        if let Some(music) = self.resource_music.take() {
            music
                .lock()
                .map_err(|_| anyhow::anyhow!("Resource audio lock failed"))?
                .pause()?;
        }
        if let Some(clip) = clip {
            let mut music = self
                .resource
                .audio
                .create_music(clip, sasa::MusicParams::default())?;
            music.play()?;
            let music = Arc::new(Mutex::new(music));
            *ACTIVE
                .lock()
                .map_err(|_| anyhow::anyhow!("Resource audio lock failed"))? =
                Arc::downgrade(&music);
            self.resource_music = Some(music);
        }
        Ok(())
    }
}
