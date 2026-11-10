//! Installed-release notes: first-run acknowledgement uses existing preferences.
mod ui;
use super::service::{self, CheckError, Release};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
};

#[derive(Default)]
pub(super) enum Content {
    #[default]
    Idle,
    Loading,
    Ready(Release),
    Failed(CheckError),
}
#[derive(Default)]
pub(super) struct Notes {
    pub root: Option<PathBuf>,
    pub content: Content,
    pub requested: bool,
    pub open: bool,
    shown: bool,
    startup_checked: bool,
    pending: Option<Receiver<Result<Release, CheckError>>>,
}
impl Notes {
    pub fn tick(&mut self, seen: Option<&str>, managed: bool) {
        if !self.startup_checked {
            self.startup_checked = true;
            if managed && seen != Some(service::installed_build()) {
                self.request(true, false);
            }
        }
        if let Some(receiver) = &self.pending {
            let result = match receiver.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => Err(CheckError::Worker),
            };
            self.pending = None;
            self.content = match result {
                Ok(release) => Content::Ready(release),
                Err(error) => Content::Failed(error),
            };
        }
    }
    pub fn request(&mut self, show: bool, refresh: bool) {
        self.requested |= show && !self.open;
        if self.pending.is_some() || (!refresh && matches!(self.content, Content::Ready(_))) {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        self.content = Content::Loading;
        self.shown = false;
        let root = self.root.clone();
        std::thread::spawn(move || {
            let _ = sender.send(load(root.as_deref(), refresh, service::check_installed));
        });
    }
    pub fn ensure_metadata(&mut self) {
        if matches!(self.content, Content::Idle) {
            self.request(false, false);
        }
    }
    pub fn presented(&mut self) {
        self.shown = true;
    }
    pub fn close(&mut self) -> Option<String> {
        self.open = false;
        self.requested = false;
        let acknowledged = self.shown && matches!(self.content, Content::Ready(_));
        self.shown = false;
        acknowledged.then(|| service::installed_build().to_owned())
    }
    pub fn release(&self) -> Option<&Release> {
        if let Content::Ready(release) = &self.content {
            Some(release)
        } else {
            None
        }
    }
}
fn read_cache(root: &Path) -> Option<Release> {
    let path = root.join("release-notes.json");
    let meta = std::fs::symlink_metadata(&path).ok()?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > service::MAX_BYTES as u64 {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(service::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > service::MAX_BYTES {
        return None;
    }
    let release = service::parse_release(&bytes, true).ok()?;
    (release.version.to_string() == service::installed_version()).then_some(release)
}
fn write_cache(root: &Path, release: &Release) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    let raw = serde_json::json!({"tag_name":format!("v{}",release.version),"html_url":release.url,"name":release.name,"body":release.body,"draft":false,"prerelease":!release.version.pre.is_empty()});
    file.write_all(&serde_json::to_vec(&raw)?)?;
    file.as_file().sync_all()?;
    file.persist(root.join("release-notes.json"))
        .map_err(|e| e.error)?;
    Ok(())
}
fn load(
    root: Option<&Path>,
    refresh: bool,
    check: impl FnOnce() -> Result<Release, CheckError>,
) -> Result<Release, CheckError> {
    let cached = root.and_then(read_cache);
    if !refresh && let Some(release) = &cached {
        return Ok(release.clone());
    }
    match check() {
        Ok(release) => {
            if let Some(root) = root
                && let Err(error) = write_cache(root, &release)
            {
                eprintln!("Could not cache release notes: {error}");
            }
            Ok(release)
        }
        Err(error) => cached.ok_or(error),
    }
}

#[cfg(test)]
mod tests;
