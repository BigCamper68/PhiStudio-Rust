use super::*;
use phistudio_core::{NoteSoundGain, write_generated_hitsound};

pub(crate) fn prepare(request: Request) -> (Target, Result<Prepared, String>) {
    let target = request.target;
    let result = (|| {
        let sounds = match request.cached {
            Some(sounds) => sounds,
            None => {
                let root = target
                    .key
                    .root
                    .as_deref()
                    .ok_or("Choose a project for default sounds")?;
                let mut sounds = NoteCreationSounds {
                    reference: target.key.reference.clone(),
                    ..Default::default()
                };
                for kind in [NoteKind::Tap, NoteKind::Flick, NoteKind::Drag] {
                    if request.cancel.load(Ordering::Relaxed) {
                        return Err("Note creation canceled".into());
                    }
                    let index = NoteCreationSounds::index(kind);
                    if index > 0 && sounds.reference.is_some() {
                        sounds.gains[index] = sounds.gains[0].clone();
                        continue;
                    }
                    if target.key.volume < 1. {
                        let wav = pollster::block_on(phistudio_prpr::render_hitsound_gain(
                            root,
                            target.key.skin.as_deref(),
                            sounds.reference.as_deref(),
                            kind,
                            target.key.volume,
                        ))
                        .map_err(|e| format!("{e:#}"))?;
                        let rendered =
                            write_generated_hitsound(root, &wav).map_err(|e| e.to_string())?;
                        sounds.gains[index] = Some(NoteSoundGain {
                            source: sounds.reference.clone(),
                            rendered,
                            volume: target.key.volume,
                        });
                    } else if let Some(reference) = &sounds.reference {
                        let path = phistudio_core::hitsound_path(root, reference)
                            .map_err(|e| e.to_string())?;
                        crate::media::MediaFile::new(path, reference.clone(), false)
                            .validate(crate::media::MediaKind::HitSound)?;
                    }
                }
                sounds
            }
        };
        if request.cancel.load(Ordering::Relaxed) {
            return Err("Note creation canceled".into());
        }
        let action = request.action.prepare(&target.defaults, &sounds, || {
            request.cancel.load(Ordering::Relaxed)
        })?;
        Ok(Prepared { action, sounds })
    })();
    (target, result)
}
