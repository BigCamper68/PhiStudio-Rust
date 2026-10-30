use crate::{scroll::TouchScroll as _, theme::*, workbench::Workbench};
use phistudio_core::{clipboard_history::History, objects::ObjectClipboard};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
};
enum Job {
    Push(Arc<ObjectClipboard>, Option<PathBuf>),
    Rename(String, String),
    Pin(String, bool),
    Remove(String),
    Activate(String, Option<PathBuf>, u64),
    Flush(mpsc::Sender<()>),
}
enum Reply {
    History(History),
    Active(ObjectClipboard, u64),
}
#[derive(Default)]
pub(crate) struct ClipboardState {
    root: Option<PathBuf>,
    send: Option<mpsc::Sender<Job>>,
    receive: Option<mpsc::Receiver<Result<Reply, String>>>,
    history: History,
    observed: Option<Arc<ObjectClipboard>>,
    selected: Option<String>,
    name: String,
}
impl ClipboardState {
    fn send(&self, job: Job) {
        if let Some(tx) = &self.send {
            let _ = tx.send(job);
        }
    }
    pub fn flush(&self) -> Result<(), String> {
        if let Some(tx) = &self.send {
            let (a, b) = mpsc::channel();
            if tx.send(Job::Flush(a)).is_err() {
                return Ok(());
            }
            b.recv_timeout(std::time::Duration::from_secs(15))
                .map_err(|_| "Clipboard write interrupted".to_owned())?;
        }
        Ok(())
    }
}
impl Workbench {
    pub(crate) fn update_clipboard_history(&mut self, root: &Path) {
        let state = &mut self.clipboard_history;
        if state.root.as_deref() != Some(root) {
            let root = root.join("clipboard-history");
            state.root = Some(root.parent().unwrap().to_owned());
            let (tx, rx) = mpsc::channel();
            let (out, input) = mpsc::channel();
            state.send = Some(tx);
            state.receive = Some(input);
            std::thread::spawn(move || {
                let mut history = match History::load(&root) {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = out.send(Err(e.to_string()));
                        return;
                    }
                };
                let _ = out.send(Ok(Reply::History(history.clone())));
                while let Ok(job) = rx.recv() {
                    let result = match job {
                        Job::Push(c, a) => history.push(&root, (*c).clone(), a.as_deref()),
                        Job::Rename(id, n) => history.rename(&root, &id, &n),
                        Job::Pin(id, p) => history.pin(&root, &id, p),
                        Job::Remove(id) => history.remove(&root, &id),
                        Job::Activate(id, assets, session) => {
                            let result = history
                                .activate(&root, &id, assets.as_deref())
                                .map(|c| Reply::Active(c, session))
                                .map_err(|e| e.to_string());
                            let _ = out.send(result);
                            continue;
                        }
                        Job::Flush(tx) => {
                            let _ = tx.send(());
                            continue;
                        }
                    };
                    let _ = out.send(
                        result
                            .map(|()| Reply::History(history.clone()))
                            .map_err(|e| e.to_string()),
                    );
                }
            });
        }
        let mut active = None;
        if let Some(rx) = &state.receive {
            while let Ok(result) = rx.try_recv() {
                match result {
                    Ok(Reply::History(h)) => state.history = h,
                    Ok(Reply::Active(c, session)) if session == self.session => {
                        active = Some(Arc::new(c))
                    }
                    Ok(_) => {}
                    Err(e) => self.error = Some(e),
                }
            }
        }
        if let Some(c) = active {
            state.observed = Some(c.clone());
            self.tools.clipboard = Some(c);
            self.status = "Clipboard entry activated".into();
        }
        let state = &self.clipboard_history;
        let changed = match (&state.observed, &self.tools.clipboard) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, Some(_)) => true,
            _ => false,
        };
        if changed && let Some(clipboard) = self.tools.clipboard.clone() {
            state.send(Job::Push(
                clipboard.clone(),
                self.asset_root().map(Path::to_owned),
            ));
            self.clipboard_history.observed = Some(clipboard);
        }
    }
    pub(crate) fn clipboard_history_ui(&mut self, ui: &mut egui::Ui) {
        let assets = self.asset_root().map(Path::to_owned);
        let mut state = std::mem::take(&mut self.clipboard_history);
        ui.strong(crate::i18n::tr("Clipboard history"));
        ui.small(crate::i18n::tr("20 recent copies and up to 20 pinned entries. Activating only changes the clipboard; paste when ready."));
        egui::ScrollArea::vertical()
            .max_height(140.)
            .touch_show(ui, |ui| {
                for (i, entry) in state.history.entries.iter().enumerate() {
                    let name = if entry.name.is_empty() {
                        crate::i18n::format("Copy {0}", &[(i + 1).to_string()])
                    } else {
                        entry.name.clone()
                    };
                    let label = format!(
                        "{}{} · {}",
                        if entry.pinned { "★ " } else { "" },
                        name,
                        entry.objects
                    );
                    if ui
                        .selectable_label(state.selected.as_deref() == Some(&entry.id), label)
                        .clicked()
                    {
                        state.selected = Some(entry.id.clone());
                        state.name = entry.name.clone();
                    }
                }
            });
        if let Some(entry) = state
            .history
            .entries
            .iter()
            .find(|e| Some(&e.id) == state.selected.as_ref())
        {
            let (id, pinned, objects, lines, preview) = (
                entry.id.clone(),
                entry.pinned,
                entry.objects,
                entry.lines,
                entry.preview.clone(),
            );
            ui.label(crate::i18n::format(
                "{0} objects · {1} lines",
                &[objects.to_string(), lines.to_string()],
            ));
            paint_preview(ui, &preview);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.name)
                        .desired_width(170.)
                        .hint_text(crate::i18n::tr("Name")),
                );
                if ui.button(crate::i18n::tr("Rename")).clicked() {
                    state.send(Job::Rename(id.clone(), state.name.clone()));
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.button(crate::i18n::tr("Activate clipboard")).clicked() {
                    state.send(Job::Activate(id.clone(), assets.clone(), self.session));
                }
                if ui
                    .button(crate::i18n::tr(if pinned { "Unpin" } else { "Pin" }))
                    .clicked()
                {
                    state.send(Job::Pin(id.clone(), !pinned));
                }
                if ui.button(crate::i18n::tr("Remove")).clicked() {
                    state.send(Job::Remove(id));
                    state.selected = None;
                }
            });
        }

        self.clipboard_history = state;
    }
}
fn paint_preview(ui: &mut egui::Ui, items: &[[f64; 4]]) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 64.), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 3., base());
    let start = items.iter().map(|v| v[0]).fold(f64::INFINITY, f64::min);
    let end = items.iter().map(|v| v[1]).fold(start + 1., f64::max);
    let lo = items.iter().map(|v| v[2]).fold(-675., f64::min);
    let hi = items.iter().map(|v| v[2]).fold(675., f64::max);
    for item in items {
        let x = rect.left() + 8. + ((item[2] - lo) / (hi - lo)) as f32 * (rect.width() - 16.);
        let y = |t: f64| {
            rect.bottom() - 6. - ((t - start) / (end - start)) as f32 * (rect.height() - 12.)
        };
        let c = if item[3] == 0. { accent() } else { flick() };
        if item[3] == 0. {
            p.circle_filled(egui::pos2(x, y(item[0])), 2., c);
        } else {
            p.line_segment(
                [egui::pos2(x, y(item[0])), egui::pos2(x, y(item[1]))],
                egui::Stroke::new(3_f32, c),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phistudio_core::{ChartDocument, Command, Note, NoteId, tools::Selection};
    #[test]
    fn activation_does_not_edit_or_recapture_the_copy() {
        let root = tempfile::tempdir().unwrap();
        let mut app = Workbench::new(ChartDocument::empty("Copy", 120.).unwrap(), None);
        app.editor
            .execute(Command::InsertNote {
                line: 0,
                value: serde_json::to_value(Note::default()).unwrap(),
            })
            .unwrap();
        let clip = ObjectClipboard::copy(
            app.editor.document(),
            &Selection {
                notes: vec![NoteId { line: 0, index: 0 }],
                events: vec![],
            },
            &[],
        )
        .unwrap();
        app.tools.clipboard = Some(Arc::new(clip));
        let before = app.editor.document().as_json().clone();
        let revision = app.editor.revision();
        app.update_clipboard_history(root.path());
        app.clipboard_history.flush().unwrap();
        app.update_clipboard_history(root.path());
        assert_eq!(app.clipboard_history.history.entries.len(), 1);
        let id = app.clipboard_history.history.entries[0].id.clone();
        app.clipboard_history
            .send(Job::Activate(id, None, app.session));
        app.clipboard_history.flush().unwrap();
        app.update_clipboard_history(root.path());
        app.clipboard_history.flush().unwrap();
        app.update_clipboard_history(root.path());
        assert_eq!(app.clipboard_history.history.entries.len(), 1);
        assert_eq!(app.tools.clipboard.as_ref().unwrap().len(), 1);
        assert_eq!(app.editor.revision(), revision);
        assert_eq!(*app.editor.document().as_json(), before);
    }
}
