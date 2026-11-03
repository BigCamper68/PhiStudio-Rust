use super::*;

impl Editor {
    pub(super) fn record(
        &mut self,
        delta: Delta,
        label: impl Into<std::borrow::Cow<'static, str>>,
    ) {
        self.history.truncate(self.cursor);
        self.history.push(Entry {
            delta,
            label: label.into(),
            before: self.state,
            after: self.next_state,
        });
        self.state = self.next_state;
        self.next_state += 1;
        self.cursor += 1;
        self.revision += 1;
        if self.history.len() > 512 {
            self.history.remove(0);
            self.cursor -= 1;
        }
    }

    pub fn undo(&mut self) -> bool {
        if !self.can_undo() {
            return false;
        }
        self.cursor -= 1;
        let entry = &self.history[self.cursor];
        entry.delta.apply(&mut self.document, false);
        self.state = entry.before;
        self.revision += 1;
        true
    }

    pub fn redo(&mut self) -> bool {
        if !self.can_redo() {
            return false;
        }
        let entry = &self.history[self.cursor];
        entry.delta.apply(&mut self.document, true);
        self.state = entry.after;
        self.cursor += 1;
        self.revision += 1;
        true
    }
}
