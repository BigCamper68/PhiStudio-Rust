use super::*;

impl Preview {
    /// Share the original decoded PCM with a read-only background analyzer.
    /// This remains available when the platform has no audio output device.
    pub fn analysis_music(&self) -> Option<crate::ResourceAudioClip> {
        self.music_length.map(|_| self.resource.music.clone())
    }
    pub fn set_playback(&mut self, seconds: f64, playing: bool, seek: bool) -> Result<()> {
        let seek = seek || (!self.transport.playing() && seconds != self.last_time);
        self.needs_seek |= seek;
        if self.playtest.is_none() && self.transport.playing() && !playing {
            // Editor pause/scrub discards live particles; resume starts fresh.
            self.clear_particles = true;
        }
        self.transport
            .synchronize(seconds, playing, seek, self.music_offset)
    }

    pub fn advance_transport(&mut self) -> Result<f64> {
        if self.audio_available {
            self.resource.audio.recover_if_needed()?;
        }
        self.transport.advance()
    }

    pub fn transport_time(&self) -> f64 {
        self.transport.chart_time()
    }

    /// End of music in chart seconds, including the package/META audio offset.
    /// Decoded duration remains available even if audio output is unavailable.
    pub fn music_duration(&self) -> Option<f64> {
        self.music_length
            .map(|length| (length - self.music_offset).max(0.))
    }

    pub fn chart_duration(&self) -> f64 {
        self.music_duration().unwrap_or(self.resource.track_length)
    }

    pub fn set_playback_rate(&mut self, rate: f64) -> Result<()> {
        self.transport.set_rate(&mut self.resource.audio, rate)?;
        self.resource.config.speed = rate as f32;
        Ok(())
    }

    pub fn set_audio_levels(&mut self, music: f32, hitsounds: f32) -> Result<()> {
        anyhow::ensure!(
            music.is_finite() && hitsounds.is_finite(),
            "Audio levels must be finite"
        );
        self.transport.set_volume(music.clamp(0., 1.))?;
        self.resource.config.volume_music = music.clamp(0., 1.);
        self.resource.config.volume_sfx = hitsounds.clamp(0., 1.);
        Ok(())
    }

    pub fn playback_report(&self) -> serde_json::Value {
        let mut judged = 0;
        let mut holding = 0;
        let mut fake = 0;
        for note in self.chart.lines.iter().flat_map(|line| &line.notes) {
            if note.fake {
                fake += 1;
            }
            if matches!(note.judge, JudgeStatus::Judged) {
                judged += 1;
            }
            if matches!(note.judge, JudgeStatus::Hold(..)) {
                holding += 1;
            }
        }
        serde_json::json!({
            "editor_preview": {"note_scale":self.resource.config.note_scale,"note_width":self.resource.note_width,"aspect":self.resource.aspect_ratio,"viewport":self.resource.camera.viewport,"pause_bounds":self.pause_bounds,"mark_ids":self.resource.config.chart_debug,"highlight":self.editor_options.highlight_current_line,"custom_sounds":self.resource.extra_sfxs.len()},
            "chart_seconds": self.last_time, "playing": self.transport.playing(),
            "sound_delays_ms": [self.editor_options.hitsound_delay_ms, self.editor_options.drag_delay_ms],
            "audio_available": self.audio_available, "track_length": self.resource.track_length,
            "offset_seconds": self.music_offset, "combo": self.judge.combo(),
            "score": self.judge.score(), "counts": self.judge.counts(),
            "judged": judged, "holding": holding, "fake": fake,
            "transport": self.transport.report(),
            "latency_seconds": self.resource.audio.estimate_latency(),
            "judgement_events": self.judge.judgements.borrow().clone(),
        })
    }
}
