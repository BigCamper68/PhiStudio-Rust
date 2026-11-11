//! Serialized private-library writes; loaded geometry never enters preferences.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) mod probe;
mod storage;
mod ui;
pub(super) use ui::draw;
#[cfg(test)]
mod tests;
use super::*;
use crate::media::MediaFile;
use phistudio_core::{
    ProjectExport,
    custom_easing::{CurveLibrary, MAX_LIBRARY_BYTES},
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{SyncSender, TrySendError},
    time::Duration,
};

pub(super) enum Action {
    Reload,
    Save {
        name: String,
        curve: Curve,
        replace: Option<String>,
    },
    Rename {
        id: String,
        name: String,
    },
    Remove(String),
    Import(MediaFile),
    Export(Option<String>),
}
enum Job {
    Action { serial: u64, action: Action },
    Flush(mpsc::Sender<()>),
}
enum Selection {
    Keep,
    Select(String),
    Clear,
}
struct Update {
    prepared: Arc<Prepared>,
    selected: Selection,
    export: Option<ProjectExport>,
    message: &'static str,
}
struct Reply {
    serial: u64,
    ready: bool,
    result: Result<Update, String>,
}
pub(super) struct Prepared {
    pub library: CurveLibrary,
    previews: Vec<Thumbnail>,
}
struct Thumbnail {
    points: Vec<[f32; 2]>,
    range: [f32; 2],
}
pub(super) enum Confirm {
    Replace {
        id: String,
        name: String,
        curve: Curve,
    },
    Remove {
        id: String,
        name: String,
    },
}
#[derive(Default)]
pub(crate) struct State {
    root: Option<PathBuf>,
    sender: Option<SyncSender<Job>>,
    receiver: Option<Receiver<Reply>>,
    requested: bool,
    pending: Option<u64>,
    serial: u64,
    ready: bool,
    prepared: Option<Arc<Prepared>>,
    selected: Option<String>,
    name: String,
    filter: String,
    pub(super) confirm: Option<Confirm>,
    error: Option<String>,
    exported: Option<ProjectExport>,
    message: Option<&'static str>,
}
impl State {
    pub(crate) fn clear_confirmation(&mut self) {
        self.confirm = None;
    }
    pub(super) fn open(&mut self, name: &str) {
        self.requested = true;
        if self.name.trim().is_empty() {
            self.name = name.to_owned();
        }
    }
    pub(super) fn request_load(&mut self) {
        self.requested = true;
    }
    pub(super) fn selected_curve(&self) -> Option<Curve> {
        self.prepared
            .as_ref()?
            .library
            .templates
            .iter()
            .find(|t| Some(&t.id) == self.selected.as_ref())
            .map(|t| t.curve.clone())
    }
    pub(super) fn can_use(&self) -> bool {
        self.ready
            && self.pending.is_none()
            && self.confirm.is_none()
            && self.prepared.as_ref().is_some_and(|p| {
                p.library
                    .templates
                    .iter()
                    .any(|t| Some(&t.id) == self.selected.as_ref())
            })
    }
    fn submit(&mut self, action: Action) -> Result<(), String> {
        if self.pending.is_some() {
            return Err("Curve template operation is still running".into());
        }
        let sender = self
            .sender
            .as_ref()
            .ok_or_else(|| "Load the curve template library first".to_owned())?;
        self.serial = self.serial.wrapping_add(1);
        sender
            .try_send(Job::Action {
                serial: self.serial,
                action,
            })
            .map_err(|e| match e {
                TrySendError::Full(_) => "Curve template queue is full".to_owned(),
                TrySendError::Disconnected(_) => {
                    "Curve template worker interrupted; reload the library".to_owned()
                }
            })?;
        self.pending = Some(self.serial);
        self.error = None;
        Ok(())
    }
    pub(crate) fn import(&mut self, file: MediaFile) -> Result<(), String> {
        self.submit(Action::Import(file))
    }
    fn send(&mut self, action: Action) {
        if let Err(error) = self.submit(action) {
            self.error = Some(error);
        }
    }
    pub(crate) fn update(&mut self, root: &Path) {
        let root = root.join("custom-easing");
        if self.root.as_ref() != Some(&root) {
            self.sender = None;
            self.receiver = None;
            self.prepared = None;
            self.exported = None;
            self.message = None;
            self.pending = None;
            self.confirm = None;
            self.selected = None;
            self.ready = false;
            self.root = Some(root.clone());
        }
        if self.requested && self.sender.is_none() {
            let (sender, input) = mpsc::sync_channel(4);
            let (output, receiver) = mpsc::channel();
            self.sender = Some(sender);
            self.receiver = Some(receiver);
            std::thread::spawn(move || storage::worker(root, input, output));
            self.send(Action::Reload);
        }
        let Some(receiver) = &self.receiver else {
            return;
        };
        let reply = match receiver.try_recv() {
            Ok(reply) => reply,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.sender = None;
                self.receiver = None;
                self.pending = None;
                self.ready = false;
                self.error = Some("Curve template worker interrupted; reload the library".into());
                return;
            }
        };
        if self.pending != Some(reply.serial) {
            return;
        }
        self.pending = None;
        self.ready = reply.ready;
        match reply.result {
            Ok(update) => {
                self.message = Some(update.message);
                self.prepared = Some(update.prepared);
                self.error = None;
                match update.selected {
                    Selection::Keep => {}
                    Selection::Select(id) => {
                        self.name = self
                            .prepared
                            .as_ref()
                            .and_then(|p| p.library.templates.iter().find(|t| t.id == id))
                            .map(|t| t.name.clone())
                            .unwrap_or_default();
                        self.selected = Some(id);
                    }
                    Selection::Clear => {
                        self.selected = None;
                    }
                }
                if let Some(export) = update.export {
                    self.exported = Some(export);
                }
            }
            Err(error) => {
                self.message = Some("Curve template operation failed");
                self.error = Some(error);
            }
        }
    }
    pub(crate) fn flush(&self) -> Result<(), String> {
        let Some(sender) = &self.sender else {
            return Ok(());
        };
        let (acknowledge, receiver) = mpsc::channel();
        sender
            .try_send(Job::Flush(acknowledge))
            .map_err(|_| "Could not flush curve template operations".to_owned())?;
        receiver
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| "Curve template write interrupted".to_owned())
    }
}
impl Workbench {
    pub(crate) fn poll_curve_templates(&mut self, root: &Path) {
        self.curve_templates.update(root);
        if let Some(message) = self.curve_templates.message.take() {
            self.status = message.into();
        }
        if self.files.modal_open() || self.files.request.is_some() {
            return;
        }
        if let Some(package) = self.curve_templates.exported.take() {
            #[cfg(any(target_os = "android", target_os = "ios"))]
            {
                self.files.request = Some(crate::files::FileRequest::ReadyFileExport(package));
            }
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                self.files.export_path = package.name.clone();
                self.files.export_package = Some(package);
            }
        }
    }
}

