//! Editor resources use the user's pack through the original prpr loader.
//! Gameplay audio uses the same sasa backend and pack clips as Phira.
use anyhow::Result;
use macroquad::prelude::*;
use prpr::{
    config::Config,
    core::{Matrix, ParticleEmitter, Resource},
    ext::SafeTexture,
    info::ChartInfo,
};
use sasa::{
    AudioClip, AudioManager, Frame,
    backend::{Backend, BackendSetup},
};
use std::cell::RefCell;

#[derive(Default)]
pub(crate) struct SilentBackend(Option<BackendSetup>);
impl Backend for SilentBackend {
    fn setup(&mut self, setup: BackendSetup) -> Result<()> {
        self.0 = Some(setup);
        Ok(())
    }
    fn start(&mut self) -> Result<()> {
        Ok(())
    }
    fn consume_broken(&self) -> bool {
        false
    }
}

pub(crate) async fn editor_resource() -> Result<(Resource, Option<String>)> {
    let blank: SafeTexture = Texture2D::from_rgba8(1, 1, &[255; 4]).into();
    let silence = AudioClip::from_raw(vec![Frame::default(); 48], 48000);
    let pack = crate::skin::load().await?;
    let emitter = ParticleEmitter::new(&pack, 1., pack.info.hide_particles)?;
    let mut config = Config::default();
    config.disable_effect = true;
    config.mods.insert(prpr::config::Mods::AUTOPLAY);
    config.interactive = false;
    config.sample_count = 1;
    #[cfg(not(target_os = "android"))]
    {
        // ALSA/Pulse may advertise very high rates. Keep the desktop harness
        // at the normal media rate with a bounded callback buffer.
        config.preferred_sample_rate = Some(48_000);
        config.audio_buffer_size = Some(1024);
    }
    let (mut audio, warning) = match prpr::ext::create_audio_manger(&config) {
        Ok(audio) => (audio, None),
        Err(error) => (
            AudioManager::new(SilentBackend::default())?,
            Some(format!("Audio output unavailable: {error:#}")),
        ),
    };
    let sfx_click = audio.create_sfx(pack.sfx_click.clone(), Some(prpr::core::BUFFER_SIZE))?;
    let sfx_drag = audio.create_sfx(pack.sfx_drag.clone(), Some(prpr::core::BUFFER_SIZE))?;
    let sfx_flick = audio.create_sfx(pack.sfx_flick.clone(), Some(prpr::core::BUFFER_SIZE))?;
    let line_color = pack.info.color_perfect();
    Ok((
        Resource {
            config,
            info: ChartInfo {
                force_aspect_ratio: true,
                ..ChartInfo::default()
            },
            aspect_ratio: 16. / 9.,
            dpi: 160,
            last_vp: (0, 0, 0, 0),
            note_width: prpr::core::NOTE_WIDTH_RATIO_BASE as f32,
            time: 0.,
            alpha: 1.,
            judge_line_color: line_color,
            camera: Camera2D {
                zoom: vec2(1., -16. / 9.),
                ..Default::default()
            },
            background: blank.clone(),
            illustration: blank.clone(),
            icons: std::array::from_fn(|_| blank.clone()),
            mod_icons: std::array::from_fn(|_| blank.clone()),
            res_pack: pack,
            player: blank.clone(),
            icon_back: blank.clone(),
            icon_retry: blank.clone(),
            icon_resume: blank.clone(),
            icon_proceed: blank,
            emitter,
            audio,
            music: silence,
            track_length: 0.,
            sfx_click,
            sfx_drag,
            sfx_flick,
            extra_sfxs: Default::default(),
            chart_target: None,
            no_effect: true,
            note_buffer: RefCell::default(),
            model_stack: vec![Matrix::identity()],
        },
        warning,
    ))
}
