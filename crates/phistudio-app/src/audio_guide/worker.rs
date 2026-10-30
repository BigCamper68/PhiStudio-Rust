use phistudio_audio::{Analysis, Control, Error, Onset, Selection, StereoSource, Waveform, cache};
use phistudio_prpr::ResourceAudioClip;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
};
#[derive(Clone)]
pub(super) struct Source(pub ResourceAudioClip);
impl StereoSource for Source {
    fn sample_rate(&self) -> u32 {
        self.0.sample_rate()
    }
    fn frame_count(&self) -> usize {
        self.0.frame_count()
    }
    fn frame(&self, i: usize) -> [f32; 2] {
        let f = self.0.frames()[i];
        [f.0, f.1]
    }
}
pub(super) enum Command {
    Analyze {
        epoch: u64,
        source: Source,
        control: Control,
        selection: Selection,
        cache: Option<PathBuf>,
        want_attacks: bool,
    },
    Select {
        epoch: u64,
        tuning: u64,
        data: Arc<Analysis>,
        selection: Selection,
        control: Control,
    },
}
pub(super) enum Message {
    Wave {
        epoch: u64,
        waveform: Arc<Waveform>,
    },
    Done {
        epoch: u64,
        data: Arc<Analysis>,
        markers: Vec<Onset>,
        selection: Selection,
        cached: bool,
        full_data: bool,
    },
    Selected {
        epoch: u64,
        tuning: u64,
        markers: Vec<Onset>,
    },
    Error {
        epoch: u64,
        error: String,
    },
}
pub(super) struct Worker {
    pub(super) sender: Sender<Command>,
    pub(super) receiver: Receiver<Message>,
}
impl Worker {
    pub fn new() -> Result<Self, String> {
        let (sender, commands) = mpsc::channel();
        let (events, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("audio-guide".into())
            .spawn(move || {
                while let Ok(mut command) = commands.recv() {
                    // Only one analyzer thread exists; rapid project/option changes
                    // replace queued work instead of accumulating CPU-heavy tasks.
                    while let Ok(newer) = commands.try_recv() {
                        command = newer;
                    }
                    let (epoch, result) = match command {
                        Command::Analyze {
                            epoch,
                            source,
                            control,
                            selection,
                            cache,
                            want_attacks,
                        } => {
                            let result = (|| {
                                control.check()?;
                                let key = if cache.is_some() {
                                    Some(cache::fingerprint(&source, &control)?)
                                } else {
                                    None
                                };
                                let stored = cache
                                    .as_ref()
                                    .zip(key.as_ref())
                                    .and_then(|(root, key)| cache::load(root, key, &control));
                                let cached = stored.is_some();
                                let full_data = cached || want_attacks;
                                let data = if let Some(data) = stored {
                                    data
                                } else if !want_attacks {
                                    Analysis {
                                        waveform: phistudio_audio::waveform_only(
                                            &source, &control,
                                        )?,
                                        candidates: vec![],
                                    }
                                } else {
                                    let data =
                                        phistudio_audio::analyze(&source, &control, |waveform| {
                                            let _ = events.send(Message::Wave { epoch, waveform });
                                        })?;
                                    if let Some((root, key)) = cache.as_ref().zip(key.as_ref()) {
                                        let _ = cache::store(root, key, &data, &control);
                                    }
                                    data
                                };
                                let markers = data.select(selection, &control)?;
                                Ok(Message::Done {
                                    epoch,
                                    data: Arc::new(data),
                                    markers,
                                    selection,
                                    cached,
                                    full_data,
                                })
                            })();
                            (epoch, result)
                        }
                        Command::Select {
                            epoch,
                            tuning,
                            data,
                            selection,
                            control,
                        } => (
                            epoch,
                            data.select(selection, &control)
                                .map(|markers| Message::Selected {
                                    epoch,
                                    tuning,
                                    markers,
                                }),
                        ),
                    };
                    let message = match result {
                        Ok(m) => m,
                        Err(Error::Canceled) => continue,
                        Err(e) => Message::Error {
                            epoch,
                            error: e.to_string(),
                        },
                    };
                    if events.send(message).is_err() {
                        break;
                    }
                }
            })
            .map_err(|_| "Audio analysis worker stopped".to_owned())?;
        Ok(Self { sender, receiver })
    }
    pub fn send(&self, command: Command) -> Result<(), mpsc::SendError<Command>> {
        self.sender.send(command)
    }
    pub fn receive(&self) -> Option<Message> {
        self.receiver.try_recv().ok()
    }
}
