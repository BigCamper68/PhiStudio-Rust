//! Short editor feedback on the existing native mixer, independent of chart audio.
use anyhow::{Result, ensure};
use phistudio_core::{EditorSoundStyle as Style, EditorSoundTone as Tone};
use sasa::{AudioClip, Frame, Renderer};
use std::{
    io::Read,
    path::Path,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
};

const VOICES: usize = 8;
static ACTIVE: Mutex<Weak<AtomicU64>> = Mutex::new(Weak::new());
pub(crate) fn suspend() {
    if let Some(gate) = ACTIVE.lock().ok().and_then(|v| v.upgrade()) {
        gate.fetch_or(1, Ordering::AcqRel);
    }
}
struct Voice {
    clip: AudioClip,
    position: f64,
    gain: f32,
    rate: f64,
    generation: u64,
}
struct Mixer {
    receiver: Receiver<Voice>,
    voices: Vec<Voice>,
    gate: Arc<AtomicU64>,
}
impl Mixer {
    fn render(&mut self, sample_rate: u32, data: &mut [f32], channels: usize) {
        let generation = self.gate.load(Ordering::Acquire);
        self.voices.retain(|v| v.generation == generation);
        if generation & 1 != 0 {
            self.voices.clear();
            while self.receiver.try_recv().is_ok() {}
            return;
        }
        for _ in 0..VOICES {
            let Ok(voice) = self.receiver.try_recv() else {
                break;
            };
            if voice.generation != generation {
                continue;
            }
            if self.voices.len() == VOICES {
                self.voices.remove(0);
            }
            self.voices.push(voice);
        }
        for output in data.chunks_exact_mut(channels) {
            let mut left = 0.;
            let mut right = 0.;
            for voice in &mut self.voices {
                if let Some(frame) = voice.clip.sample(voice.position) {
                    left += frame.0 * voice.gain;
                    right += frame.1 * voice.gain;
                }
                voice.position += voice.rate / sample_rate.max(1) as f64;
            }
            output[0] += if channels == 1 {
                (left + right) * 0.5
            } else {
                left
            };
            if channels == 2 {
                output[1] += right;
            }
        }
        self.voices.retain(|v| v.position < v.clip.length());
    }
}
impl Renderer for Mixer {
    fn alive(&self) -> bool {
        Arc::strong_count(&self.gate) > 1 || !self.voices.is_empty()
    }
    fn render_mono(&mut self, rate: u32, data: &mut [f32]) {
        self.render(rate, data, 1);
    }
    fn render_stereo(&mut self, rate: u32, data: &mut [f32]) {
        self.render(rate, data, 2);
    }
}
pub struct InterfaceAudio {
    sender: SyncSender<Voice>,
    gate: Arc<AtomicU64>,
}
impl InterfaceAudio {
    pub fn set_muted(&mut self, muted: bool) {
        // Every mute/resume transition invalidates queued and active feedback.
        let mut generation = self.gate.load(Ordering::Acquire);
        while (generation & 1 != 0) != muted {
            match self.gate.compare_exchange_weak(
                generation,
                generation.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(current) => generation = current,
            }
        }
    }
    pub fn play(&mut self, clip: AudioClip, volume: f32, pitch: f32) {
        let generation = self.gate.load(Ordering::Acquire);
        if !volume.is_finite() || !pitch.is_finite() || generation & 1 != 0 {
            return;
        }
        // A full queue discards excess feedback instead of blocking editing.
        let _ = self.sender.try_send(Voice {
            clip,
            generation,
            position: 0.,
            gain: volume.clamp(0., 1.),
            rate: pitch.clamp(0.25, 4.) as f64,
        });
    }
}
impl crate::Preview {
    pub fn create_interface_audio(&mut self) -> Result<InterfaceAudio> {
        let (sender, receiver) = mpsc::sync_channel(VOICES);
        let gate = Arc::new(AtomicU64::new(0));
        self.resource.audio.add_renderer(Mixer {
            receiver,
            voices: Vec::with_capacity(VOICES),
            gate: gate.clone(),
        })?;
        *ACTIVE
            .lock()
            .map_err(|_| anyhow::anyhow!("Interface audio lock failed"))? = Arc::downgrade(&gate);
        Ok(InterfaceAudio { sender, gate })
    }
}

