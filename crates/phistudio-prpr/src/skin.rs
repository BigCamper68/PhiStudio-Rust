//! The user-supplied pack is embedded unchanged and decoded by prpr itself.
//! Only the absent, unused ending cue is supplied as a short silent WAV.
use anyhow::{Result, bail};
use async_trait::async_trait;
use prpr::{
    core::{NoteStyle, ResourcePack},
    fs::FileSystem,
};
use std::any::Any;
use std::path::Path;

const ROOT: &str = "Phigros Official";
pub const ARCHIVE_SHA256: &str = "946d471b57633783798466b1987709f8af72a0e23e6ef05728d75fb6d686a914";
pub const FILES: &[(&str, &[u8])] = &[
    (
        "info.yml",
        include_bytes!("../../../assets/skins/phigros/info.yml"),
    ),
    (
        "click.png",
        include_bytes!("../../../assets/skins/phigros/click.png"),
    ),
    (
        "click_mh.png",
        include_bytes!("../../../assets/skins/phigros/click_mh.png"),
    ),
    (
        "hold.png",
        include_bytes!("../../../assets/skins/phigros/hold.png"),
    ),
    (
        "hold_mh.png",
        include_bytes!("../../../assets/skins/phigros/hold_mh.png"),
    ),
    (
        "flick.png",
        include_bytes!("../../../assets/skins/phigros/flick.png"),
    ),
    (
        "flick_mh.png",
        include_bytes!("../../../assets/skins/phigros/flick_mh.png"),
    ),
    (
        "drag.png",
        include_bytes!("../../../assets/skins/phigros/drag.png"),
    ),
    (
        "drag_mh.png",
        include_bytes!("../../../assets/skins/phigros/drag_mh.png"),
    ),
    (
        "hit_fx.png",
        include_bytes!("../../../assets/skins/phigros/hit_fx.png"),
    ),
    (
        "click.ogg",
        include_bytes!("../../../assets/skins/phigros/click.ogg"),
    ),
    (
        "hold.ogg",
        include_bytes!("../../../assets/skins/phigros/hold.ogg"),
    ),
    (
        "flick.ogg",
        include_bytes!("../../../assets/skins/phigros/flick.ogg"),
    ),
    (
        "drag.ogg",
        include_bytes!("../../../assets/skins/phigros/drag.ogg"),
    ),
];

fn silence() -> Vec<u8> {
    // 1 ms, mono, PCM16, 48 kHz. This is an integration fallback, not a pack edit.
    let mut wav = Vec::with_capacity(140);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&132_u32.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&48000_u32.to_le_bytes());
    wav.extend_from_slice(&96000_u32.to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&96_u32.to_le_bytes());
    wav.resize(140, 0);
    wav
}

