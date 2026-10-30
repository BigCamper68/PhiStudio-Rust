//! Audition staged note heads through the existing skin SFX player.
use super::*;
use phistudio_core::NoteKind;

#[derive(Default)]
pub(super) struct Audio {
    heads: [Vec<f64>; 3],
    cursor: [usize; 3],
    clock: Option<(f64, [f32; 2])>,
}
impl Audio {
    pub fn new(plan: &AudioNotePreview, tempo: &phistudio_core::TempoMap) -> Self {
        let mut audio = Self::default();
        for item in &plan.notes {
            if item.note.is_fake != 0 {
                continue;
            }
            let kind = match item.note.kind {
                NoteKind::Tap | NoteKind::Hold => 0,
                NoteKind::Flick => 1,
                NoteKind::Drag => 2,
            };
            audio.heads[kind].push(tempo.seconds_at(item.note.start_time.as_f64()));
        }
        for heads in &mut audio.heads {
            heads.sort_by(f64::total_cmp);
        }
        audio
    }
    fn advance(
        &mut self,
        seconds: f64,
        playing: bool,
        seek: bool,
        delays: [f32; 2],
        mut play: impl FnMut(NoteKind) -> anyhow::Result<()>,
    ) -> anyhow::Result<usize> {
        if !seconds.is_finite() {
            self.clock = None;
            return Ok(0);
        }
        let reset = self.clock.is_none_or(|(last, old_delays)| {
            seek || seconds < last || delays != old_delays || (!playing && seconds != last)
        });
        let initial = self.clock.is_none() && !seek;
        self.clock = Some((seconds, delays));
        let mut played = 0;
        for (i, kind) in [NoteKind::Tap, NoteKind::Flick, NoteKind::Drag]
            .into_iter()
            .enumerate()
        {
            let at = seconds - f64::from(delays[usize::from(i == 2)]) / 1000.;
            if reset {
                self.cursor[i] =
                    self.heads[i].partition_point(|t| if initial { *t < at } else { *t <= at });
            }
            if playing && !seek {
                while self.heads[i].get(self.cursor[i]).is_some_and(|t| *t <= at) {
                    self.cursor[i] += 1;
                    play(kind)?;
                    played += 1;
                }
            }
        }
        Ok(played)
    }
}
impl Workbench {
    pub(crate) fn audition_audio_notes(
        &mut self,
        seconds: f64,
        seek: bool,
        play: impl FnMut(NoteKind) -> anyhow::Result<()>,
    ) -> anyhow::Result<usize> {
        let valid = self
            .audio_notes
            .preview
            .as_ref()
            .is_some_and(|(key, _)| self.audio_note_key().as_ref().ok() == Some(key));
        let playing = self.playing
            && !self.is_scrubbing()
            && !self.playtest.active()
            && !self.show_projects
            && self.render_error.is_none();
        let Some((_, prepared)) = &mut self.audio_notes.preview else {
            return Ok(0);
        };
        if !valid {
            prepared.audio.clock = None;
            return Ok(0);
        }
        prepared.audio.advance(
            seconds,
            playing,
            seek,
            [self.options.hitsound_delay_ms, self.options.drag_delay_ms],
            play,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn staged_heads_play_once_follow_delays_and_skip_pause_seek_and_old_notes() {
        let mut audio = Audio {
            heads: [vec![0., 1., 2.], vec![0.5], vec![0.5]],
            ..Default::default()
        };
        let mut heard = Vec::new();
        let mut step = |audio: &mut Audio, t, playing, seek, delays| {
            audio
                .advance(t, playing, seek, delays, |kind| {
                    heard.push(kind);
                    Ok(())
                })
                .unwrap()
        };
        assert_eq!(step(&mut audio, 0., false, false, [0., 100.]), 0);
        assert_eq!(step(&mut audio, 0., true, false, [0., 100.]), 1);
        assert_eq!(step(&mut audio, 0., true, false, [0., 100.]), 0);
        assert_eq!(step(&mut audio, 0.5, true, false, [0., 100.]), 1);
        assert_eq!(step(&mut audio, 0.61, true, false, [0., 100.]), 1);
        assert_eq!(step(&mut audio, 1.5, false, false, [0., 100.]), 0);
        assert_eq!(step(&mut audio, 1.5, true, false, [0., 100.]), 0);
        assert_eq!(step(&mut audio, 0.25, true, true, [0., 100.]), 0);
        assert_eq!(step(&mut audio, 0.61, true, false, [0., 100.]), 2);
        assert_eq!(step(&mut audio, 1.5, true, false, [500., 100.]), 0);
        assert_eq!(step(&mut audio, 2.5, true, false, [500., 100.]), 1);
        assert_eq!(
            heard,
            [
                NoteKind::Tap,
                NoteKind::Flick,
                NoteKind::Drag,
                NoteKind::Flick,
                NoteKind::Drag,
                NoteKind::Tap
            ]
        );
    }
}
