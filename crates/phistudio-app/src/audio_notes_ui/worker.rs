use super::*;
use std::sync::mpsc::{self, Receiver, SyncSender};
pub(super) struct Request {
    pub sounds: crate::note_creation::SoundKey,
    pub defaults: phistudio_core::NoteCreationDefaults,
    pub id: u64,
    pub document: phistudio_core::ChartDocument,
    pub markers: Vec<AudioMarker>,
    pub spec: AudioNoteSpec,
    pub cancel: Arc<AtomicBool>,
}
pub(super) struct Reply {
    pub id: u64,
    pub result: Result<Prepared, String>,
}
pub(super) struct Planner {
    commands: SyncSender<Request>,
    pub replies: Receiver<Reply>,
}
impl Planner {
    pub fn new() -> Result<Self, String> {
        let (commands, requests) = mpsc::sync_channel::<Request>(1);
        let (events, replies) = mpsc::channel();
        std::thread::Builder::new()
            .name("audio-note-planner".into())
            .spawn(move || {
                let mut audition_cache: Option<(
                    crate::note_creation::SoundKey,
                    phistudio_prpr::PreparedNoteAudition,
                )> = None;
                while let Ok(request) = requests.recv() {
                    let result = (|| {
                        let mut plan = preview_audio_notes(
                            &request.document,
                            &request.markers,
                            &request.spec,
                            || request.cancel.load(Ordering::Acquire),
                        )
                        .map_err(|e| e.to_string())?;
                        for item in &mut plan.notes {
                            request.defaults.apply_style(
                                &mut item.note,
                                if request.spec.options.use_creation_defaults {
                                    phistudio_core::NoteCreationPolicy::Regular
                                } else {
                                    phistudio_core::NoteCreationPolicy::Audio
                                },
                            );
                        }
                        let mut groups =
                            BTreeMap::<usize, Vec<(usize, phistudio_core::Note)>>::new();
                        for item in &plan.notes {
                            let notes = groups.entry(item.line).or_default();
                            notes.push((notes.len(), item.note.clone()));
                        }
                        let ghosts = groups
                            .into_iter()
                            .map(|(line, notes)| {
                                let index = crate::note_index::NoteIndex::new(&notes);
                                (line, Ghosts { notes, index })
                            })
                            .collect();
                        let audio = audio::Audio::new(&plan, request.document.tempo());
                        let audition = if let Some((_, audition)) = audition_cache
                            .as_ref()
                            .filter(|(key, _)| key == &request.sounds)
                        {
                            audition.clone()
                        } else {
                            let audition =
                                pollster::block_on(phistudio_prpr::prepare_note_audition(
                                    request.sounds.root.as_deref(),
                                    request.sounds.skin.as_deref(),
                                    request.sounds.reference.as_deref(),
                                    request.sounds.volume,
                                ))
                                .map_err(|e| format!("{e:#}"))?;
                            audition_cache = Some((request.sounds.clone(), audition.clone()));
                            audition
                        };
                        Ok(Prepared {
                            audition,
                            audition_id: request.id,
                            plan,
                            ghosts,
                            audio,
                        })
                    })();
                    if events
                        .send(Reply {
                            id: request.id,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| format!("Could not start note generator: {e}"))?;
        Ok(Self { commands, replies })
    }
    pub fn send(&self, request: Request) -> Result<(), String> {
        self.commands
            .try_send(request)
            .map_err(|_| "The previous note preview is stopping. Try again.".into())
    }
}
