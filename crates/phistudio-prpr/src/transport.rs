//! Signed music transport driven by Phira's clock and sasa Music.
use anyhow::Result;
use prpr::time::TimeManager;
use sasa::{AudioClip, AudioManager, Music, MusicParams};
use std::sync::{Arc, Mutex, Weak};

static ACTIVE_MUSIC: Mutex<Weak<Mutex<Music>>> = Mutex::new(Weak::new());

pub(crate) fn suspend_audio() {
    let music = ACTIVE_MUSIC.lock().ok().and_then(|m| m.upgrade());
    if let Some(music) = music
        && let Ok(mut music) = music.lock()
    {
        let _ = music.pause();
    }
}

pub(crate) struct Transport {
    clock: TimeManager,
    music: Option<Arc<Mutex<Music>>>,
    clip: Option<AudioClip>,
    length: f64,
    offset: f64,
    music_playing: bool,
    music_volume: f32,
    cursor: f64,
    pending_start: Option<(f64, f64)>,
    last_chart: f64,
}
impl Transport {
    pub fn new() -> Self {
        let mut clock = TimeManager::new(1., true);
        clock.pause();
        clock.seek_to(0.);
        Self {
            clock,
            music: None,
            clip: None,
            length: 0.,
            offset: 0.,
            music_playing: false,
            music_volume: 1.,
            cursor: 0.,
            pending_start: None,
            last_chart: 0.,
        }
    }
    pub fn set_music(&mut self, audio: &mut AudioManager, clip: Option<AudioClip>) -> Result<()> {
        // Drop the previous renderer before replacing it. Revisions of the same
        // chart retain this music handle; only a resource change reloads music.
        self.clip = clip.clone();
        self.music = None;
        self.music_playing = false;
        self.pending_start = None;
        self.length = clip.as_ref().map_or(0., AudioClip::length);
        if let Some(clip) = clip {
            self.music = Some(Arc::new(Mutex::new(audio.create_music(
                clip,
                MusicParams {
                    amplifier: self.music_volume,
                    playback_rate: self.clock.speed,
                    ..Default::default()
                },
            )?)));
        }
        *ACTIVE_MUSIC
            .lock()
            .map_err(|_| anyhow::anyhow!("Audio lifecycle lock failed"))? =
            self.music.as_ref().map_or_else(Weak::new, Arc::downgrade);
        self.position_music()?;
        Ok(())
    }
    pub fn set_rate(&mut self, audio: &mut AudioManager, rate: f64) -> Result<()> {
        anyhow::ensure!(
            matches!(rate, 0.5 | 0.75 | 1.),
            "Choose playback rate 0.5, 0.75 or 1.0"
        );
        if self.clock.speed != rate {
            let position = self.audio_time();
            self.clock.speed = rate;
            self.clock.seek_to(position);
            self.pending_start = None;
            self.set_music(audio, self.clip.clone())?;
        }
        Ok(())
    }
    pub fn set_volume(&mut self, volume: f32) -> Result<()> {
        if volume != self.music_volume {
            if let Some(music) = &self.music {
                music
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Audio volume lock failed"))?
                    .set_amplifier(volume)?;
            }
            self.music_volume = volume;
        }
        Ok(())
    }
    pub fn playing(&self) -> bool {
        !self.clock.paused()
    }
    pub fn chart_time(&self) -> f64 {
        if self.playing() {
            (self.audio_time() - self.offset).max(self.last_chart)
        } else {
            self.cursor
        }
    }
    pub fn report(&self) -> serde_json::Value {
        let music = self
            .music
            .as_ref()
            .and_then(|m| m.lock().ok())
            .map(|mut m| serde_json::json!({"position":m.position(), "paused":m.paused()}));
        serde_json::json!({"rate":self.clock.speed,"audio_seconds":self.audio_time(), "music":music,
            "waiting_for_audio":self.pending_start.is_some()})
    }
    fn audio_time(&self) -> f64 {
        self.pending_start.map_or_else(|| self.clock.now(), |p| p.0)
    }
    pub fn synchronize(
        &mut self,
        seconds: f64,
        playing: bool,
        seek: bool,
        offset: f64,
    ) -> Result<()> {
        let offset_changed = self.offset != offset;
        self.offset = offset;
        if seek || offset_changed {
            self.cursor = seconds;
            self.clock.seek_to(audio_time(seconds, offset));
            self.last_chart = self.clock.now() - offset;
            self.position_music()?;
        }
        if playing != self.playing() {
            if playing {
                self.clock.resume();
            } else {
                self.cursor = seconds;
                self.clock.pause();
                // A stopped Surface may deliver pause after a long wall-time
                // gap. Resume the selected cursor, not that unrendered gap.
                if !seek && !offset_changed {
                    self.clock.seek_to(seconds + self.offset);
                }
            }
            self.update_music_state()?;
        }
        Ok(())
    }
    pub fn advance(&mut self) -> Result<f64> {
        self.update_music_state()?;
        if self.music_playing
            && let Some(music) = &self.music
        {
            let (position, paused) = {
                let mut music = music
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Music control lock failed"))?;
                (music.position(), music.paused())
            };
            if let Some((target, started)) = self.pending_start {
                let elapsed = self.clock.real_time() - started;
                if !paused
                    && position >= target - 0.005
                    && position - target <= elapsed * self.clock.speed + 0.1
                {
                    self.clock.seek_to(position);
                    self.pending_start = None;
                } else {
                    anyhow::ensure!(elapsed < 3., "Audio output did not start");
                }
            } else if !paused {
                if (position - self.clock.now()).abs() > 0.08 {
                    self.clock.seek_to(position);
                } else {
                    self.clock.update(position);
                }
            }
        }
        let time = self.chart_time();
        self.last_chart = time;
        Ok(time)
    }
    fn position_music(&mut self) -> Result<()> {
        if let Some(music) = &self.music {
            let mut music = music
                .lock()
                .map_err(|_| anyhow::anyhow!("Music control lock failed"))?;
            music.pause()?;
            music.seek_to(self.clock.now().clamp(0., self.length))?;
        }
        self.music_playing = false;
        self.pending_start = None;
        self.update_music_state()
    }
    fn update_music_state(&mut self) -> Result<()> {
        let time = self.audio_time();
        let audible = self.playing() && time >= 0. && time < self.length;
        if audible != self.music_playing
            && let Some(music) = &self.music
        {
            let mut music = music
                .lock()
                .map_err(|_| anyhow::anyhow!("Music control lock failed"))?;
            if audible {
                music.seek_to(time)?;
                music.play()?;
                self.pending_start = Some((time, self.clock.real_time()));
                self.clock.wait();
            } else {
                music.pause()?;
                self.pending_start = None;
            }
            self.music_playing = audible;
        }
        Ok(())
    }
}

