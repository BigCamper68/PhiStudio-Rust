//! Offline frames retain the original parser, Judge, particles, Paint and FX.
use crate::Preview;
use anyhow::{Result, ensure};
use macroquad::prelude::*;
use phistudio_core::video::{VideoPlan, VideoSettings};
use sasa::AudioClip;
use std::{
    io::{BufWriter, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

struct Cue {
    time: f64,
    clip: AudioClip,
}
pub struct MovieAudio {
    music: Option<AudioClip>,
    offset: f64,
    cues: Vec<Cue>,
    music_level: f32,
    sound_level: f32,
}
impl Preview {
    pub fn movie_audio(&self, settings: &VideoSettings) -> Result<MovieAudio> {
        ensure!(
            settings.music == 0.
                || self.music_key.as_ref().is_none_or(|k| k.1.is_none())
                || self.music_length.is_some(),
            "Could not decode music for video export"
        );
        let mut cues = Vec::new();
        for n in self
            .chart
            .lines
            .iter()
            .flat_map(|l| &l.notes)
            .filter(|n| !n.fake && settings.hitsounds > 0.)
        {
            use prpr::judge::HitSound;
            let clip = match &n.hitsound {
                HitSound::None => continue,
                HitSound::Click => &self.resource.res_pack.sfx_click,
                HitSound::Drag => &self.resource.res_pack.sfx_drag,
                HitSound::Flick => &self.resource.res_pack.sfx_flick,
                HitSound::Custom(name) => self
                    .chart
                    .hitsounds
                    .get(name)
                    .ok_or_else(|| anyhow::anyhow!("Missing video hitsound: {name}"))?,
            };
            let delay = if matches!(n.kind, prpr::core::NoteKind::Drag) {
                self.editor_options.drag_delay_ms
            } else {
                self.editor_options.hitsound_delay_ms
            };
            cues.push(Cue {
                time: n.time + f64::from(delay) / 1000.,
                clip: clip.clone(),
            });
        }
        cues.sort_by(|a, b| a.time.total_cmp(&b.time));
        Ok(MovieAudio {
            music: self.music_length.map(|_| self.resource.music.clone()),
            offset: self.music_offset,
            cues,
            music_level: settings.music,
            sound_level: settings.hitsounds,
        })
    }
    /// Exact output size; display scaling and UI density never affect the movie.
    pub fn begin_movie(&mut self, plan: VideoPlan) -> Result<()> {
        let mut maximum = 0;
        unsafe {
            macroquad::miniquad::gl::glGetIntegerv(
                macroquad::miniquad::gl::GL_MAX_TEXTURE_SIZE,
                &mut maximum,
            );
        }
        ensure!(
            plan.width <= maximum as u32 && plan.height <= maximum as u32,
            "Video resolution exceeds this GPU's texture limit"
        );
        unsafe { get_internal_gl() }.flush();
        let next = render_target(plan.width, plan.height);
        let retired = std::mem::replace(&mut self.target, next);
        retired.delete();
        self.resource.last_vp = (0, 0, 0, 0);
        self.chart.reset();
        self.judge.reset();
        self.needs_seek = false;
        self.clear_particles = true;
        self.last_time = f64::NEG_INFINITY;
        self.resource.config.volume_sfx = 0.;
        self.offline_step = Some(0.);
        Ok(())
    }
    pub fn movie_frame(&mut self, seconds: f64, step: f32) {
        self.offline_step = Some(step);
        self.render(seconds, self.target_viewport());
    }
    pub fn dispose_movie(&mut self) {
        unsafe { get_internal_gl() }.flush();
        set_default_camera();
        self.target.delete();
    }
}
impl MovieAudio {
    /// Stream PCM blocks to disk; memory does not grow with movie duration.
    pub fn write_wav(self, path: &Path, plan: VideoPlan, cancel: &AtomicBool) -> Result<()> {
        let bytes = plan
            .audio_frames()
            .checked_mul(4)
            .filter(|n| *n <= u64::from(u32::MAX) - 36)
            .ok_or_else(|| anyhow::anyhow!("Video audio exceeds the WAV size limit"))?
            as u32;
        let mut out = BufWriter::new(std::fs::File::create(path)?);
        out.write_all(b"RIFF")?;
        out.write_all(&(bytes + 36).to_le_bytes())?;
        out.write_all(b"WAVEfmt ")?;
        out.write_all(&16_u32.to_le_bytes())?;
        out.write_all(&1_u16.to_le_bytes())?;
        out.write_all(&2_u16.to_le_bytes())?;
        out.write_all(&48000_u32.to_le_bytes())?;
        out.write_all(&192000_u32.to_le_bytes())?;
        out.write_all(&4_u16.to_le_bytes())?;
        out.write_all(&16_u16.to_le_bytes())?;
        out.write_all(b"data")?;
        out.write_all(&bytes.to_le_bytes())?;
        let mut active: Vec<&Cue> = Vec::new();
        let mut next = 0;
        for index in 0..plan.audio_frames() {
            if index.is_multiple_of(4096) {
                ensure!(!cancel.load(Ordering::Relaxed), "Video export canceled");
            }
            let time = plan.start + index as f64 / 48000.;
            while next < self.cues.len() && self.cues[next].time <= time {
                active.push(&self.cues[next]);
                next += 1;
            }
            let mut sample = [0_f32; 2];
            if time + self.offset >= 0.
                && let Some(frame) = self
                    .music
                    .as_ref()
                    .and_then(|c| c.sample(time + self.offset))
            {
                sample = [frame.0 * self.music_level, frame.1 * self.music_level];
            }
            active.retain(|cue| {
                if let Some(frame) = cue.clip.sample(time - cue.time) {
                    sample[0] += frame.0 * self.sound_level;
                    sample[1] += frame.1 * self.sound_level;
                    true
                } else {
                    false
                }
            });
            for value in sample {
                out.write_all(&((value.clamp(-1., 1.) * 32767.).round() as i16).to_le_bytes())?;
            }
        }
        out.flush()?;
        out.get_ref().sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sasa::Frame;
    #[test]
    fn pcm_mix_respects_offsets_range_overlaps_levels_and_cancellation() {
        let clip = |l, r| AudioClip::from_raw(vec![Frame(l, r); 4800], 48000);
        let mix = MovieAudio {
            music: Some(clip(0.5, -0.5)),
            offset: -0.102,
            cues: vec![
                Cue {
                    time: 0.099,
                    clip: clip(0.2, 0.4),
                },
                Cue {
                    time: 0.101,
                    clip: clip(0.4, 0.2),
                },
            ],
            music_level: 0.5,
            sound_level: 0.5,
        };
        let plan = VideoPlan {
            width: 64,
            height: 64,
            fps: 100,
            frames: 1,
            start: 0.1,
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audio.wav");
        mix.write_wav(&path, plan, &AtomicBool::new(false)).unwrap();
        let wav = std::fs::read(&path).unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(wav.len(), 44 + 480 * 4);
        assert_eq!(&wav[22..24], &2_u16.to_le_bytes());
        let sample = |index: usize, channel: usize| {
            i16::from_le_bytes(
                wav[44 + index * 4 + channel * 2..46 + index * 4 + channel * 2]
                    .try_into()
                    .unwrap(),
            ) as f32
                / 32767.
        };
        for (index, left, right) in [(0, 0.1, 0.2), (60, 0.3, 0.3), (120, 0.55, 0.05)] {
            assert!((sample(index, 0) - left).abs() < 0.0001);
            assert!((sample(index, 1) - right).abs() < 0.0001);
        }
        let empty = MovieAudio {
            music: None,
            offset: 0.,
            cues: vec![],
            music_level: 0.,
            sound_level: 0.,
        };
        assert!(
            empty
                .write_wav(&path, plan, &AtomicBool::new(true))
                .is_err()
        );
    }
}
