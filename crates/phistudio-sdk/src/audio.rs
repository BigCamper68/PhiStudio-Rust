//! Read-only decoded music and background analysis (`host/audio` v1).
use crate::{DocumentVersion, ProjectAssetsVersion};
use serde::{Deserialize, Serialize};

pub const MAX_PCM_FRAMES: usize = 4096;
pub const MAX_WAVEFORM_BUCKETS: usize = 2048;
pub const MAX_AUDIO_CANDIDATES: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioSourceVersion {
    pub assets: ProjectAssetsVersion,
    pub serial: u64,
}

/// `info`, `analyze`, `status`, and `cancel` take this document-bound query.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioQuery {
    pub version: DocumentVersion,
    #[serde(default)]
    pub assets: Option<ProjectAssetsVersion>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioInfo {
    pub source: AudioSourceVersion,
    pub sample_rate: u32,
    pub frames: u64,
    pub channels: u8,
    pub duration: f64,
    pub waveform_ready: bool,
    pub analysis_ready: bool,
    pub working: bool,
    #[serde(default)]
    pub cancelled: bool,
    pub progress: u16,
    pub error: Option<String>,
}

/// `pcm`: interleaved stereo f32 little-endian, encoded as base64.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioPcmQuery {
    pub version: DocumentVersion,
    pub source: AudioSourceVersion,
    pub offset: u64,
    pub frames: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioPcm {
    pub source: AudioSourceVersion,
    pub offset: u64,
    pub frames: usize,
    pub eof: bool,
    pub f32le_base64: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioWaveformQuery {
    pub version: DocumentVersion,
    pub source: AudioSourceVersion,
    pub start: f64,
    pub end: f64,
    pub buckets: usize,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AudioWaveformBucket {
    pub min: f32,
    pub max: f32,
    pub rms: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioCandidatesQuery {
    pub version: DocumentVersion,
    pub source: AudioSourceVersion,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioCandidate {
    pub seconds: f64,
    pub strength: [f32; 4],
    pub band: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioCandidates {
    pub source: AudioSourceVersion,
    pub offset: usize,
    pub total: usize,
    pub candidates: Vec<AudioCandidate>,
}
