//! Staged note sounds use the original native SFX backend without chart edits.
use crate::Preview;
use anyhow::{Result, ensure};
use phistudio_core::{NoteCreationSounds, NoteKind};
use sasa::{AudioClip, PlaySfxParams, Sfx};
use std::path::Path;

#[derive(Clone)]
pub struct PreparedNoteAudition {
    clips: Vec<AudioClip>,
    volume: f32,
}
pub(crate) struct NoteAudition {
    key: (u64, u64),
    sounds: Vec<Sfx>,
    volume: f32,
}
pub async fn prepare_note_audition(
    assets: Option<&Path>,
    skin: Option<&Path>,
    reference: Option<&str>,
    volume: f64,
) -> Result<PreparedNoteAudition> {
    ensure!(
        volume.is_finite() && (0. ..=1.).contains(&volume),
        "Note sound volume must be between 0 and 100%"
    );
    let mut clips = Vec::with_capacity(3);
    for kind in [NoteKind::Tap, NoteKind::Flick, NoteKind::Drag] {
        if reference.is_some()
            && let Some(first) = clips.first()
        {
            clips.push(AudioClip::clone(first));
        } else {
            clips.push(crate::sound_gain::load_note_sound(assets, skin, reference, kind).await?);
        }
    }
    Ok(PreparedNoteAudition {
        clips,
        volume: volume as f32,
    })
}
impl Preview {
    pub fn set_note_audition(
        &mut self,
        source: Option<((u64, u64), &PreparedNoteAudition)>,
    ) -> Result<()> {
        let Some((key, prepared)) = source else {
            self.note_audition = None;
            return Ok(());
        };
        if self
            .note_audition
            .as_ref()
            .is_some_and(|current| current.key == key)
        {
            return Ok(());
        }
        let sounds = prepared
            .clips
            .iter()
            .map(|clip| self.resource.create_sfx(clip.clone()))
            .collect::<Result<Vec<_>>>()?;
        self.note_audition = Some(NoteAudition {
            key,
            sounds,
            volume: prepared.volume,
        });
        Ok(())
    }
    pub fn audition_staged_note(&mut self, kind: NoteKind) -> Result<()> {
        if let Some(prepared) = &mut self.note_audition {
            let volume = self.resource.config.volume_sfx * prepared.volume;
            if self.resource.config.volume_sfx > 0.01 && volume > 0. {
                prepared.sounds[NoteCreationSounds::index(kind)]
                    .play(PlaySfxParams { amplifier: volume })?;
            }
            Ok(())
        } else {
            self.audition(None, kind)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_cues_match_native_sources_without_writing_gain_files() {
        let assets = tempfile::tempdir().unwrap();
        for reference in [None, Some("flick.mp3")] {
            let prepared = pollster::block_on(prepare_note_audition(
                Some(assets.path()),
                None,
                reference,
                0.6,
            ))
            .unwrap();
            assert_eq!(prepared.volume, 0.6);
            for kind in [
                NoteKind::Tap,
                NoteKind::Hold,
                NoteKind::Flick,
                NoteKind::Drag,
            ] {
                let source = pollster::block_on(crate::sound_gain::load_note_sound(
                    Some(assets.path()),
                    None,
                    reference,
                    kind,
                ))
                .unwrap();
                let staged = &prepared.clips[NoteCreationSounds::index(kind)];
                assert_eq!(staged.sample_rate(), source.sample_rate());
                assert_eq!(staged.frames().len(), source.frames().len());
                assert!(staged.frames().iter().zip(source.frames()).all(|(a, b)| {
                    a.0.to_bits() == b.0.to_bits() && a.1.to_bits() == b.1.to_bits()
                }));
            }
        }
        assert_eq!(std::fs::read_dir(assets.path()).unwrap().count(), 0);
        assert!(pollster::block_on(prepare_note_audition(None, None, None, f64::NAN)).is_err());
    }
}
