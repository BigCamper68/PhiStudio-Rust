//! Offline, deterministic audio guides. No chart edits, playback or model runtime.
pub mod cache;
mod onset;
mod spectrum;
mod statistics;
mod waveform;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
};
pub use waveform::{WaveRange, Waveform};

pub const ALGORITHM_VERSION: u32 = 1;
pub const MAX_SECONDS: usize = 30 * 60;
pub const FRAME_RATE: usize = 100;

/// Implement on an immutable shared decoder clip; analysis never copies the PCM.
pub trait StereoSource {
    fn sample_rate(&self) -> u32;
    fn frame_count(&self) -> usize;
    fn frame(&self, index: usize) -> [f32; 2];
}
#[derive(Clone, Default)]
pub struct Control {
    pub canceled: Arc<AtomicBool>,
    pub progress: Arc<AtomicU32>,
}
impl Control {
    pub fn cancel(&self) {
        self.canceled.store(true, Ordering::Release);
    }
    pub fn check(&self) -> Result<(), Error> {
        if self.canceled.load(Ordering::Acquire) {
            Err(Error::Canceled)
        } else {
            Ok(())
        }
    }
    pub(crate) fn progress(&self, value: u32) -> Result<(), Error> {
        self.check()?;
        self.progress.store(value.min(1000), Ordering::Relaxed);
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Canceled,
    InvalidAudio,
    TooLong,
    Cache,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Canceled => "Audio analysis canceled",
            Self::InvalidAudio => "Audio analysis requires finite stereo samples at 8–192 kHz",
            Self::TooLong => "Audio guides support music up to 30 minutes",
            Self::Cache => "Audio analysis cache is invalid",
        })
    }
}
impl std::error::Error for Error {}

pub(crate) fn validate(source: &impl StereoSource) -> Result<(), Error> {
    let sr = source.sample_rate();
    if !(8000..=192000).contains(&sr) || source.frame_count() == 0 {
        return Err(Error::InvalidAudio);
    }
    if source.frame_count() > sr as usize * MAX_SECONDS {
        return Err(Error::TooLong);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub seconds: f64,
    /// Relative salience for All / Low / Mid / High. These are not probabilities.
    pub strength: [f32; 4],
    pub band: usize,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Onset {
    pub seconds: f64,
    pub strength: f32,
    pub band: usize,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selection {
    pub sensitivity: f32,
    pub min_gap: f64,
    pub band: usize,
}
impl Default for Selection {
    fn default() -> Self {
        Self {
            sensitivity: 0.5,
            min_gap: 0.03,
            band: 0,
        }
    }
}
pub struct Analysis {
    pub waveform: Arc<Waveform>,
    pub candidates: Vec<Candidate>,
}
impl Analysis {
    /// Adjust display density/frequency focus without repeating spectral analysis.
    pub fn select(&self, selection: Selection, control: &Control) -> Result<Vec<Onset>, Error> {
        control.check()?;
        let sensitivity = if selection.sensitivity.is_finite() {
            selection.sensitivity.clamp(0., 1.)
        } else {
            0.5
        };
        let gap = if selection.min_gap.is_finite() {
            selection.min_gap.clamp(0.02, 0.5)
        } else {
            0.03
        };
        let band = selection.band.min(3);
        let threshold = 0.80 - 0.55 * sensitivity;
        let mut candidates: Vec<_> = self
            .candidates
            .iter()
            .filter(|c| c.strength[band] >= threshold)
            .collect();
        candidates.sort_unstable_by(|a, b| {
            b.strength[band]
                .total_cmp(&a.strength[band])
                .then(a.seconds.total_cmp(&b.seconds))
        });
        let mut selected = std::collections::BTreeMap::new();
        let gap = (gap * 1e6).round() as i64;
        for (index, candidate) in candidates.into_iter().enumerate() {
            if index % 256 == 0 {
                control.check()?;
            }
            let at = (candidate.seconds * 1e6).round() as i64;
            if selected
                .range((at - gap + 1)..=(at + gap - 1))
                .next()
                .is_none()
            {
                let dominant = if band == 0 { candidate.band } else { band };
                selected.insert(
                    at,
                    Onset {
                        seconds: candidate.seconds,
                        strength: candidate.strength[band],
                        band: dominant,
                    },
                );
            }
        }
        Ok(selected.into_values().collect())
    }
}

/// Waveform is published before the slower onset stages finish.
pub fn analyze(
    source: &impl StereoSource,
    control: &Control,
    waveform_ready: impl FnOnce(Arc<Waveform>),
) -> Result<Analysis, Error> {
    validate(source)?;
    control.check()?;
    let (waveform, energy, dc) = waveform::build(source, control)?;
    let waveform = Arc::new(waveform);
    waveform_ready(Arc::clone(&waveform));
    let candidates = onset::detect(source, &energy, dc, control)?;
    control.progress(1000)?;
    Ok(Analysis {
        waveform,
        candidates,
    })
}

pub fn waveform_only(
    source: &impl StereoSource,
    control: &Control,
) -> Result<Arc<Waveform>, Error> {
    validate(source)?;
    let (waveform, _, _) = waveform::build(source, control)?;
    control.progress(1000)?;
    Ok(Arc::new(waveform))
}
