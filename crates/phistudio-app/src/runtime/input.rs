//! Track dialog transitions that must release the platform's current pointer.
use crate::workbench::Workbench;

#[derive(Default)]
pub(crate) struct ModalInput {
    blocked: bool,
}

impl ModalInput {
    pub(crate) fn update(&mut self, app: &Workbench) -> bool {
        // Curve capture blocks chart tools while retaining the current pointer.
        // Resetting here would cancel every graph press on the following frame,
        // including an asynchronous edit still waiting to commit after release.
        let blocked = app.show_projects || app.modal_open_except_graphs();
        let reset = blocked && !self.blocked;
        self.blocked = blocked;
        reset
    }
}