fn audio_time(chart: f64, offset: f64) -> f64 {
    // Match Java PlaybackTimeline's signed start, retaining negative editor
    // beats as an optional count-in before that start.
    if chart <= 0. {
        chart + offset.min(0.)
    } else {
        chart + offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    #[test]
    fn rate_changes_keep_cursor_offset_pause_and_seek_on_original_clock() {
        let now = Rc::new(Cell::new(0.));
        let time = now.clone();
        let mut transport = Transport::new();
        transport.clock = TimeManager::manual(Box::new(move || time.get()));
        transport.clock.pause();
        let mut audio = AudioManager::new(crate::resources::SilentBackend::default()).unwrap();
        transport.synchronize(8., false, true, -0.2).unwrap();
        for (i, rate) in [0.5, 0.75, 1.].into_iter().enumerate() {
            transport.set_rate(&mut audio, rate).unwrap();
            assert!(!transport.playing());
            now.set(i as f64 * 20. + 10.);
            assert!((transport.advance().unwrap() - 8.).abs() < 1e-10);
            transport.synchronize(8., true, false, -0.2).unwrap();
            now.set(now.get() + 2.);
            assert!((transport.advance().unwrap() - (8. + 2. * rate)).abs() < 1e-10);
            transport.synchronize(3., true, true, -0.2).unwrap();
            assert!((transport.advance().unwrap() - 3.).abs() < 1e-10);
            transport.synchronize(8., false, true, -0.2).unwrap();
        }
        assert!(transport.set_rate(&mut audio, 0.).is_err());
        assert_eq!(transport.clock.speed, 1.);
    }
    #[test]
    fn signed_offsets_pause_resume_seek_and_clock_use_one_timeline() {
        let near = |a: f64, b: f64| assert!((a - b).abs() < 1e-12, "{a} != {b}");
        for offset in [-0.35, 0., 0.25] {
            let now = Rc::new(Cell::new(0.));
            let time = now.clone();
            let mut transport = Transport::new();
            transport.clock = TimeManager::manual(Box::new(move || time.get()));
            transport.clock.pause();
            transport.synchronize(0., true, true, offset).unwrap();
            assert_eq!(transport.clock.now(), offset.min(0.));
            now.set(1.);
            let expected = 1. + offset.min(0.) - offset;
            near(transport.advance().unwrap(), expected);
            transport
                .synchronize(expected, false, false, offset)
                .unwrap();
            now.set(5.);
            near(transport.advance().unwrap(), expected);
            transport
                .synchronize(expected, true, false, offset)
                .unwrap();
            now.set(6.);
            near(transport.advance().unwrap(), expected + 1.);
            transport.synchronize(12., true, true, offset).unwrap();
            near(transport.advance().unwrap(), 12.);
            transport.synchronize(2., false, true, offset).unwrap();
            assert_eq!(transport.advance().unwrap(), 2.);
            now.set(30.);
            assert_eq!(transport.advance().unwrap(), 2.);
        }
    }
    #[test]
    fn audio_clock_corrections_do_not_replay_notes_but_explicit_seeks_can_rewind() {
        let now = Rc::new(Cell::new(0.));
        let time = now.clone();
        let mut transport = Transport::new();
        transport.clock = TimeManager::manual(Box::new(move || time.get()));
        transport.clock.pause();
        transport.synchronize(1., true, true, 0.).unwrap();
        assert_eq!(transport.advance().unwrap(), 1.);
        transport.clock.seek_to(0.8);
        assert_eq!(transport.advance().unwrap(), 1.);
        now.set(0.3);
        assert!((transport.advance().unwrap() - 1.1).abs() < 1e-12);
        transport.synchronize(0.2, true, true, 0.).unwrap();
        assert!((transport.advance().unwrap() - 0.2).abs() < 1e-12);
    }
    #[test]
    fn suspend_pause_excludes_time_without_surface_frames() {
        let now = Rc::new(Cell::new(0.));
        let time = now.clone();
        let mut transport = Transport::new();
        transport.clock = TimeManager::manual(Box::new(move || time.get()));
        transport.clock.pause();
        transport.synchronize(2., true, true, 0.25).unwrap();
        now.set(0.1);
        let cursor = transport.advance().unwrap();
        now.set(120.);
        transport.synchronize(cursor, false, false, 0.25).unwrap();
        now.set(200.);
        transport.synchronize(cursor, true, false, 0.25).unwrap();
        assert!((transport.advance().unwrap() - cursor).abs() < 1e-12);
    }
}
