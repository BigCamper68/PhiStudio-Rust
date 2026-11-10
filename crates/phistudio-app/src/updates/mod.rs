//! Session-wide update state; checks run on a worker and never block editing.
mod about;
mod notes;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) mod probe;
mod service;
#[cfg(test)]
mod tests;
mod ui;
use service::{CheckError, Release};
use std::sync::mpsc::{self, Receiver};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Status {
    #[default]
    Idle,
    Checking,
    Current(Release),
    Available(Release),
    Failed(CheckError),
}
struct Pending {
    receiver: Receiver<Result<Release, CheckError>>,
    automatic: bool,
}
#[derive(Default)]
pub(crate) struct State {
    notes: notes::Notes,
    identity: about::Identity,
    status: Status,
    pending: Option<Pending>,
    startup_checked: bool,
    dismissed: bool,
}
impl State {
    pub(crate) fn set_cache_root(&mut self, root: Option<std::path::PathBuf>) {
        self.notes.root = root;
    }
    pub(crate) fn tick(&mut self, automatic: bool) {
        if !self.startup_checked {
            self.startup_checked = true;
            if automatic {
                self.start(true);
            }
        }
        self.poll(automatic);
    }
    fn start(&mut self, automatic: bool) {
        self.start_with(automatic, service::check);
    }
    fn start_with(
        &mut self,
        automatic: bool,
        check: impl FnOnce() -> Result<Release, CheckError> + Send + 'static,
    ) {
        if self.pending.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(Pending {
            receiver,
            automatic,
        });
        self.status = Status::Checking;
        std::thread::spawn(move || {
            let _ = sender.send(check());
        });
    }
    fn poll(&mut self, automatic: bool) {
        let Some(pending) = &self.pending else {
            return;
        };
        if pending.automatic && !automatic {
            self.pending = None;
            self.status = Status::Idle;
            return;
        }
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(CheckError::Worker),
        };
        self.pending = None;
        self.finish(result);
    }
    fn finish(&mut self, result: Result<Release, CheckError>) {
        self.status = match result.and_then(|release| {
            service::newer(&release, service::installed_version()).map(|newer| (release, newer))
        }) {
            Ok((release, true)) => {
                self.dismissed = false;
                Status::Available(release)
            }
            Ok((release, false)) => Status::Current(release),
            Err(error) => Status::Failed(error),
        };
    }
    fn notification(&self) -> Option<&Release> {
        if self.dismissed {
            return None;
        }
        if let Status::Available(release) = &self.status {
            Some(release)
        } else {
            None
        }
    }
}
