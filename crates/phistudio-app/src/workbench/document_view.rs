//! Revision-scoped read models. A new document/session invalidates them together.
use crate::note_index::NoteIndex;
use phistudio_core::{Editor, LineSummary, Note};
use std::{collections::BTreeMap, sync::Arc};
struct Notes {
    values: Arc<Vec<(usize, Note)>>,
    index: NoteIndex,
}
#[derive(Default)]
pub(super) struct DocumentView {
    key: Option<(u64, u64)>,
    notes: BTreeMap<usize, Notes>,
    lines: Option<Arc<Vec<LineSummary>>>,
    extent: Option<f64>,
    counts: Option<(usize, usize)>,
}
impl DocumentView {
    fn refresh(&mut self, editor: &Editor, session: u64) {
        let key = (session, editor.revision());
        if self.key != Some(key) {
            *self = Self {
                key: Some(key),
                ..Self::default()
            };
        }
    }
    fn line(&mut self, editor: &Editor, session: u64, line: usize) -> &Notes {
        self.refresh(editor, session);
        self.notes.entry(line).or_insert_with(|| {
            let values = editor.document().notes(line);
            Notes {
                index: NoteIndex::new(&values),
                values: Arc::new(values),
            }
        })
    }
    pub fn notes(&mut self, editor: &Editor, session: u64, line: usize) -> Arc<Vec<(usize, Note)>> {
        Arc::clone(&self.line(editor, session, line).values)
    }
    pub fn candidates(
        &mut self,
        editor: &Editor,
        session: u64,
        line: usize,
        first: f64,
        last: f64,
    ) -> Vec<usize> {
        self.line(editor, session, line).index.query(first, last)
    }
    pub fn summaries(&mut self, editor: &Editor, session: u64) -> Arc<Vec<LineSummary>> {
        self.refresh(editor, session);
        Arc::clone(
            self.lines
                .get_or_insert_with(|| Arc::new(editor.document().lines())),
        )
    }
    pub fn counts(&mut self, editor: &Editor, session: u64) -> (usize, usize) {
        self.refresh(editor, session);
        *self.counts.get_or_insert_with(|| {
            let doc = editor.document();
            (doc.note_count(), doc.event_count())
        })
    }
    pub fn seek_extent(&mut self, editor: &Editor, session: u64) -> f64 {
        self.refresh(editor, session);
        *self.extent.get_or_insert_with(|| {
            let document = editor.document();
            let last = (0..document.line_count())
                .flat_map(|line| document.iter_notes(line))
                .map(|(_, note)| note.end_time.as_f64().max(note.start_time.as_f64()))
                .fold(0_f64, f64::max);
            ((last + 8.) / 16.).ceil().max(4.) * 16.
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use phistudio_core::{Beat, ChartDocument, Command, NoteId};
    #[test]
    fn notes_index_summaries_and_extent_follow_edits_history_and_session() {
        let mut editor = Editor::new(ChartDocument::empty("First", 120.).unwrap());
        let mut view = DocumentView::default();
        let empty = view.notes(&editor, 0, 0);
        assert!(Arc::ptr_eq(&empty, &view.notes(&editor, 0, 0)));
        assert_eq!(view.seek_extent(&editor, 0), 64.);
        editor
            .execute(Command::AddNote {
                line: 0,
                note: Note {
                    start_time: Beat::new(100, 0, 1).unwrap(),
                    end_time: Beat::new(100, 0, 1).unwrap(),
                    ..Default::default()
                },
            })
            .unwrap();
        assert_eq!(view.summaries(&editor, 0)[0].note_count, 1);
        assert_eq!(view.candidates(&editor, 0, 0, 99., 101.), vec![0]);
        assert_eq!(view.seek_extent(&editor, 0), 112.);
        assert!(!Arc::ptr_eq(&empty, &view.notes(&editor, 0, 0)));
        editor
            .execute(Command::DeleteNote(NoteId { line: 0, index: 0 }))
            .unwrap();
        assert!(view.candidates(&editor, 0, 0, 99., 101.).is_empty());
        assert!(editor.undo());
        assert_eq!(view.candidates(&editor, 0, 0, 99., 101.), vec![0]);
        assert!(editor.redo());
        assert_eq!(view.seek_extent(&editor, 0), 64.);
        // An independently opened project can have the same revision.
        let other = Editor::new(ChartDocument::empty("Other", 120.).unwrap());
        let before = view.notes(&other, 1, 0);
        assert!(!Arc::ptr_eq(&empty, &before));
        assert!(before.is_empty());
        assert_eq!(view.summaries(&other, 1).len(), 1);
    }
}
