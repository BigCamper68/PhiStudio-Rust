use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use phistudio_sdk::*;

impl Guide {
    pub(crate) fn plugin_analyze(&mut self) {
        self.manual_cancel = false;
        if self.full_data && self.result.is_some() {
            self.status = Status::Ready;
        }
    }
    pub(crate) fn plugin_info(&self) -> Result<AudioInfo, String> {
        let key = self.key.ok_or("Decoded music is unavailable")?;
        Ok(AudioInfo {
            source: AudioSourceVersion {
                assets: ProjectAssetsVersion {
                    session: key.session,
                    revision: key.assets,
                },
                serial: self.source_serial,
            },
            sample_rate: key.sr,
            frames: key.frames as u64,
            channels: 2,
            duration: key.frames as f64 / f64::from(key.sr),
            waveform_ready: self.waveform.is_some(),
            analysis_ready: self.full_data && self.result.is_some(),
            working: self.working,
            cancelled: matches!(self.status, Status::Canceled),
            progress: self
                .control
                .progress
                .load(std::sync::atomic::Ordering::Relaxed)
                .min(1000) as u16,
            error: match &self.status {
                Status::Failed(error) => Some(error.clone()),
                _ => None,
            },
        })
    }
    fn check_plugin_source(&self, source: AudioSourceVersion) -> Result<AudioInfo, String> {
        let info = self.plugin_info()?;
        if info.source != source {
            return Err("Decoded music changed".into());
        }
        Ok(info)
    }
    pub(crate) fn plugin_pcm(&self, query: AudioPcmQuery) -> Result<AudioPcm, String> {
        let info = self.check_plugin_source(query.source)?;
        if query.frames == 0 || query.frames > MAX_PCM_FRAMES || query.offset > info.frames {
            return Err("PCM page is outside the supported bounds".into());
        }
        let source = self.source.as_ref().ok_or("Decoded music is unavailable")?;
        let start = query.offset as usize;
        let end = (start + query.frames).min(source.0.frame_count());
        let mut bytes = Vec::with_capacity((end - start) * 8);
        for sample in &source.0.frames()[start..end] {
            let (left, right) = (sample.0, sample.1);
            if !left.is_finite() || !right.is_finite() {
                return Err("Decoded music contains invalid samples".into());
            }
            bytes.extend_from_slice(&left.to_le_bytes());
            bytes.extend_from_slice(&right.to_le_bytes());
        }
        Ok(AudioPcm {
            source: info.source,
            offset: query.offset,
            frames: end - start,
            eof: end == source.0.frame_count(),
            f32le_base64: STANDARD.encode(bytes),
        })
    }
    pub(crate) fn plugin_waveform(
        &self,
        query: AudioWaveformQuery,
    ) -> Result<Vec<AudioWaveformBucket>, String> {
        let info = self.check_plugin_source(query.source)?;
        if query.buckets == 0
            || query.buckets > MAX_WAVEFORM_BUCKETS
            || !query.start.is_finite()
            || !query.end.is_finite()
            || query.start < 0.
            || query.end <= query.start
            || query.end > info.duration
        {
            return Err("Waveform range is outside the supported bounds".into());
        }
        let waveform = self
            .waveform
            .as_ref()
            .ok_or("Waveform analysis is not ready")?;
        let step = (query.end - query.start) / query.buckets as f64;
        Ok((0..query.buckets)
            .map(|i| {
                let range = waveform.range(
                    query.start + i as f64 * step,
                    query.start + (i + 1) as f64 * step,
                );
                AudioWaveformBucket {
                    min: range.min,
                    max: range.max,
                    rms: range.rms,
                }
            })
            .collect())
    }
    pub(crate) fn plugin_candidates(
        &self,
        query: AudioCandidatesQuery,
    ) -> Result<AudioCandidates, String> {
        self.check_plugin_source(query.source)?;
        let result = self
            .result
            .as_ref()
            .filter(|_| self.full_data)
            .ok_or("Audio analysis is not ready")?;
        if query.limit == 0
            || query.limit > MAX_AUDIO_CANDIDATES
            || query.offset > result.candidates.len()
        {
            return Err("Audio candidate page is outside the supported bounds".into());
        }
        Ok(AudioCandidates {
            source: query.source,
            offset: query.offset,
            total: result.candidates.len(),
            candidates: result
                .candidates
                .iter()
                .skip(query.offset)
                .take(query.limit)
                .map(|c| AudioCandidate {
                    seconds: c.seconds,
                    strength: c.strength,
                    band: c.band,
                })
                .collect(),
        })
    }
}