/// Original synthesized assets: no Geometry Dash or mod resources are bundled.
pub fn interface_tone(style: Style, tone: Tone) -> AudioClip {
    let (frequency, duration, decay) = match tone {
        Tone::Tick => (1800., 0.028, 180.),
        Tone::Pop => (640., 0.055, 85.),
        Tone::Wood => (370., 0.045, 110.),
        Tone::Glass => (1250., 0.09, 52.),
        Tone::Chime => (880., 0.14, 35.),
        Tone::Bass => (180., 0.08, 60.),
    };
    let frames = (0..(duration * 48000.) as usize)
        .map(|i| {
            let t = i as f32 / 48000.;
            let phase = std::f32::consts::TAU * frequency * t;
            let signal = match style {
                Style::Soft => (phase + 1.8 * (-100. * t).exp()).sin(),
                Style::Mechanical => {
                    phase.sin() * 0.6 + (phase * 2.73).sin() * 0.25 + (phase * 6.17).sin() * 0.15
                }
                Style::Arcade => phase.sin() * 0.75 + (phase * 3.).sin() * 0.25,
            };
            let envelope =
                (t / 0.002).min(1.) * (-decay * t).exp() * ((duration - t) / 0.005).clamp(0., 1.);
            let value = signal * envelope * 0.3;
            Frame(value, value)
        })
        .collect();
    AudioClip::from_raw(frames, 48000)
}

pub fn load_interface_sound(path: &Path) -> Result<AudioClip> {
    ensure!(
        std::fs::metadata(path)?.is_file(),
        "Editor sound must be a regular file"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= 2 * 1024 * 1024,
        "Editor sound must contain 1 byte–2 MiB"
    );
    let clip = AudioClip::new(bytes)?;
    ensure!(
        clip.length().is_finite() && clip.length() > 0. && clip.length() <= 2.,
        "Use an editor sound no longer than 2 seconds"
    );
    ensure!(
        clip.frames()
            .iter()
            .all(|f| f.0.is_finite() && f.1.is_finite()),
        "Editor sound contains invalid samples"
    );
    Ok(clip)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mixer() -> (InterfaceAudio, Mixer) {
        let (sender, receiver) = mpsc::sync_channel(VOICES);
        let gate = Arc::new(AtomicU64::new(0));
        (
            InterfaceAudio {
                sender,
                gate: gate.clone(),
            },
            Mixer {
                receiver,
                voices: Vec::with_capacity(VOICES),
                gate,
            },
        )
    }
    #[test]
    fn feedback_gain_pitch_polyphony_and_mute_use_the_same_mixer() {
        let (mut output, mut mixer) = mixer();
        let clip = AudioClip::from_raw(vec![Frame(0.5, 0.25); 100], 1000);
        output.play(clip.clone(), 0.4, 2.);
        let mut samples = [0.; 40];
        mixer.render_stereo(1000, &mut samples);
        assert!((samples[0] - 0.2).abs() < 1e-6);
        assert!((samples[1] - 0.1).abs() < 1e-6);
        assert!((mixer.voices[0].position - 0.04).abs() < 1e-6);
        for _ in 0..100 {
            output.play(clip.clone(), 0.2, 1.);
        }
        mixer.render_mono(1000, &mut [0.; 10]);
        assert_eq!(mixer.voices.len(), VOICES);
        output.set_muted(true);
        let mut silence = [0.; 10];
        mixer.render_mono(1000, &mut silence);
        assert_eq!(silence, [0.; 10]);
        assert!(mixer.voices.is_empty());
    }
    #[test]
    fn suspend_and_resume_before_the_next_audio_callback_discards_stale_sounds() {
        let (mut output, mut mixer) = mixer();
        let clip = AudioClip::from_raw(vec![Frame(0.5, 0.5); 100], 1000);
        output.play(clip.clone(), 1., 1.);
        mixer.render_mono(1000, &mut [0.; 1]);
        output.play(clip, 1., 1.);
        output.set_muted(true);
        output.set_muted(false);
        let mut samples = [0.; 10];
        mixer.render_mono(1000, &mut samples);
        assert_eq!(samples, [0.; 10]);
        assert!(mixer.voices.is_empty());
    }
    #[test]
    fn every_builtin_is_short_finite_and_fades_to_silence() {
        for style in Style::ALL {
            for tone in Tone::ALL {
                let clip = interface_tone(style, tone);
                assert!(clip.length() > 0. && clip.length() < 0.2);
                assert!(clip.frames().iter().any(|f| f.0.abs() > 0.01));
                assert!(
                    clip.frames()
                        .iter()
                        .all(|f| f.0.is_finite() && f.0.abs() <= 0.31)
                );
                assert!(clip.frames().last().unwrap().0.abs() < 0.001);
            }
        }
    }
}
