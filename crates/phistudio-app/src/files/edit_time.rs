//! Editing time is monotonic and checkpointed separately from chart edits.
use crate::workbench::Workbench;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const CHECKPOINT: Duration = Duration::from_secs(15);
const FRAME_GAP: Duration = Duration::from_secs(5);
#[derive(Default)]
struct Elapsed {
    time: Duration,
    saved_ms: u64,
}
impl Elapsed {
    fn milliseconds(&self) -> u64 {
        self.time.as_millis().min(u64::MAX as u128) as u64
    }
}
struct Write {
    workspace: PathBuf,
    milliseconds: u64,
    result: Receiver<Result<(), String>>,
}
pub(super) struct EditTime {
    root: Option<PathBuf>,
    session: String,
    last: Option<(PathBuf, Instant)>,
    elapsed: BTreeMap<PathBuf, Elapsed>,
    pending: Option<Write>,
    last_attempt: Option<Instant>,
}
impl EditTime {
    pub fn new(root: Option<PathBuf>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Self {
            root,
            session: format!("{:x}-{now:x}", std::process::id()),
            last: None,
            elapsed: BTreeMap::new(),
            pending: None,
            last_attempt: None,
        }
    }
    fn sample(&mut self, workspace: Option<PathBuf>, now: Instant) {
        let next = workspace.map(|p| (p, now));
        if let (Some((previous, then)), Some((current, _))) = (&self.last, &next)
            && previous == current
            && let Some(delta) = now.checked_duration_since(*then)
            && delta <= FRAME_GAP
        {
            let entry = self.elapsed.entry(current.clone()).or_default();
            entry.time = entry.time.saturating_add(delta);
        }
        self.last = next;
    }
    fn active_workspace(app: &Workbench) -> Option<PathBuf> {
        if app.editor_focused
            && !app.show_projects
            && !app.files.busy
            && !app.files.recovery_loading
        {
            app.current_workspace()
        } else {
            None
        }
    }
    pub fn update(&mut self, app: &mut Workbench) {
        let Some(root) = &self.root else {
            return;
        };
        let now = Instant::now();
        let root = root.clone();
        self.sample(Self::active_workspace(app), now);
        if let Some(write) = &self.pending {
            let result = match write.result.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err("Editing time writer interrupted".into())
                }
            };
            if let Some(write) = self.pending.take() {
                match result {
                    Ok(()) => self.mark_saved(&write.workspace, write.milliseconds),
                    Err(error) => app.error = Some(error),
                }
            }
        }
        if self
            .last_attempt
            .is_some_and(|last| now.duration_since(last) < CHECKPOINT)
        {
            return;
        }
        if let Some((workspace, entry)) = self.elapsed.iter().find(|(_, e)| {
            e.milliseconds().saturating_sub(e.saved_ms) >= CHECKPOINT.as_millis() as u64
        }) {
            let milliseconds = entry.milliseconds();
            let workspace = workspace.clone();
            let destination = workspace.clone();
            let session = self.session.clone();
            let (sender, result) = mpsc::channel();
            self.pending = Some(Write {
                workspace,
                milliseconds,
                result,
            });
            self.last_attempt = Some(now);
            std::thread::spawn(move || {
                let _ = sender.send(write_time(&root, &destination, &session, milliseconds));
            });
        }
    }
    fn mark_saved(&mut self, workspace: &Path, milliseconds: u64) {
        if let Some(entry) = self.elapsed.get_mut(workspace) {
            entry.saved_ms = entry.saved_ms.max(milliseconds);
        }
    }
    /// Pause at a lifecycle boundary and publish the last partial checkpoint.
    pub fn flush(&mut self, app: &Workbench) -> Result<(), String> {
        self.sample(Self::active_workspace(app), Instant::now());
        self.last = None;
        self.flush_records()
    }
    fn flush_records(&mut self) -> Result<(), String> {
        if let Some(write) = self.pending.take() {
            write
                .result
                .recv()
                .map_err(|_| "Editing time writer interrupted".to_owned())??;
            self.mark_saved(&write.workspace, write.milliseconds);
        }
        let Some(root) = &self.root else {
            return Ok(());
        };
        for (workspace, entry) in &mut self.elapsed {
            let milliseconds = entry.milliseconds();
            if milliseconds > entry.saved_ms {
                write_time(root, workspace, &self.session, milliseconds)?;
                entry.saved_ms = milliseconds;
            }
        }
        Ok(())
    }
}
fn write_time(
    root: &Path,
    workspace: &Path,
    session: &str,
    milliseconds: u64,
) -> Result<(), String> {
    phistudio_core::record_project_edit_time(root, workspace, session, milliseconds)
        .map(|_| ())
        .map_err(|error| format!("Could not save editing time: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use phistudio_core::{ChartDocument, create_project, project_edit_time};

    #[test]
    fn foreground_work_accumulates_across_switches_and_process_sessions() {
        let root = tempfile::tempdir().unwrap();
        let doc = ChartDocument::empty("Time", 120.).unwrap();
        let first = create_project(root.path(), &doc).unwrap().assets.unwrap();
        let other = create_project(root.path(), &doc).unwrap().assets.unwrap();
        let mut clock = EditTime::new(Some(root.path().into()));
        let start = Instant::now();
        let mut sample = |workspace: Option<&Path>, ms| {
            clock.sample(
                workspace.map(Path::to_owned),
                start + Duration::from_millis(ms),
            );
        };
        sample(Some(&first), 0);
        sample(Some(&first), 1225);
        sample(None, 1300);
        sample(Some(&first), 19_000);
        sample(Some(&first), 19_775);
        sample(Some(&other), 19_900);
        sample(Some(&other), 20_400);
        sample(Some(&other), 40_400); // Suspension without a focus frame.
        clock.flush_records().unwrap();
        clock.flush_records().unwrap();
        assert_eq!(project_edit_time(root.path(), &first).unwrap(), 2000);
        assert_eq!(project_edit_time(root.path(), &other).unwrap(), 500);
        let mut reopened = EditTime::new(Some(root.path().into()));
        reopened.sample(Some(first.clone()), start);
        reopened.sample(Some(first.clone()), start + Duration::from_millis(900));
        reopened.flush_records().unwrap();
        assert_eq!(project_edit_time(root.path(), &first).unwrap(), 2900);
    }
}
