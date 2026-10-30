//! Movie settings and a bounded offline producer/encoder pipeline.
mod settings;
use settings::video_settings;
mod files;
#[cfg(target_os = "android")]
pub(crate) use files::clear_stale_movies;
use files::movie_name;
mod dialog;
mod generation;
use crate::{
    i18n::{format as fmt, tr},
    scroll::{TouchCombo as _, TouchScroll as _},
    workbench::Workbench,
};
use anyhow::{Context, Result};
use phistudio_core::video::{VideoPlan, VideoSettings};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::path::Path;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    time::Instant,
};

pub(crate) struct MovieFile {
    pub directory: tempfile::TempDir,
    pub name: String,
}
impl MovieFile {
    pub fn path(&self) -> PathBuf {
        self.directory.path().join("movie.mp4")
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub(crate) fn save(&self, path: &Path) -> Result<()> {
        use std::io;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut staged = tempfile::NamedTempFile::new_in(parent)?;
        io::copy(&mut std::fs::File::open(self.path())?, &mut staged)?;
        staged.as_file().sync_all()?;
        staged.persist(path).map_err(|e| e.error)?;
        Ok(())
    }
}
#[derive(Default)]
pub(crate) struct VideoState {
    pub open: bool,
    pub start: bool,
    pub settings: VideoSettings,
    pub(crate) name: String,
    pub(crate) destination: String,
    pub job: Option<MovieJob>,
    pub completed: Option<MovieFile>,
    pub(crate) error: Option<String>,
    pub message: Option<&'static str>,
}
pub(crate) struct MovieJob {
    renderer: MovieRenderer,
    plan: VideoPlan,
    warm: u64,
    next: u64,
    last_time: Option<f64>,
    pending: Option<Vec<u8>>,
    sender: Option<SyncSender<Option<Vec<u8>>>>,
    receiver: Receiver<Result<MovieFile, String>>,
    pub cancel: Arc<AtomicBool>,
    encoded: Arc<AtomicU64>,
    phase: Arc<AtomicU8>,
}
struct MovieRenderer(phistudio_prpr::Preview);
impl std::ops::Deref for MovieRenderer {
    type Target = phistudio_prpr::Preview;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for MovieRenderer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for MovieRenderer {
    fn drop(&mut self) {
        self.0.dispose_movie();
    }
}
impl Drop for MovieJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.sender.take();
    }
}
impl MovieJob {
    fn update(&mut self) -> Option<Result<MovieFile, String>> {
        match self.receiver.try_recv() {
            Ok(result) => return Some(result),
            Err(mpsc::TryRecvError::Disconnected) => {
                return Some(Err("Video export worker interrupted".into()));
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if self.cancel.load(Ordering::Relaxed) {
            self.sender.take();
            return None;
        }
        if self.phase.load(Ordering::Acquire) != 1 {
            return None;
        }
        let budget = Instant::now();
        while budget.elapsed().as_millis() < 8 {
            let warm_time = self.warm as f64 / f64::from(self.plan.fps);
            if warm_time < self.plan.start {
                let dt = self.last_time.map_or(0., |t| (warm_time - t) as f32);
                self.renderer.movie_frame(warm_time, dt);
                self.last_time = Some(warm_time);
                self.warm += 1;
                continue;
            }
            if self.next >= self.plan.frames {
                if let Some(sender) = &self.sender {
                    match sender.try_send(None) {
                        Ok(()) => {
                            self.sender = None;
                        }
                        Err(TrySendError::Full(_)) => {}
                        Err(TrySendError::Disconnected(_)) => {}
                    }
                }
                break;
            }
            let pixels = if let Some(pixels) = self.pending.take() {
                pixels
            } else {
                let time = self.plan.time(self.next);
                let dt = self.last_time.map_or(0., |t| (time - t) as f32);
                self.renderer.movie_frame(time, dt);
                self.last_time = Some(time);
                let mut image = self.renderer.snapshot();
                #[cfg(all(debug_assertions, not(any(target_os = "android", target_os = "ios"))))]
                if let Some(root) = std::env::var_os("PHISTUDIO_TEST_MOVIE_FRAMES") {
                    let path = std::path::PathBuf::from(root)
                        .join(format!("{:.6}-{:06}.png", self.plan.start, self.next));
                    image.export_png(path.to_str().unwrap());
                }
                flip_rows(
                    &mut image.bytes,
                    self.plan.width as usize,
                    self.plan.height as usize,
                );
                image.bytes
            };
            match self.sender.as_ref().unwrap().try_send(Some(pixels)) {
                Ok(()) => self.next += 1,
                Err(TrySendError::Full(Some(pixels))) => {
                    self.pending = Some(pixels);
                    break;
                }
                Err(TrySendError::Disconnected(_)) => break,
                Err(TrySendError::Full(None)) => unreachable!(),
            }
        }
        None
    }
}
fn flip_rows(bytes: &mut [u8], width: usize, height: usize) {
    let row = width * 4;
    for y in 0..height / 2 {
        let (a, b) = bytes.split_at_mut((height - 1 - y) * row);
        a[y * row..(y + 1) * row].swap_with_slice(&mut b[..row]);
    }
}

#[cfg(test)]
mod tests;
