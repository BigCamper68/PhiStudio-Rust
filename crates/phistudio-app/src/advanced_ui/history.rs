use super::*;

impl Workbench {
    pub(crate) fn anchor(&self) -> Anchor {
        if let Some(id) = self.selected
            && let Ok(n) = self.editor.document().note(id.line, id.index)
        {
            return Anchor {
                line: id.line,
                beat: n.start_time.as_f64(),
                x: n.x,
            };
        }
        if let Some(id) = self.events.selected
            && let Ok(e) = self.editor.document().event(id)
        {
            return Anchor {
                line: id.line,
                beat: e.start_time.as_f64(),
                x: self.timeline.camera_x,
            };
        }
        Anchor {
            line: self.line,
            beat: self.transport.beat,
            x: self.timeline.camera_x,
        }
    }

    pub(crate) fn remember_anchor(&mut self, anchor: Anchor) {
        self.advanced
            .anchors
            .insert(self.editor.current_state_id(), anchor);
        while self.advanced.anchors.len() > 600 {
            let key = *self.advanced.anchors.keys().next().unwrap();
            self.advanced.anchors.remove(&key);
        }
    }

    pub(crate) fn go_to_note(&mut self, id: NoteId) {
        if let Ok(note) = self.editor.document().note(id.line, id.index) {
            self.close_fullscreen_preview();
            self.line = id.line;
            self.transport.beat = (note.start_time.as_f64() - 1.).max(0.);
            self.timeline.camera_x = if note.x.abs() <= 675. { 0. } else { note.x };
            self.select(Some(id));
            self.unlink_invalid_xy();
            self.windows.sidebar = true;
            self.status = format!("Selected L{} note {}", id.line, id.index + 1);
            self.transport.playing = false;
        }
    }
}
