use crate::workbench::Workbench;
use phistudio_core::EditorPreferences;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};
/// Serialize small preference writes so an older worker cannot overwrite a newer layout.
pub(crate) struct Preferences {
    root: Option<PathBuf>,
    loaded: bool,
    observed: EditorPreferences,
    saved: EditorPreferences,
    changed: Instant,
    pending: Option<(EditorPreferences, Receiver<Result<(), String>>)>,
}
impl Preferences {
    pub fn new(root: Option<PathBuf>) -> Self {
        Self {
            root,
            loaded: false,
            observed: EditorPreferences::default(),
            saved: EditorPreferences::default(),
            changed: Instant::now(),
            pending: None,
        }
    }
    pub fn update(&mut self, app: &mut Workbench) {
        if !self.loaded {
            if let Some(root) = &self.root {
                match phistudio_core::load_editor_preferences(root) {
                    Ok(preferences) => app.apply_preferences(&preferences),
                    Err(error) => {
                        app.error = Some(format!("Could not load editor settings: {error}"))
                    }
                }
            }
            self.observed = app.preferences();
            self.saved = self.observed.clone();
            self.loaded = true;
        }
        if let Some((_, receiver)) = &self.pending {
            match receiver.try_recv() {
                Ok(result) => {
                    let (preferences, _) = self.pending.take().unwrap();
                    match result {
                        Ok(()) => self.saved = preferences,
                        Err(error) => {
                            app.error = Some(error);
                            self.changed = Instant::now();
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.changed = Instant::now();
                    app.error = Some("Editor settings save interrupted".into());
                }
            }
        }
        let current = app.preferences();
        if current != self.observed {
            self.observed = current;
            self.changed = Instant::now();
        }
        if self.pending.is_none()
            && self.observed != self.saved
            && self.changed.elapsed() >= Duration::from_millis(600)
            && let Some(root) = self.root.clone()
        {
            let preferences = self.observed.clone();
            let snapshot = preferences.clone();
            let (sender, receiver) = mpsc::channel();
            self.pending = Some((preferences, receiver));
            std::thread::spawn(move || {
                let _ = sender.send(
                    phistudio_core::save_editor_preferences(&root, &snapshot)
                        .map_err(|e| format!("Could not save editor settings: {e}")),
                );
            });
        }
    }
    pub fn flush(&mut self, app: &Workbench) -> Result<(), String> {
        if !self.loaded {
            return Ok(());
        }
        if let Some((preferences, receiver)) = self.pending.take() {
            receiver
                .recv()
                .map_err(|_| "Editor settings save interrupted".to_owned())??;
            self.saved = preferences;
        }
        if let Some(root) = &self.root {
            let current = app.preferences();
            if current != self.saved {
                phistudio_core::save_editor_preferences(root, &current)
                    .map_err(|e| format!("Could not save editor settings: {e}"))?;
                self.saved = current;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shutdown_flush_waits_for_an_older_write_before_persisting_a_reverted_value() {
        let root = tempfile::tempdir().unwrap();
        let mut app = Workbench::new(
            phistudio_core::ChartDocument::empty("Settings", 120.).unwrap(),
            None,
        );
        let mut store = Preferences::new(Some(root.path().to_owned()));
        store.update(&mut app);
        let mut older = app.preferences();
        older.division = 16;
        let (sender, receiver) = mpsc::channel();
        phistudio_core::save_editor_preferences(root.path(), &older).unwrap();
        sender.send(Ok(())).unwrap();
        store.pending = Some((older, receiver));
        store.flush(&app).unwrap();
        assert_eq!(
            phistudio_core::load_editor_preferences(root.path())
                .unwrap()
                .division,
            4
        );
    }
}
