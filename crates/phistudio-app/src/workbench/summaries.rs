//! Immutable line labels/counts shared by UI views for one document revision.
use super::Workbench;
use phistudio_core::LineSummary;
use std::sync::Arc;

pub(super) struct LineCache {
    session: u64,
    revision: u64,
    lines: Arc<Vec<LineSummary>>,
}

impl Workbench {
    pub(crate) fn line_summaries(&mut self) -> Arc<Vec<LineSummary>> {
        let revision = self.editor.revision();
        if self
            .line_cache
            .as_ref()
            .is_none_or(|cache| cache.session != self.session || cache.revision != revision)
        {
            self.line_cache = Some(LineCache {
                session: self.session,
                revision,
                lines: Arc::new(self.editor.document().lines()),
            });
        }
        Arc::clone(&self.line_cache.as_ref().unwrap().lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phistudio_core::{ChartDocument, Command, Note};

    #[test]
    fn invalidates_after_edits_undo_redo_and_replacing_a_project() {
        let mut app = Workbench::new(ChartDocument::empty("First", 120.).unwrap(), None);
        let initial = app.line_summaries();
        app.beat = 24.;
        app.playing = true;
        assert!(Arc::ptr_eq(&initial, &app.line_summaries()));

        app.execute(Command::AddNote {
            line: 0,
            note: Note::default(),
        });
        let edited = app.line_summaries();
        assert!(!Arc::ptr_eq(&initial, &edited));
        assert_eq!(edited[0].note_count, 1);
        assert_eq!(initial[0].note_count, 0);
        assert!(app.editor.undo());
        assert_eq!(app.line_summaries()[0].note_count, 0);
        assert!(app.editor.redo());
        assert_eq!(app.line_summaries()[0].note_count, 1);

        let mut properties = app.editor.document().line_properties(0).unwrap();
        properties.name = "新しいライン".into();
        app.execute(Command::EditLine {
            index: 0,
            properties,
        });
        assert_eq!(app.line_summaries()[0].name, "新しいライン");
        app.execute(Command::AddLine {
            name: "Second".into(),
        });
        assert_eq!(app.line_summaries().len(), 2);
        app.execute(Command::DeleteLine(1));
        assert_eq!(app.line_summaries().len(), 1);

        // The new Editor starts at revision zero, as the original cached one did.
        app.replace_document(ChartDocument::empty("Next", 120.).unwrap(), None, None);
        let replaced = app.line_summaries();
        assert!(!Arc::ptr_eq(&initial, &replaced));
        assert_eq!(replaced[0].note_count, 0);
        assert_eq!(replaced[0].name, app.editor.document().lines()[0].name);
    }

    #[test]
    fn repeated_reads_reuse_all_line_metadata() {
        let mut value = ChartDocument::empty("Many lines", 120.)
            .unwrap()
            .as_json()
            .clone();
        let line = value["judgeLineList"][0].clone();
        value["judgeLineList"] = serde_json::Value::Array(
            (0..1000)
                .map(|i| {
                    let mut line = line.clone();
                    line["Name"] = serde_json::json!(format!("Line {i} · 中文"));
                    line
                })
                .collect(),
        );
        let mut app = Workbench::new(ChartDocument::from_value(value).unwrap(), None);
        let expected = app.editor.document().lines();
        let cached = app.line_summaries();
        assert_eq!(expected.len(), cached.len());
        for (a, b) in expected.iter().zip(cached.iter()) {
            assert_eq!(
                (&a.name, a.index, a.note_count),
                (&b.name, b.index, b.note_count)
            );
        }
        let start = std::time::Instant::now();
        for _ in 0..500 {
            std::hint::black_box(app.editor.document().lines());
        }
        let uncached_ms = start.elapsed().as_secs_f64() * 1000.;
        let start = std::time::Instant::now();
        for _ in 0..500 {
            assert!(Arc::ptr_eq(
                &cached,
                &std::hint::black_box(app.line_summaries())
            ));
        }
        let cached_ms = start.elapsed().as_secs_f64() * 1000.;
        eprintln!(
            "line metadata: 1000 lines, 500 reads; uncached={uncached_ms:.3}ms cached={cached_ms:.3}ms"
        );
        assert!(!app.editor.is_dirty());
    }
}
