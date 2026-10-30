//! Bake per-note gain into standard custom WAVs, understood by native Phira.
use anyhow::{Result, ensure};
use phistudio_core::{MAX_HITSOUND_BYTES, NoteKind};
use std::{io::Read, path::Path};

pub async fn render_hitsound_gain(
    assets: &Path,
    skin: Option<&Path>,
    reference: Option<&str>,
    kind: NoteKind,
    volume: f64,
) -> Result<Vec<u8>> {
    ensure!(
        volume.is_finite() && (0. ..1.).contains(&volume),
        "Note sound volume must be between 0 and 100%"
    );
    let cue = match reference {
        Some("tap.mp3") => Some("click"),
        Some("flick.mp3") => Some("flick"),
        Some("drag.mp3") => Some("drag"),
        None => Some(match kind {
            NoteKind::Tap | NoteKind::Hold => "click",
            NoteKind::Flick => "flick",
            NoteKind::Drag => "drag",
        }),
        Some(_) => None,
    };
    let bytes = if let Some(cue) = cue {
        crate::skin::sound_bytes(skin, cue).await?
    } else {
        let path = phistudio_core::hitsound_path(assets, reference.unwrap())?;
        let mut bytes = vec![];
        std::fs::File::open(path)?
            .take(MAX_HITSOUND_BYTES + 1)
            .read_to_end(&mut bytes)?;
        bytes
    };
    ensure!(
        !bytes.is_empty() && bytes.len() as u64 <= MAX_HITSOUND_BYTES,
        "Hitsound audio must contain 1 byte–8 MiB"
    );
    let clip = sasa::AudioClip::new(bytes)?;
    encode(&clip, volume)
}

fn encode(clip: &sasa::AudioClip, volume: f64) -> Result<Vec<u8>> {
    ensure!(
        clip.length().is_finite()
            && clip.length() > 0.
            && clip.length() <= 10.
            && clip.sample_rate() <= 192000,
        "Hitsounds must be at most 10 seconds and 192 kHz"
    );
    let size = clip
        .frames()
        .len()
        .checked_mul(4)
        .filter(|n| *n + 44 <= MAX_HITSOUND_BYTES as usize)
        .ok_or_else(|| anyhow::anyhow!("Rendered hitsound exceeds 8 MiB"))? as u32;
    let mut wav = Vec::with_capacity(size as usize + 44);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(size + 36).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&clip.sample_rate().to_le_bytes());
    wav.extend_from_slice(&(clip.sample_rate() * 4).to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&size.to_le_bytes());
    for frame in clip.frames() {
        for sample in [frame.0, frame.1] {
            ensure!(sample.is_finite(), "Hitsound contains invalid samples");
            let sample = (sample as f64 * volume * 32768.)
                .round()
                .clamp(-32768., 32767.) as i16;
            wav.extend_from_slice(&sample.to_le_bytes());
        }
    }
    Ok(wav)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pcm_roundtrip_preserves_timing_stereo_and_gain_including_silence() {
        let clip = sasa::AudioClip::from_raw(vec![sasa::Frame(0.8, -0.4); 480], 48000);
        let half = sasa::AudioClip::new(encode(&clip, 0.5).unwrap()).unwrap();
        assert_eq!(half.sample_rate(), 48000);
        assert_eq!(half.frames().len(), clip.frames().len());
        assert!((half.frames()[100].0 - 0.4).abs() < 1. / 32768.);
        assert!((half.frames()[100].1 + 0.2).abs() < 1. / 32768.);
        let mute = sasa::AudioClip::new(encode(&clip, 0.).unwrap()).unwrap();
        assert!(mute.frames().iter().all(|f| f.0 == 0. && f.1 == 0.));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use phistudio_core::*;
    #[test]
    fn exported_note_properties_and_gain_reach_original_phira_parser() {
        let assets = tempfile::tempdir().unwrap();
        let clip = sasa::AudioClip::from_raw(vec![sasa::Frame(0.8, -0.4); 480], 48000);
        let original = encode(&clip, 1.).unwrap();
        std::fs::write(assets.path().join("original.wav"), &original).unwrap();
        let wav = pollster::block_on(render_hitsound_gain(
            assets.path(),
            None,
            Some("original.wav"),
            NoteKind::Tap,
            0.5,
        ))
        .unwrap();
        let rendered = write_generated_hitsound(assets.path(), &wav).unwrap();
        let id = NoteId { line: 0, index: 0 };
        let mut e = Editor::new(ChartDocument::empty("Native note properties", 120.).unwrap());
        e.execute(Command::AddNote {
            line: 0,
            note: Note {
                judge_area: Some(2.5),
                tint_hit_effects: Some([12, 34, 56]),
                ..Default::default()
            },
        })
        .unwrap();
        e.execute(Command::SetNoteVolume {
            id,
            gain: Some(NoteSoundGain {
                source: Some("original.wav".into()),
                rendered: rendered.clone(),
                volume: 0.5,
            }),
        })
        .unwrap();
        let before = e.document().as_json().clone();
        let package = prepare_project_export(
            e.document(),
            &ProjectOptions::default(),
            Some(assets.path()),
            None,
        )
        .unwrap();
        let imported_root = tempfile::tempdir().unwrap();
        let imported =
            import_project(package.path(), imported_root.path(), "Native.pez", true).unwrap();
        let doc = load_chart(&imported.charts[imported.suggested.unwrap()].path).unwrap();
        assert_eq!(doc.as_json(), &before);
        let mut fs = crate::assets::ChartAssets::new(Some(&imported.assets)).unwrap();
        let native = pollster::block_on(prpr::parse::parse_rpe(
            &crate::compatibility::render_source(&doc).unwrap(),
            &mut fs,
            prpr::core::ChartExtra::default(),
            false,
        ))
        .unwrap();
        let note = &native.lines[0].notes[0];
        assert_eq!(note.judge_area, 2.5);
        let c = note.fx_color.unwrap();
        assert!(
            (c.r - 12. / 255.).abs() < 1e-6
                && (c.g - 34. / 255.).abs() < 1e-6
                && (c.b - 56. / 255.).abs() < 1e-6
        );
        assert!(matches!(&note.hitsound,prpr::judge::HitSound::Custom(p) if p==&rendered));
        let sound = &native.hitsounds[&rendered];
        assert!((sound.frames()[100].0 - 0.4).abs() < 2. / 32768.);
        assert_eq!(sound.frames().len(), clip.frames().len());
        assert_eq!(
            std::fs::read(assets.path().join("original.wav")).unwrap(),
            original
        );
        assert_eq!(e.document().as_json(), &before);
    }
}
