//! Serialize checkpoint writes and clears so an old worker cannot resurrect
//! discarded edits after a save, project switch, or recovery decision.
use crate::workbench::Workbench;
use phistudio_core::{ProjectLocation, RecoverySnapshot, clear_recovery, save_recovery};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Key {
    session: u64,
    revision: u64,
    dirty: bool,
    file: Option<PathBuf>,
}
impl Key {
    fn new(app: &Workbench) -> Self {
        Self {
            session: app.session,
            revision: app.editor.revision(),
            dirty: app.editor.is_dirty(),
            file: app.file.clone(),
        }
    }
}
fn snapshot(app: &Workbench) -> RecoverySnapshot {
    RecoverySnapshot {
        document: app.editor.document().clone(),
        location: app.file.clone().map(|chart| ProjectLocation {
            chart,
            assets: app.asset_root().map(ToOwned::to_owned),
        }),
        beat: app.transport.beat,
        line: app.line,
    }
}
pub struct Checkpoints {
    root: Option<PathBuf>,
    pending: Option<(Key, Receiver<Result<(), String>>)>,
    written: Option<Key>,
    last_attempt: Instant,
}
impl Checkpoints {
    pub fn new(root: Option<PathBuf>) -> Self {
        Self {
            root,
            pending: None,
            written: None,
            last_attempt: Instant::now(),
        }
    }
    pub fn update(&mut self, app: &mut Workbench) {
        if app.files.recovery_loading || app.files.recovery.is_some() {
            return;
        }
        if let Some((_, receiver)) = &self.pending {
            match receiver.try_recv() {
                Ok(result) => {
                    let (key, _) = self.pending.take().unwrap();
                    match result {
                        Ok(()) => self.written = Some(key),
                        Err(error) => app.error = Some(format!("Could not save recovery: {error}")),
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    app.error = Some("Recovery worker interrupted".into());
                }
            }
        }
        let Some(root) = &self.root else {
            return;
        };
        let key = Key::new(app);
        if self.written.as_ref() == Some(&key) {
            return;
        }
        if key.dirty
            && self.last_attempt.elapsed()
                < Duration::from_secs(u64::from(app.options.recovery_seconds))
        {
            return;
        }
        let value = key.dirty.then(|| snapshot(app));
        let root = root.clone();
        let (sender, receiver) = mpsc::channel();
        self.pending = Some((key, receiver));
        self.last_attempt = Instant::now();
        std::thread::spawn(move || {
            let result = match value {
                Some(value) => save_recovery(&root, &value),
                None => clear_recovery(&root),
            }
            .map_err(|e| e.to_string());
            let _ = sender.send(result);
        });
    }
    /// Flush before leaving for a picker or an intentional application exit.
    pub fn flush(&mut self, app: &Workbench, discard: bool) -> Result<(), String> {
        let Some(root) = &self.root else {
            return Ok(());
        };
        if let Some((_, receiver)) = self.pending.take() {
            // Complete the old operation first; then publish the latest state.
            let _ = receiver.recv();
        }
        if (app.files.recovery_loading || app.files.recovery.is_some()) && !discard {
            return Ok(());
        }
        let key = Key::new(app);
        if discard || !key.dirty {
            clear_recovery(root)
        } else {
            save_recovery(root, &snapshot(app))
        }
        .map_err(|e| e.to_string())?;
        self.written = if discard && key.dirty {
            None
        } else {
            Some(key)
        };
        self.last_attempt = Instant::now();
        Ok(())
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    #[test]
    fn pending_startup_recovery_is_never_cleared_by_the_blank_home_document() {
        let root = tempfile::tempdir().unwrap();
        let recovery = RecoverySnapshot {
            document: phistudio_core::ChartDocument::empty("Recover me", 173.).unwrap(),
            location: None,
            beat: 12.,
            line: 0,
        };
        save_recovery(root.path(), &recovery).unwrap();
        let file = root.path().join("recovery/session.json");
        let before = std::fs::read(&file).unwrap();
        let mut app = Workbench::new(
            phistudio_core::ChartDocument::empty("Untitled", 120.).unwrap(),
            None,
        );
        app.show_projects = true;
        app.files.recovery_loading = true;
        let mut checkpoints = Checkpoints::new(Some(root.path().to_owned()));
        checkpoints.update(&mut app);
        checkpoints.flush(&app, false).unwrap();
        assert_eq!(std::fs::read(file).unwrap(), before);
        assert_eq!(
            phistudio_core::load_recovery(root.path())
                .unwrap()
                .unwrap()
                .document
                .name(),
            "Recover me"
        );
    }
}