#[derive(Clone)]
struct EmbeddedPack;
#[async_trait]
impl FileSystem for EmbeddedPack {
    async fn load_file(&mut self, path: &str) -> Result<Vec<u8>> {
        if path == "ending.wav" {
            return Ok(silence());
        }
        if let Some((_, bytes)) = FILES.iter().find(|(name, _)| *name == path) {
            return Ok(bytes.to_vec());
        }
        bail!("{ROOT}: asset not found: {path}")
    }
    async fn exists(&mut self, path: &str) -> Result<bool> {
        Ok(path == "ending.wav" || FILES.iter().any(|(name, _)| *name == path))
    }
    fn list_root(&self) -> Result<Vec<String>> {
        Ok(FILES
            .iter()
            .map(|(name, _)| name.to_string())
            .chain(std::iter::once("ending.wav".into()))
            .collect())
    }
    fn clone_box(&self) -> Box<dyn FileSystem> {
        Box::new(self.clone())
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

pub(crate) async fn load() -> Result<ResourcePack> {
    ResourcePack::load(&mut EmbeddedPack).await
}

#[derive(Clone)]
struct DiskPack {
    assets: crate::assets::ChartAssets,
}
#[async_trait]
impl FileSystem for DiskPack {
    async fn load_file(&mut self, path: &str) -> Result<Vec<u8>> {
        if self.assets.exists(path).await? {
            return self.assets.load_file(path).await;
        }
        // Phira allows omitted audio to use the built-in cues. Try all of a
        // pack's native formats before falling back, so WAV/MP3 stays audible.
        if let Some(name) = path.strip_suffix(".ogg")
            && matches!(name, "click" | "drag" | "flick" | "ending")
        {
            for ext in ["wav", "mp3"] {
                if self.assets.exists(&format!("{name}.{ext}")).await? {
                    anyhow::bail!("Try the pack's {ext} cue");
                }
            }
            if name == "ending" {
                return Ok(silence());
            }
            return EmbeddedPack.load_file(path).await;
        }
        anyhow::bail!("Skin asset not found: {path}")
    }
    async fn exists(&mut self, path: &str) -> Result<bool> {
        self.assets.exists(path).await
    }
    fn list_root(&self) -> Result<Vec<String>> {
        self.assets.list_root()
    }
    fn clone_box(&self) -> Box<dyn FileSystem> {
        Box::new(self.clone())
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}
pub(crate) async fn load_from(path: Option<&Path>) -> Result<ResourcePack> {
    if let Some(path) = path {
        ResourcePack::load(&mut DiskPack {
            assets: crate::assets::ChartAssets::new(Some(path))?,
        })
        .await
    } else {
        load().await
    }
}
/// Match ResourcePack's cue order and missing-cue fallback without GPU resources.
pub(crate) async fn sound_bytes(path: Option<&Path>, cue: &str) -> Result<Vec<u8>> {
    let mut fs: Box<dyn FileSystem> = if let Some(path) = path {
        Box::new(DiskPack {
            assets: crate::assets::ChartAssets::new(Some(path))?,
        })
    } else {
        Box::new(EmbeddedPack)
    };
    for extension in ["ogg", "wav", "mp3"] {
        if let Ok(bytes) = fs.load_file(&format!("{cue}.{extension}")).await {
            return Ok(bytes);
        }
    }
    bail!("Skin hitsound not found: {cue}")
}
/// CPU validation runs on the import worker, before publishing a library entry.
/// The original ResourcePack loader still creates every native GPU/audio resource.
pub fn inspect_skin(root: &Path) -> Result<(String, String, String)> {
    use anyhow::{Context, ensure};
    let info_path = root.join("info.yml");
    ensure!(
        info_path.metadata()?.len() <= 64 * 1024,
        "Skin info.yml exceeds 64 KiB"
    );
    let info: prpr::core::ResPackInfo = serde_yaml::from_slice(&std::fs::read(info_path)?)?;
    info.verify()?;
    ensure!(
        info.hit_fx_duration.is_finite()
            && info.hit_fx_duration > 0.
            && info.hit_fx_scale.is_finite()
            && info.hit_fx_scale > 0.,
        "Invalid hit-effect duration or scale"
    );
    let mut pixels = 0u64;
    for name in [
        "click.png",
        "click_mh.png",
        "hold.png",
        "hold_mh.png",
        "flick.png",
        "flick_mh.png",
        "drag.png",
        "drag_mh.png",
        "hit_fx.png",
    ] {
        let path = root.join(name);
        let reader = image::ImageReader::open(&path)
            .with_context(|| format!("Missing {name}"))?
            .with_guessed_format()?;
        let (w, h) = reader.into_dimensions()?;
        pixels += u64::from(w) * u64::from(h);
        ensure!(
            w > 0 && h > 0 && w <= 8192 && h <= 8192 && pixels <= 64 * 1024 * 1024,
            "Skin textures exceed the size limit"
        );
        let _ = image::open(path)?;
        let atlas = match name {
            "hold.png" => Some(info.hold_atlas),
            "hold_mh.png" => Some(info.hold_atlas_mh),
            _ => None,
        };
        if let Some((a, b)) = atlas {
            ensure!(u32::from(a) + u32::from(b) < h, "Invalid Hold atlas");
        }
        if name == "hit_fx.png" {
            ensure!(
                w >= info.hit_fx.0 && h >= info.hit_fx.1,
                "Hit-effect atlas has more frames than pixels"
            );
        }
    }
    for name in ["click", "drag", "flick", "ending"] {
        let path = ["ogg", "wav", "mp3"]
            .into_iter()
            .map(|e| root.join(format!("{name}.{e}")))
            .find(|p| p.is_file());
        if let Some(path) = path {
            let _ =
                crate::audio_file_length(&path).with_context(|| format!("Invalid {name} audio"))?;
        }
    }
    Ok((info.name, info.author, info.description))
}

impl crate::Preview {
    pub async fn set_skin(&mut self, path: Option<&Path>) -> Result<()> {
        let pack = load_from(path).await?;
        let emitter = prpr::core::ParticleEmitter::new(
            &pack,
            self.resource.config.note_scale,
            pack.info.hide_particles,
        )?;
        let click = self
            .resource
            .audio
            .create_sfx(pack.sfx_click.clone(), Some(prpr::core::BUFFER_SIZE))?;
        let drag = self
            .resource
            .audio
            .create_sfx(pack.sfx_drag.clone(), Some(prpr::core::BUFFER_SIZE))?;
        let flick = self
            .resource
            .audio
            .create_sfx(pack.sfx_flick.clone(), Some(prpr::core::BUFFER_SIZE))?;
        self.resource.judge_line_color = pack.info.color_perfect();
        self.resource.res_pack = pack;
        self.resource.emitter = emitter;
        self.resource.sfx_click = click;
        self.resource.sfx_drag = drag;
        self.resource.sfx_flick = flick;
        self.resource.config.res_pack_path = path.map(|p| p.to_string_lossy().into_owned());
        self.clear_particles = true;
        self.needs_seek = true;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SkinTexture {
    pub id: u64,
    pub size: [f32; 2],
    pub uv: [f32; 4],
}
impl SkinTexture {
    fn new(texture: &prpr::ext::SafeTexture) -> Self {
        Self {
            id: texture.raw_miniquad_texture_handle().gl_internal_id() as u64,
            size: [texture.width(), texture.height()],
            uv: [0., 0., 1., 1.],
        }
    }
}
#[derive(Clone, Debug)]
pub struct SkinStyle {
    pub width_scale: f32,
    pub tap: SkinTexture,
    pub hold: SkinTexture,
    pub flick: SkinTexture,
    pub drag: SkinTexture,
    pub head: [f32; 4],
    pub body: [f32; 4],
    pub tail: [f32; 4],
    pub body_texture: Option<SkinTexture>,
    /// Optional grid atlas copy; native repeating texture remains the fallback.
    pub body_atlas: Option<SkinTexture>,
}
impl SkinStyle {
    fn new(style: &NoteStyle, width_scale: f32) -> Self {
        let uv = |r: macroquad::prelude::Rect| [r.x, r.y, r.x + r.w, r.y + r.h];
        Self {
            width_scale,
            tap: SkinTexture::new(&style.click),
            hold: SkinTexture::new(&style.hold),
            flick: SkinTexture::new(&style.flick),
            drag: SkinTexture::new(&style.drag),
            head: uv(style.hold_head_rect()),
            body: uv(style.hold_body_rect()),
            tail: uv(style.hold_tail_rect()),
            body_texture: style.hold_body.as_ref().map(SkinTexture::new),
            body_atlas: None,
        }
    }
}
#[derive(Clone, Debug)]
pub struct EditorSkin {
    pub name: String,
    pub author: String,
    pub normal: SkinStyle,
    pub highlighted: SkinStyle,
    pub grid: Option<crate::GridAtlas>,
}
impl EditorSkin {
    pub(crate) fn new(pack: &ResourcePack) -> Self {
        let normal = SkinStyle::new(&pack.note_style, 1.);
        let highlighted = SkinStyle::new(
            &pack.note_style_mh,
            pack.note_style_mh.click.width() / pack.note_style.click.width(),
        );
        let grid = crate::GridAtlas::new(pack, &normal, &highlighted);
        Self {
            name: pack.info.name.clone(),
            author: pack.info.author.clone(),
            normal,
            highlighted,
            grid,
        }
    }
}