impl Prepared {
    fn new(library: CurveLibrary) -> Result<Self, String> {
        let mut previews = Vec::with_capacity(library.templates.len());
        for template in &library.templates {
            let mut points = Vec::new();
            for (index, segment) in template
                .curve
                .segments()
                .map_err(|e| e.to_string())?
                .into_iter()
                .enumerate()
            {
                if segment.hold {
                    points.extend([
                        segment.points[0],
                        segment.points[3],
                        template.curve.nodes[index + 1].point(),
                    ]);
                } else {
                    let mut parameters = vec![0., 1.];
                    parameters.extend(segment.extrema());
                    for n in 1..64 {
                        let x = f64::from(n) / 64.;
                        if x > segment.points[0][0] && x < segment.points[3][0] {
                            parameters.push(segment.parameter_at(x));
                        }
                    }
                    parameters.sort_by(f64::total_cmp);
                    for parameter in parameters {
                        points.push(segment.point(parameter));
                    }
                }
            }
            let lo = points.iter().map(|p| p[1]).fold(0., f64::min);
            let hi = points.iter().map(|p| p[1]).fold(1., f64::max);
            previews.push(Thumbnail {
                points: points
                    .into_iter()
                    .map(|p| [p[0] as f32, p[1] as f32])
                    .collect(),
                range: [lo as f32, hi as f32],
            });
        }
        Ok(Self { library, previews })
    }
}
