//! A local curve draft; chart history remains owned by the calling inspector.
mod graph;
mod input;
pub(crate) mod library;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) mod probe;
#[cfg(test)]
mod tests;
mod ui;

use crate::workbench::Workbench;
use phistudio_core::{
    Beat, EventId, LineEvent,
    custom_easing::{Curve, Mode, Snapshot},
};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftKey {
    pub session: u64,
    pub revision: u64,
    pub id: EventId,
    pub event: LineEvent,
    pub times: [String; 2],
}
struct Request {
    key: DraftKey,
    snapshot: Snapshot,
    domain: [f64; 2],
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Tool {
    #[default]
    Edit,
    Add,
    Pan,
}
#[derive(Clone)]
struct Draft {
    curve: Curve,
    selected: BTreeSet<usize>,
    preserve_domain: bool,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            curve: Curve::default(),
            selected: BTreeSet::from([0]),
            preserve_domain: true,
        }
    }
}
struct Pending {
    serial: u64,
    cancel: Arc<AtomicBool>,
    receiver: Receiver<Result<LineEvent, String>>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
#[derive(Default)]
pub(crate) struct State {
    pub open: bool,
    pub input_canceled: bool,
    request: Option<Request>,
    draft: Draft,
    undo: Vec<Draft>,
    redo: Vec<Draft>,
    numeric_before: Option<Draft>,
    tool: Tool,
    multiple: bool,
    details: bool,
    library_view: bool,
    view: graph::Bounds,
    pointer: Option<input::Pointer>,
    navigation: input::Navigation,
    serial: u64,
    pending: Option<Pending>,
    completed: Option<(DraftKey, LineEvent)>,
    error: Option<String>,
    preview: f64,
    animate: bool,
}
impl State {
    pub(crate) fn back(&mut self) {
        if self.library_view {
            self.library_view = false;
        } else {
            self.close();
        }
    }
    pub(crate) fn close(&mut self) {
        self.cancel_gesture();
        self.pending = None;
        self.request = None;
        self.completed = None;
        self.navigation = Default::default();
        self.open = false;
        self.serial = self.serial.wrapping_add(1);
    }
    pub(crate) fn cancel_gesture(&mut self) {
        if let Some(pointer) = self.pointer.take() {
            self.draft = pointer.before;
        }
        if let Some(before) = self.numeric_before.take() {
            self.draft = before;
        }
    }
    pub(crate) fn take_completed(&mut self, key: &DraftKey) -> Option<LineEvent> {
        let (expected, event) = self.completed.take()?;
        (expected == *key).then_some(event)
    }
    fn remember(&mut self, before: Draft) {
        if before.curve == self.draft.curve && before.preserve_domain == self.draft.preserve_domain
        {
            return;
        }
        if self.undo.len() == 128 {
            self.undo.remove(0);
        }
        self.undo.push(before);
        self.redo.clear();
        self.error = None;
    }
    fn edit(&mut self, next: Result<Curve, phistudio_core::ChartError>) {
        match next {
            Ok(curve) => {
                let before = self.draft.clone();
                self.draft.curve = curve;
                self.remember(before);
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    fn history(&mut self, redo: bool) {
        self.cancel_gesture();
        let source = if redo { &mut self.redo } else { &mut self.undo };
        let Some(next) = source.pop() else { return };
        let before = std::mem::replace(&mut self.draft, next);
        if redo {
            self.undo.push(before);
        } else {
            self.redo.push(before);
        }
        self.error = None;
    }
    fn indices(&self) -> Vec<usize> {
        self.draft.selected.iter().copied().collect()
    }
    fn insert(&mut self, x: f64) {
        match self.draft.curve.insert(x) {
            Ok((curve, index)) => {
                let before = self.draft.clone();
                self.draft.curve = curve;
                self.draft.selected = BTreeSet::from([index]);
                self.remember(before);
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    fn delete(&mut self) {
        let before = self.draft.clone();
        let next = self.draft.curve.delete_nodes(&self.indices());
        match next {
            Ok(curve) => {
                self.draft.curve = curve;
                self.draft.selected.clear();
                self.remember(before);
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    fn set_mode(&mut self, mode: Mode) {
        self.edit(self.draft.curve.set_mode(&self.indices(), mode));
    }
    fn load_template(&mut self, curve: Curve) {
        self.finish_numeric();
        let before = self.draft.clone();
        self.view = graph::Bounds::fit(&curve);
        self.draft = Draft {
            curve,
            selected: BTreeSet::from([0]),
            preserve_domain: false,
        };
        self.remember(before);
        self.library_view = false;
    }
    fn prepare(&mut self) {
        self.finish_numeric();
        let Some(request) = &self.request else { return };
        let snapshot = request.snapshot.clone();
        let kind = request.key.id.kind;
        let curve = self.draft.curve.clone();
        let preserve_domain = self.draft.preserve_domain;
        self.serial = self.serial.wrapping_add(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let canceled = cancel.clone();
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(Pending {
            serial: self.serial,
            cancel,
            receiver,
        });
        self.error = None;
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let active = || canceled.load(Ordering::Relaxed);
                let edit = if preserve_domain {
                    snapshot.prepare_preserving_domains(&curve, active)
                } else {
                    snapshot.prepare(&curve, active)
                }
                .map_err(|e| e.to_string())?;
                let (_, _, _, raw) = edit
                    .authored_events()
                    .next()
                    .ok_or_else(|| "The curve draft has no source event".to_owned())?;
                LineEvent::read(kind, raw).map_err(|e| e.to_string())
            }))
            .unwrap_or_else(|_| Err("Custom easing preparation failed".into()));
            let _ = sender.send(result);
        });
    }
    fn poll(&mut self) {
        let Some(pending) = &self.pending else { return };
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("Custom easing preparation interrupted".into())
            }
        };
        let serial = pending.serial;
        self.pending = None;
        if !self.open || serial != self.serial {
            return;
        }
        match result {
            Ok(event) => {
                if let Some(request) = self.request.take() {
                    self.completed = Some((request.key, event));
                }
                self.open = false;
            }
            Err(error) => self.error = Some(error),
        }
    }
    fn finish_numeric(&mut self) {
        if let Some(before) = self.numeric_before.take() {
            self.remember(before);
        }
    }
}

impl Workbench {
    pub(crate) fn custom_easing_back(&mut self) {
        self.curve_templates.confirm = None;
        self.custom_easing.back();
    }
    pub(crate) fn open_custom_easing(&mut self, key: DraftKey) {
        let result = (|| {
            let mut event = key.event.clone();
            event.start_time = key.times[0].parse::<Beat>()?;
            event.end_time = key.times[1].parse::<Beat>()?;
            let snapshot =
                Snapshot::capture_draft(self.editor.document(), key.id, key.revision, &event)?;
            event = snapshot.sources[0].event.clone();
            let curve = phistudio_prpr::editable_easing(key.id.kind, &event)?;
            let domain = event.custom_easing.as_ref().map_or([0., 1.], |b| b.domain);
            Ok::<_, phistudio_core::ChartError>((snapshot, curve, domain))
        })();
        match result {
            Ok((snapshot, curve, domain)) => {
                self.custom_easing.close();
                self.curve_templates.confirm = None;
                self.custom_easing = State {
                    open: true,
                    request: Some(Request {
                        key,
                        snapshot,
                        domain,
                    }),
                    view: graph::Bounds::fit(&curve),
                    draft: Draft {
                        curve,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                self.timeline.cancel_gesture();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}
