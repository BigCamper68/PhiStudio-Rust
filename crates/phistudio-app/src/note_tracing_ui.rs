//! Motion Generator's native note tracing workflow.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) mod probe;
mod roots;
mod ui;
mod worker;
use crate::workbench::Workbench;
use phistudio_core::{
    Beat, NoteId,
    note_tracing::{self, Snapshot, Spec, TracePoint, TraceTime},
};
use phistudio_prpr::{NoteTraceDestination, NoteTraceSample, Preview};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
    },
    time::Instant,
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum MotionMode {
    #[default]
    Trajectory,
    NoteTracing,
}
#[derive(Clone, Debug, PartialEq)]
struct Key {
    session: u64,
    revision: u64,
    assets: u64,
    aspect: u32,
    modern: bool,
    uniform_scale: bool,
    line_defaults: Option<phistudio_core::LineCreationDefaults>,
    spec: Spec,
}
struct Sampling {
    snapshot: Snapshot,
    times: Vec<TraceTime>,
    samples: Vec<NoteTraceSample>,
    control_heights: Vec<f64>,
}
struct Prepared {
    edit: Result<note_tracing::Edit, String>,
    path: Vec<[f64; 2]>,
    points: Vec<[f64; 2]>,
    samples: usize,
}
enum Completed {
    Schedule {
        snapshot: Snapshot,
        times: Vec<TraceTime>,
        control_heights: Vec<f64>,
    },
    Prepared(Prepared),
    Refine(roots::Refinement),
}
struct Pending {
    key: Key,
    cancel: Arc<AtomicBool>,
    stage: Stage,
}
enum Stage {
    Requested(Snapshot),
    Worker(Receiver<Result<Completed, String>>),
    Sampling(Sampling),
    Refining(roots::Refinement),
}
pub(crate) struct State {
    pub mode: MotionMode,
    source: Option<(u64, NoteId)>,
    destination: Option<usize>,
    layer: usize,
    start: String,
    end: String,
    rate: u32,
    rotation: bool,
    replace: bool,
    tolerance: f64,
    angle_tolerance: f64,
    pending: Option<Pending>,
    prepared: Option<(Key, Prepared)>,
    error: Option<String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            mode: Default::default(),
            source: None,
            destination: None,
            layer: 0,
            start: "0".into(),
            end: "4".into(),
            rate: 120,
            rotation: true,
            replace: false,
            tolerance: 0.15,
            angle_tolerance: 0.015,
            pending: None,
            prepared: None,
            error: None,
        }
    }
}
impl State {
    fn cancel(&mut self) {
        if let Some(pending) = self.pending.take() {
            pending.cancel.store(true, Ordering::Relaxed);
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl Workbench {
    fn tracing_key(&self) -> Result<Key, String> {
        let (session, source) = self.note_tracing.source.ok_or("Choose a source note")?;
        if session != self.session {
            return Err("Choose a source note in this project".into());
        }
        if self
            .editor
            .document()
            .note(source.line, source.index)
            .is_err()
        {
            return Err("Source note no longer exists".into());
        }
        let state = &self.note_tracing;
        if state
            .destination
            .is_some_and(|line| line >= self.editor.document().line_count())
        {
            return Err("Tracing destination no longer exists".into());
        }
        let spec = Spec {
            source,
            destination: state.destination,
            layer: state.layer,
            range: [
                state.start.parse::<Beat>().map_err(|e| e.to_string())?,
                state.end.parse::<Beat>().map_err(|e| e.to_string())?,
            ],
            sample_rate: state.rate,
            rotation: state.rotation,
            replace: state.replace,
            position_tolerance: state.tolerance,
            rotation_tolerance: state.angle_tolerance,
        };
        spec.validate().map_err(|e| e.to_string())?;
        Ok(Key {
            session,
            revision: self.editor.revision(),
            assets: self.asset_revision,
            aspect: self.preview_aspect().to_bits(),
            modern: self.project_options.use_rpe_170_speed,
            uniform_scale: self.project_options.note_uniform_scale,
            line_defaults: state
                .destination
                .is_none()
                .then_some(self.options.new_line_defaults),
            spec,
        })
    }
    fn select_trace_source(&mut self) {
        let selected = self.selection();
        if selected.notes.len() != 1 {
            return;
        }
        let id = selected.notes[0];
        if let Ok(note) = self.editor.document().note(id.line, id.index) {
            let hit = note.start_time.as_f64();
            self.note_tracing.cancel();
            self.note_tracing.source = Some((self.session, id));
            self.note_tracing.start = Beat::from_grid((hit - 8.).max(0.), 1920)
                .unwrap_or(Beat::ZERO)
                .to_string();
            self.note_tracing.end = if note.kind == phistudio_core::NoteKind::Hold {
                note.end_time.to_string()
            } else {
                Beat::from_grid(hit.max(1.), 1920)
                    .unwrap_or(Beat::ONE)
                    .to_string()
            };
            self.note_tracing.prepared = None;
            self.note_tracing.error = None;
        }
    }
    fn begin_note_trace(&mut self) {
        let result = (|| {
            let key = self.tracing_key()?;
            let snapshot = Snapshot::capture_with_line_defaults(
                self.editor.document(),
                key.spec.clone(),
                key.line_defaults.unwrap_or_default(),
            )
            .map_err(|e| e.to_string())?;
            self.note_tracing.cancel();
            self.note_tracing.prepared = None;
            self.note_tracing.pending = Some(Pending {
                key,
                cancel: Arc::new(AtomicBool::new(false)),
                stage: Stage::Requested(snapshot),
            });
            Ok(())
        })();
        self.note_tracing.error = result.err();
    }
    pub(crate) fn update_note_tracing(&mut self, preview: &mut Preview, native_current: bool) {
        if self.note_tracing.mode == MotionMode::Trajectory
            && self.note_tracing.pending.is_none()
            && self.note_tracing.prepared.is_none()
        {
            return;
        }
        let key = self.tracing_key().ok();
        let mut state = std::mem::take(&mut self.note_tracing);
        let active = self.tools.show
            && self.tools.page == crate::toolkit::Page::Motion
            && state.mode == MotionMode::NoteTracing
            && !self.show_projects
            && !self.playtest.active();
        if !active
            || state
                .pending
                .as_ref()
                .is_some_and(|p| Some(&p.key) != key.as_ref())
        {
            state.cancel();
        }
        if state.pending.is_some() && !native_current && self.render_error.is_some() {
            state.cancel();
            state.error = Some("The native preview is unavailable for tracing".into());
        }
        if state
            .prepared
            .as_ref()
            .is_some_and(|p| Some(&p.0) != key.as_ref())
        {
            state.prepared = None;
        }
        if let Some(mut pending) = state.pending.take() {
            let destination = |snapshot: &Snapshot| {
                snapshot.spec.destination.map(|line| NoteTraceDestination {
                    line,
                    components: snapshot.native_components,
                })
            };
            let next = match pending.stage {
                Stage::Requested(snapshot) if native_current => {
                    let range = snapshot
                        .spec
                        .range
                        .map(|b| snapshot.tempo.seconds_at(b.as_f64()));
                    match preview.note_trace_boundaries(
                        snapshot.spec.source,
                        destination(&snapshot),
                        range,
                    ) {
                        Ok(boundaries) => Some(Stage::Worker(worker::schedule(
                            snapshot,
                            boundaries,
                            pending.cancel.clone(),
                        ))),
                        Err(error) => {
                            state.error = Some(format!("{error:#}"));
                            None
                        }
                    }
                }
                Stage::Requested(_) if self.render_error.is_some() => {
                    state.error = Some("The native preview is unavailable for tracing".into());
                    None
                }
                Stage::Requested(snapshot) => Some(Stage::Requested(snapshot)),
                Stage::Worker(receiver) => match receiver.try_recv() {
                    Ok(Ok(Completed::Schedule {
                        snapshot,
                        times,
                        control_heights,
                    })) => Some(Stage::Sampling(Sampling {
                        snapshot,
                        times,
                        samples: vec![],
                        control_heights,
                    })),
                    Ok(Ok(Completed::Prepared(prepared))) => {
                        state.prepared = Some((pending.key.clone(), prepared));
                        None
                    }
                    Ok(Ok(Completed::Refine(refinement))) => Some(Stage::Refining(refinement)),
                    Ok(Err(error)) => {
                        state.error = Some(error);
                        None
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => Some(Stage::Worker(receiver)),
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        state.error = Some("Note tracing worker interrupted".into());
                        None
                    }
                },
                Stage::Sampling(mut sample) if native_current => {
                    let deadline = Instant::now() + std::time::Duration::from_millis(3);
                    let mut failure = None;
                    while sample.samples.len() < sample.times.len() && Instant::now() < deadline {
                        let from = sample.samples.len();
                        let mut count = (sample.times.len() - from).min(32);
                        loop {
                            let times: Vec<_> = sample.times[from..from + count]
                                .iter()
                                .map(|t| t.sample_seconds)
                                .collect();
                            match preview.sample_note_trace(
                                sample.snapshot.spec.source,
                                destination(&sample.snapshot),
                                &times,
                            ) {
                                Ok(points) => {
                                    sample.samples.extend(points);
                                    break;
                                }
                                Err(error)
                                    if count > 1 && error.to_string().contains("sample budget") =>
                                {
                                    count /= 2;
                                }
                                Err(error) => {
                                    failure = Some(format!("{error:#}"));
                                    break;
                                }
                            }
                        }
                        if failure.is_some() {
                            break;
                        }
                    }
                    if let Some(error) = failure {
                        state.error = Some(error);
                        None
                    } else if sample.samples.len() == sample.times.len() {
                        Some(Stage::Worker(worker::after_sampling(
                            sample,
                            pending.key.revision,
                            pending.cancel.clone(),
                        )))
                    } else {
                        Some(Stage::Sampling(sample))
                    }
                }
                Stage::Sampling(sample) => Some(Stage::Sampling(sample)),
                Stage::Refining(mut refinement) if native_current => {
                    match refinement.advance(preview) {
                        Ok(true) => Some(Stage::Worker(worker::reschedule(
                            refinement,
                            pending.cancel.clone(),
                        ))),
                        Ok(false) => Some(Stage::Refining(refinement)),
                        Err(error) => {
                            state.error = Some(error);
                            None
                        }
                    }
                }
                Stage::Refining(refinement) => Some(Stage::Refining(refinement)),
            };
            if let Some(stage) = next {
                pending.stage = stage;
                state.pending = Some(pending);
            }
        }
        self.note_tracing = state;
    }
}
#[cfg(test)]
mod tests;
