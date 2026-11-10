//! Lazy, single-note distance editing. Only the latest worker result may commit.
mod graph;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) mod probe;
mod ui;
use crate::workbench::Workbench;
use phistudio_core::{
    Beat, NoteId,
    speed_curve::{SpeedCurveEdit, SpeedCurvePoint, SpeedCurveSource},
};
use phistudio_prpr::SpeedCurveProfile;
use std::sync::{
    Arc,
    mpsc::{self, Receiver},
};
#[derive(Clone, Copy, PartialEq, Eq)]
struct Target {
    session: u64,
    revision: u64,
    id: NoteId,
    modern: bool,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Edit,
    Add,
    Delete,
}
enum Task {
    Load,
    Fit {
        profile: Arc<SpeedCurveProfile>,
        points: Vec<SpeedCurvePoint>,
        commit: bool,
    },
}
struct Job {
    serial: u64,
    source: Arc<SpeedCurveSource>,
    layer: usize,
    range: [Beat; 2],
    modern: bool,
    task: Task,
}
enum Done {
    Loaded(SpeedCurveProfile),
    Fitted {
        edit: Box<SpeedCurveEdit>,
        profile: SpeedCurveProfile,
        commit: bool,
    },
}
struct Completed {
    serial: u64,
    result: Result<Done, String>,
}
pub(crate) struct State {
    pub open: bool,
    target: Option<Target>,
    native_clock: Option<(u64, u64, f64)>,
    clock_required: bool,
    clock_waiting: bool,
    source: Option<Arc<SpeedCurveSource>>,
    profile: Option<Arc<SpeedCurveProfile>>,
    preview: Option<SpeedCurveProfile>,
    points: Vec<SpeedCurvePoint>,
    selected: Option<usize>,
    layer: usize,
    range: [Beat; 2],
    range_text: [String; 2],
    mode: Mode,
    start: f64,
    span: f64,
    limit: f64,
    snap: bool,
    snap_step: f64,
    worker: Option<Receiver<Completed>>,
    queued: Option<Job>,
    serial: u64,
    drag: Option<Vec<SpeedCurvePoint>>,
    commit_pending: bool,
    error: Option<String>,
    fit_view: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            target: None,
            native_clock: None,
            clock_required: false,
            clock_waiting: false,
            source: None,
            profile: None,
            preview: None,
            points: Vec::new(),
            selected: None,
            layer: 0,
            range: [Beat::ZERO, Beat::new(8, 0, 1).unwrap()],
            range_text: ["0".into(), "8".into()],
            mode: Mode::Edit,
            start: 0.,
            span: 8.,
            limit: 1600.,
            snap: true,
            snap_step: 50.,
            worker: None,
            queued: None,
            serial: 0,
            drag: None,
            commit_pending: false,
            error: None,
            fit_view: true,
        }
    }
}
impl State {
    pub(crate) fn clear_native_clock(&mut self) {
        self.clock_required = true;
        self.native_clock = None;
    }
    pub(crate) fn record_native_clock(&mut self, session: u64, revision: u64, horizon: f64) {
        self.clock_required = true;
        self.native_clock = Some((session, revision, horizon));
    }
    fn queue(&mut self, task: Task) {
        let (Some(source), Some(target)) = (&self.source, self.target) else {
            return;
        };
        self.serial = self.serial.wrapping_add(1);
        self.queued = Some(Job {
            serial: self.serial,
            source: source.clone(),
            layer: self.layer,
            range: self.range,
            modern: target.modern,
            task,
        });
        self.start_worker();
    }
    fn start_worker(&mut self) {
        if self.worker.is_some() {
            return;
        }
        let Some(job) = self.queued.take() else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        self.worker = Some(receiver);
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                pollster::block_on(async {
                    match job.task {
                        Task::Load => phistudio_prpr::speed_curve_profile(
                            &job.source,
                            job.layer,
                            job.range,
                            job.modern,
                        )
                        .await
                        .map(Done::Loaded),
                        Task::Fit {
                            profile,
                            points,
                            commit,
                        } => {
                            let edit = phistudio_prpr::fit_speed_curve(
                                &job.source,
                                job.layer,
                                &profile,
                                points,
                                job.modern,
                            )
                            .await?;
                            let mut candidate = (*job.source).clone();
                            candidate.layers[job.layer] = edit.events.clone();
                            if !candidate.recipes.is_object() {
                                candidate.recipes = serde_json::json!({});
                            }
                            candidate.recipes[job.layer.to_string()] =
                                serde_json::to_value(&edit.recipe)?;
                            let profile = phistudio_prpr::speed_curve_profile(
                                &candidate, job.layer, job.range, job.modern,
                            )
                            .await?;
                            Ok(Done::Fitted {
                                edit: Box::new(edit),
                                profile,
                                commit,
                            })
                        }
                    }
                })
            }))
            .map_err(|_| "The native speed curve could not be evaluated".into())
            .and_then(|r| r.map_err(|e| format!("{e:#}")));
            let _ = sender.send(Completed {
                serial: job.serial,
                result,
            });
        });
    }
    fn poll(&mut self) -> Option<SpeedCurveEdit> {
        let mut apply = None;
        if let Some(receiver) = &self.worker {
            let ready = match receiver.try_recv() {
                Ok(done) => Some(done),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Completed {
                    serial: self.serial,
                    result: Err("Speed curve worker interrupted".into()),
                }),
            };
            if let Some(done) = ready {
                self.worker = None;
                if done.serial == self.serial {
                    match done.result {
                        Ok(Done::Loaded(profile)) => {
                            self.points = profile.points.clone();
                            self.selected = self.selected.filter(|i| *i < self.points.len());
                            if self.fit_view {
                                self.start = profile.range[0].as_f64();
                                self.span = profile.range[1].as_f64() - self.start;
                                self.limit = profile
                                    .samples
                                    .iter()
                                    .map(|p| p[1].abs())
                                    .fold(100., f64::max)
                                    * 1.15;
                                self.fit_view = false;
                            }
                            self.profile = Some(Arc::new(profile));
                            self.preview = None;
                            self.error = None;
                        }
                        Ok(Done::Fitted {
                            edit,
                            profile,
                            commit,
                        }) => {
                            self.preview = Some(profile);
                            self.error = None;
                            if commit {
                                self.commit_pending = false;
                                apply = Some(*edit);
                            }
                        }
                        Err(error) => {
                            self.error = Some(error);
                            self.commit_pending = false;
                            if self.drag.is_none() {
                                if let Some(profile) = &self.profile {
                                    self.points = profile.points.clone();
                                }
                                self.preview = None;
                            }
                        }
                    }
                }
            }
        }
        self.start_worker();
        apply
    }
    fn request_fit(&mut self, commit: bool) {
        let Some(profile) = self.profile.clone() else {
            return;
        };
        self.commit_pending = commit;
        self.queue(Task::Fit {
            profile,
            points: self.points.clone(),
            commit,
        });
    }
    pub(crate) fn cancel(&mut self) {
        if self.drag.is_none() && !self.commit_pending && self.preview.is_none() {
            return;
        }
        self.serial = self.serial.wrapping_add(1);
        self.queued = None;
        self.drag = None;
        self.preview = None;
        self.commit_pending = false;
        if let Some(profile) = &self.profile {
            self.points = profile.points.clone();
        }
    }
    pub(crate) fn captures_input(&self) -> bool {
        self.open && (self.drag.is_some() || self.commit_pending)
    }
    pub(crate) fn close(&mut self) {
        self.cancel();
        self.open = false;
        self.serial = self.serial.wrapping_add(1);
        self.queued = None;
        self.target = None;
        self.source = None;
        self.profile = None;
        self.points.clear();
    }
    fn load(&mut self) {
        self.cancel();
        self.profile = None;
        self.preview = None;
        self.error = None;
        self.queue(Task::Load);
    }
    fn busy(&self) -> bool {
        self.profile.is_none() || self.commit_pending
    }
}
impl Workbench {
    fn speed_curve_note(&self) -> Option<NoteId> {
        let selection = self.selection();
        (selection.notes.len() == 1
            && selection.events.is_empty()
            && self.advanced.story_selection.is_empty())
        .then(|| selection.notes[0])
    }
    pub(crate) fn open_speed_curve(&mut self) {
        if self.speed_curve_note().is_some() {
            self.transport.playing = false;
            self.timeline.cancel_gesture();
            if !self.speed_curve.open {
                self.speed_curve.layer = self.events.layer.min(3);
            }
            self.speed_curve.open = true;
            self.speed_curve.target = None;
            self.speed_curve.fit_view = true;
        }
    }
}
#[cfg(test)]
mod tests;
