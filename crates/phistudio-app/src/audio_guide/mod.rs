//! Background analysis and passive grid guides; no document/history commands.
mod controls;
mod paint;
mod plugin;
mod worker;
use phistudio_audio::{Analysis, Control, Onset, Selection, Waveform};
use phistudio_prpr::ResourceAudioClip;
use std::{path::PathBuf, sync::Arc};
use worker::{Command, Message, Source, Worker};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Key {
    session: u64,
    assets: u64,
    pcm: usize,
    frames: usize,
    sr: u32,
}
#[derive(Default)]
enum Status {
    #[default]
    NoMusic,
    Working,
    Ready,
    Canceled,
    Failed(String),
}
#[derive(Default)]
pub(crate) struct Guide {
    pub(crate) show: bool,
    pub(crate) generation_requested: bool,
    pub(crate) plugin_requested: bool,
    source_serial: u64,
    worker: Option<Worker>,
    key: Option<Key>,
    music: Option<String>,
    source: Option<Source>,
    epoch: u64,
    tuning: u64,
    control: Control,
    working: bool,
    manual_cancel: bool,
    waveform: Option<Arc<Waveform>>,
    result: Option<Arc<Analysis>>,
    markers: Vec<Onset>,
    selection: Option<Selection>,
    requested_selection: Option<Selection>,
    status: Status,
    cached: bool,
    full_data: bool,
    want_attacks: bool,
    cache_root: Option<PathBuf>,
}
impl Guide {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub(crate) fn probe_report(&self) -> serde_json::Value {
        serde_json::json!({"ready":matches!(self.status,Status::Ready)&&self.full_data,"working":self.working,
            "cached":self.cached,"epoch":self.epoch,"markers":self.markers.iter().map(|m|
                serde_json::json!({"seconds":m.seconds,"strength":m.strength,"band":m.band})).collect::<Vec<_>>(),
            "waveform_bytes":self.waveform.as_ref().map(|w|w.memory_bytes())})
    }
    pub fn clear_project(&mut self) {
        self.show = false;
        self.generation_requested = false;
        self.plugin_requested = false;
        self.source_serial = self.source_serial.wrapping_add(1);
        self.control.cancel();
        self.epoch = self.epoch.wrapping_add(1);
        self.key = None;
        self.music = None;
        self.source = None;
        self.waveform = None;
        self.result = None;
        self.markers.clear();
        self.working = false;
        self.manual_cancel = false;
        self.full_data = false;
        self.cached = false;
        self.requested_selection = None;
        self.status = Status::NoMusic;
    }
    pub fn set_cache_root(&mut self, root: Option<PathBuf>) {
        self.cache_root = root;
    }
    pub fn current(&self, session: u64, assets: u64, music: Option<&str>) -> bool {
        self.key
            .is_some_and(|k| k.session == session && k.assets == assets)
            && self.music.as_deref() == music
    }
    fn send(&mut self, command: Command) {
        if self.worker.is_none() {
            match Worker::new() {
                Ok(worker) => self.worker = Some(worker),
                Err(error) => {
                    self.working = false;
                    self.manual_cancel = true;
                    self.status = Status::Failed(error);
                    return;
                }
            }
        }
        let worker = self.worker.as_ref().unwrap();
        if worker.send(command).is_err() {
            self.working = false;
            self.manual_cancel = true;
            self.worker = None;
            self.status = Status::Failed("Audio analysis worker stopped".into());
        }
    }
    fn start(&mut self) {
        let Some(source) = self.source.clone() else {
            return;
        };
        self.control.cancel();
        self.control = Control::default();
        self.epoch = self.epoch.wrapping_add(1);
        self.working = true;
        self.manual_cancel = false;
        self.status = Status::Working;
        self.send(Command::Analyze {
            epoch: self.epoch,
            source,
            control: self.control.clone(),
            selection: self.selection.unwrap_or_default(),
            cache: self.cache_root.clone(),
            want_attacks: self.want_attacks,
        });
    }
    fn cancel(&mut self) {
        self.control.cancel();
        self.epoch = self.epoch.wrapping_add(1);
        self.working = false;
        self.manual_cancel = true;
        self.status = Status::Canceled;
        self.requested_selection = None;
    }
    pub fn update(
        &mut self,
        session: u64,
        assets: u64,
        music: Option<&str>,
        clip: Option<ResourceAudioClip>,
        options: &phistudio_core::AudioGuideOptions,
    ) {
        let key = clip.as_ref().map(|c| Key {
            session,
            assets,
            pcm: c.frames().as_ptr() as usize,
            frames: c.frame_count(),
            sr: c.sample_rate(),
        });
        if key != self.key || self.music.as_deref() != music {
            self.source_serial = self.source_serial.wrapping_add(1);
            self.control.cancel();
            self.epoch = self.epoch.wrapping_add(1);
            self.key = key;
            self.music = music.map(str::to_owned);
            self.source = clip.map(Source);
            self.waveform = None;
            self.result = None;
            self.markers.clear();
            self.working = false;
            self.manual_cancel = false;
            self.cached = false;
            self.full_data = false;
            self.requested_selection = None;
            self.status = Status::NoMusic;
        }
        let selection = Selection {
            sensitivity: options.sensitivity,
            min_gap: f64::from(options.min_gap_ms) / 1000.,
            band: options.band,
        };
        self.selection = Some(selection);
        self.want_attacks = options.attacks || self.generation_requested || self.plugin_requested;
        loop {
            let message = self.worker.as_ref().and_then(|w| w.receive());
            let Some(message) = message else {
                break;
            };
            match message {
                Message::Wave { epoch, waveform } if epoch == self.epoch => {
                    self.waveform = Some(waveform)
                }
                Message::Done {
                    epoch,
                    data,
                    markers,
                    selection,
                    cached,
                    full_data,
                } if epoch == self.epoch => {
                    self.waveform = Some(Arc::clone(&data.waveform));
                    self.result = Some(data);
                    self.cached = cached;
                    self.full_data = full_data;
                    self.requested_selection = Some(selection);
                    if self.selection == Some(selection) {
                        self.markers = markers;
                        self.status = Status::Ready;
                    }
                    self.working = false;
                }
                Message::Selected {
                    epoch,
                    tuning,
                    markers,
                } if epoch == self.epoch && tuning == self.tuning => {
                    self.markers = markers;
                    self.working = false;
                    self.status = Status::Ready;
                }
                Message::Error { epoch, error } if epoch == self.epoch => {
                    self.working = false;
                    self.manual_cancel = true;
                    self.status = Status::Failed(error);
                }
                _ => {}
            }
        }
        if !options.waveform && !self.want_attacks {
            if self.working {
                self.cancel();
                self.manual_cancel = false;
            }
            return;
        }
        if !self.want_attacks
            && self.working
            && self.result.is_none()
            && let Some(waveform) = self.waveform.clone()
        {
            self.control.cancel();
            self.epoch = self.epoch.wrapping_add(1);
            self.working = false;
            self.result = Some(Arc::new(Analysis {
                waveform,
                candidates: vec![],
            }));
            self.full_data = false;
            self.status = Status::Ready;
            self.requested_selection = Some(selection);
        }
        if self.want_attacks
            && !self.full_data
            && self.source.is_some()
            && self.result.is_some()
            && !self.working
            && !self.manual_cancel
        {
            self.start();
            return;
        }
        if !self.manual_cancel
            && self.result.is_some()
            && self.requested_selection != Some(selection)
        {
            self.control.cancel();
            self.control = Control::default();
            self.tuning = self.tuning.wrapping_add(1);
            self.markers.clear();
            self.working = true;
            self.status = Status::Working;
            self.requested_selection = Some(selection);
            self.send(Command::Select {
                epoch: self.epoch,
                tuning: self.tuning,
                data: Arc::clone(self.result.as_ref().unwrap()),
                selection,
                control: self.control.clone(),
            });
        } else if self.source.is_some()
            && self.result.is_none()
            && !self.working
            && !self.manual_cancel
        {
            self.start();
        }
    }
}
impl Drop for Guide {
    fn drop(&mut self) {
        self.control.cancel();
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
impl Guide {
    pub(crate) fn seed_generation_test(
        &mut self,
        session: u64,
        assets: u64,
        music: Option<&str>,
        options: &phistudio_core::AudioGuideOptions,
    ) -> ResourceAudioClip {
        let epoch = self.epoch.wrapping_add(1);
        *self = Self::default();
        self.epoch = epoch;
        let sr = 8000u32;
        let bytes = sr * 4 * 4;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(bytes + 36).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&sr.to_le_bytes());
        wav.extend_from_slice(&(sr * 4).to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&bytes.to_le_bytes());
        wav.resize(wav.len() + bytes as usize, 0);
        let clip = ResourceAudioClip::new(wav).unwrap();
        self.key = Some(Key {
            session,
            assets,
            pcm: clip.frames().as_ptr() as usize,
            frames: clip.frame_count(),
            sr,
        });
        let source = Source(clip.clone());
        let waveform = phistudio_audio::waveform_only(&source, &Control::default()).unwrap();
        self.waveform = Some(Arc::clone(&waveform));
        self.result = Some(Arc::new(Analysis {
            waveform,
            candidates: vec![],
        }));
        self.source = Some(source);
        self.music = music.map(str::to_owned);
        self.status = Status::Ready;
        self.full_data = true;
        let selection = Selection {
            sensitivity: options.sensitivity,
            min_gap: f64::from(options.min_gap_ms) / 1000.,
            band: options.band,
        };
        self.selection = Some(selection);
        self.requested_selection = Some(selection);
        self.markers = vec![
            Onset {
                seconds: 0.25,
                strength: 0.9,
                band: 1,
            },
            Onset {
                seconds: 0.75,
                strength: 0.85,
                band: 2,
            },
            Onset {
                seconds: 1.25,
                strength: 0.8,
                band: 3,
            },
        ];
        clip
    }
}
