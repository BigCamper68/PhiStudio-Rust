//! Signed music transport driven by Phira's clock and sasa Music.
use anyhow::Result;
use prpr::time::TimeManager;
use sasa::{AudioClip, AudioManager, Music, MusicParams};
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicU32, Ordering},
};

static ACTIVE_MUSIC: Mutex<Weak<Mutex<Music>>> = Mutex::new(Weak::new());

pub(crate) fn suspend_audio() {
    let music = ACTIVE_MUSIC.lock().ok().and_then(|m| m.upgrade());
    if let Some(music) = music
        && let Ok(mut music) = music.lock()
    {
        let _ = music.pause();
    }
}

// Observe the actual backend rate without changing its PCM or timing.
struct OutputRate(Weak<AtomicU32>);
impl sasa::Renderer for OutputRate {
    fn alive(&self) -> bool {
        self.0.strong_count() != 0
    }
    fn render_mono(&mut self, rate: u32, _: &mut [f32]) {
        if let Some(output) = self.0.upgrade() {
            output.store(rate, Ordering::Relaxed);
        }
    }
    fn render_stereo(&mut self, rate: u32, data: &mut [f32]) {
        self.render_mono(rate, data);
    }
}

struct NoiseLowPass {
    cutoff: f32,
    from: f32,
    target: f32,
    elapsed: f32,
    coefficient: f32,
}
impl Default for NoiseLowPass {
    fn default() -> Self {
        Self {
            cutoff: 22_000.,
            from: 22_000.,
            target: 22_000.,
            elapsed: 0.1,
            coefficient: 0.,
        }
    }
}
impl NoiseLowPass {
    fn update(&mut self, blocked: bool, dt: f32, sample_rate: u32) -> f32 {
        let target = if blocked { 1500. } else { 22_000. };
        if target != self.target {
            self.from = self.cutoff;
            self.target = target;
            self.elapsed = 0.;
        }
        self.elapsed = (self.elapsed + dt.max(0.)).min(0.1);
        self.cutoff = self.from + (self.target - self.from) * (self.elapsed / 0.1);
        // sasa exposes a one-pole coefficient, not Unity's cutoff-frequency DSP.
        if self.cutoff == 22_000. {
            0.
        } else {
            (-std::f32::consts::TAU * self.cutoff / sample_rate.max(1) as f32).exp()
        }
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
    output_rate: Arc<AtomicU32>,
    rate_observer_added: bool,
    noise_filter: NoiseLowPass,
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
            output_rate: Arc::new(AtomicU32::new(48_000)),
            rate_observer_added: false,
            noise_filter: NoiseLowPass::default(),
        }
    }
    pub fn set_music(&mut self, audio: &mut AudioManager, clip: Option<AudioClip>) -> Result<()> {
        // Drop the previous renderer before replacing it. Revisions of the same
        // chart retain this music handle; only a resource change reloads music.
        if !self.rate_observer_added {
            audio.add_renderer(OutputRate(Arc::downgrade(&self.output_rate)))?;
            self.rate_observer_added = true;
        }
        self.noise_filter = NoiseLowPass::default();
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
    pub fn update_noise_filter(&mut self, blocked: bool, dt: f32) -> Result<()> {
        let coefficient =
            self.noise_filter
                .update(blocked, dt, self.output_rate.load(Ordering::Relaxed));
        if coefficient != self.noise_filter.coefficient {
            if let Some(music) = &self.music {
                music
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Noise Field audio lock failed"))?
                    .set_low_pass(coefficient)?;
            }
            self.noise_filter.coefficient = coefficient;
        }
        Ok(())
    }
    pub fn reset_noise_filter(&mut self) -> Result<()> {
        if self.noise_filter.coefficient != 0.
            && let Some(music) = &self.music
        {
            music
                .lock()
                .map_err(|_| anyhow::anyhow!("Noise Field audio lock failed"))?
                .set_low_pass(0.)?;
        }
        self.noise_filter = NoiseLowPass::default();
        Ok(())
    }
    pub fn set_rate(&mut self, audio: &mut AudioManager, rate: f64) -> Result<()> {
        anyhow::ensure!(
            phistudio_core::PLAYBACK_RATES.contains(&rate),
            "Choose playback rate 0.25, 0.5, 0.75, 1.0 or 1.5"
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
        serde_json::json!({"noise_cutoff_hz":self.noise_filter.cutoff,"rate":self.clock.speed,"audio_seconds":self.audio_time(), "music":music,
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
    fn noise_filter_lerps_from_current_cutoff_and_restores_audio() {
        let mut filter = NoiseLowPass::default();
        assert_eq!(filter.update(false, 1., 48000), 0.);
        let coefficient = filter.update(true, 0.05, 48000);
        assert_eq!(filter.cutoff, 11750.);
        assert!(coefficient > 0. && coefficient < 1.);
        filter.update(false, 0.05, 48000);
        assert_eq!(filter.cutoff, 16875.);
        assert_eq!(filter.update(false, 0.05, 48000), 0.);
        let coefficient = filter.update(true, 0.1, 48000);
        assert_eq!(filter.cutoff, 1500.);
        assert!((coefficient - (-std::f32::consts::TAU * 1500. / 48000.).exp()).abs() < 1e-6);
        assert!(filter.update(true, 0., 96000) > coefficient);
        let mut transport = Transport::new();
        transport.noise_filter = filter;
        transport.noise_filter.coefficient = coefficient;
        transport.reset_noise_filter().unwrap();
        assert_eq!(transport.noise_filter.cutoff, 22000.);
        assert_eq!(transport.noise_filter.coefficient, 0.);
    }
    #[test]
    fn rate_changes_keep_cursor_offset_pause_and_seek_on_original_clock() {
        let now = Rc::new(Cell::new(0.));
        let time = now.clone();
        let mut transport = Transport::new();
        transport.clock = TimeManager::manual(Box::new(move || time.get()));
        transport.clock.pause();
        let mut audio = AudioManager::new(crate::resources::SilentBackend::default()).unwrap();
        transport.synchronize(8., false, true, -0.2).unwrap();
        for (i, rate) in phistudio_core::PLAYBACK_RATES.into_iter().enumerate() {
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
        for invalid in [0., -1., 0.3, 2., f64::NAN, f64::INFINITY] {
            assert!(transport.set_rate(&mut audio, invalid).is_err());
            assert_eq!(transport.clock.speed, 1.5);
        }
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
