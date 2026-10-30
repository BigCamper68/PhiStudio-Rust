//! Parameter snapshots are loaded into forms; applying remains an explicit action.
use crate::{scroll::TouchCombo as _, workbench::Workbench};
use phistudio_core::tool_presets::Library;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
};
enum Job {
    Save(String, Value),
    Remove(u64),
    Recent(Value),
    Flush(mpsc::Sender<()>),
}
#[derive(Default)]
pub(crate) struct PresetsState {
    root: Option<PathBuf>,
    send: Option<mpsc::Sender<Job>>,
    receive: Option<mpsc::Receiver<Result<Library, String>>>,
    library: Option<Library>,
    name: String,
    selected: Option<u64>,
    pending: Vec<Job>,
}
impl PresetsState {
    fn send(&mut self, job: Job) {
        if let Some(send) = &self.send {
            let _ = send.send(job);
        } else {
            self.pending.push(job);
        }
    }
    pub fn update(&mut self, root: &Path) -> Option<String> {
        if self.root.as_deref() != Some(root) {
            let root = root.to_owned();
            self.root = Some(root.clone());
            let (tx, rx) = mpsc::channel();
            let (out, input) = mpsc::channel();
            self.send = Some(tx);
            self.receive = Some(input);
            std::thread::spawn(move || {
                let mut library = match Library::load(&root) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = out.send(Err(e.to_string()));
                        return;
                    }
                };
                let _ = out.send(Ok(library.clone()));
                while let Ok(job) = rx.recv() {
                    let result = match job {
                        Job::Save(n, v) => library.save(&root, &n, v),
                        Job::Remove(id) => library.remove(&root, id),
                        Job::Recent(v) => library.remember(&root, v),
                        Job::Flush(ack) => {
                            let _ = ack.send(());
                            continue;
                        }
                    };
                    let _ = out.send(result.map(|()| library.clone()).map_err(|e| e.to_string()));
                }
            });
            for job in self.pending.drain(..) {
                let _ = self.send.as_ref().unwrap().send(job);
            }
        }
        let mut error = None;
        if let Some(rx) = &self.receive {
            while let Ok(result) = rx.try_recv() {
                match result {
                    Ok(v) => self.library = Some(v),
                    Err(e) => error = Some(e),
                }
            }
        }
        error
    }
    pub fn flush(&self) -> Result<(), String> {
        if let Some(send) = &self.send {
            let (tx, rx) = mpsc::channel();
            if send.send(Job::Flush(tx)).is_err() {
                return Ok(());
            }
            rx.recv_timeout(std::time::Duration::from_secs(10))
                .map_err(|_| "Tool configuration write interrupted".to_owned())?;
        }
        Ok(())
    }
}
impl Workbench {
    pub(crate) fn parameter_snapshot(&self) -> Result<Value, serde_json::Error> {
        let mut tools = serde_json::to_value(&self.tools)?;
        if matches!(
            self.tools.page(),
            crate::toolkit::Page::Configurations | crate::toolkit::Page::Clipboard
        ) {
            tools["page"] = serde_json::to_value(self.tools.last_page)?;
        }
        Ok(
            json!({"version":1,"tools":tools,"story":serde_json::to_value(&self.story_tools)?,"trajectory":self.trajectory.spec,"trajectory_name":self.trajectory.name,"text_composition":self.text_composition.spec}),
        )
    }
    fn load_parameters(&mut self, value: Value) -> Result<(), String> {
        if value["version"] != 1 {
            return Err("Unsupported tool configuration".into());
        }
        let mut tools: crate::toolkit::ToolsState =
            serde_json::from_value(value["tools"].clone()).map_err(|e| e.to_string())?;
        let story = serde_json::from_value(value["story"].clone()).map_err(|e| e.to_string())?;
        let spec: phistudio_core::trajectories::Spec =
            serde_json::from_value(value["trajectory"].clone()).map_err(|e| e.to_string())?;
        if tools.page() == crate::toolkit::Page::Motion {
            spec.compile().map_err(|e| e.to_string())?;
        }
        let text_spec: phistudio_core::text_composition::Spec =
            serde_json::from_value(value.get("text_composition").cloned().unwrap_or_else(|| {
                serde_json::to_value(phistudio_core::text_composition::Spec::default()).unwrap()
            }))
            .map_err(|e| e.to_string())?;
        tools.sanitize();
        tools.restore_context(
            crate::story_grid::active(self)
                && (tools.scope == crate::toolkit::Scope::Events
                    || !self.story_selection().is_empty()),
        );
        tools.selection = std::mem::take(&mut self.tools.selection);
        tools.clipboard = self.tools.clipboard.take();
        tools.show = self.tools.show;
        tools.additive = self.tools.additive;
        self.text_composition.load(text_spec);
        self.tools = tools;
        self.story_tools = story;
        self.trajectory.load_configuration(
            spec,
            value["trajectory_name"]
                .as_str()
                .unwrap_or("Trajectory")
                .to_owned(),
        );
        self.status = "Tool configuration loaded; apply the operation when ready".into();
        Ok(())
    }
    pub(crate) fn remember_trajectory_parameters(
        &mut self,
        spec: &phistudio_core::trajectories::Spec,
        name: &str,
    ) {
        if let Ok(mut v) = self.parameter_snapshot() {
            v["trajectory"] = serde_json::to_value(spec).unwrap();
            v["trajectory_name"] = Value::String(name.into());
            self.presets.remember(v);
        }
    }
    pub(crate) fn remember_tool_parameters(&mut self) {
        if !self.tools.show
            || matches!(
                self.tools.page(),
                crate::toolkit::Page::Selection
                    | crate::toolkit::Page::Configurations
                    | crate::toolkit::Page::Clipboard
            )
        {
            return;
        }
        if let Ok(value) = self.parameter_snapshot() {
            self.presets.remember(value);
        }
    }
    pub(crate) fn tool_presets_ui(&mut self, ui: &mut egui::Ui) {
        let mut state = std::mem::take(&mut self.presets);
        let mut load = None;
        ui.vertical(|ui|{
            ui.strong(crate::i18n::tr("Tool configurations"));
            ui.small(crate::i18n::tr("Loading parameters does not edit the chart. A matching name replaces the saved configuration."));
            ui.horizontal(|ui|{ui.add(egui::TextEdit::singleline(&mut state.name).desired_width(150.).hint_text(crate::i18n::tr("Name")));if ui.add_enabled(state.library.is_some(),egui::Button::new(crate::i18n::tr("Save parameters"))).clicked(){match self.parameter_snapshot(){Ok(v)=>state.send(Job::Save(state.name.clone(),v)),Err(e)=>self.error=Some(e.to_string())}}});
            let entries=state.library.as_ref().map(|l|l.presets.clone()).unwrap_or_default();
            egui::ComboBox::from_id_source("saved-tool-configurations").width(230.).selected_text(entries.iter().find(|p|Some(p.id)==state.selected).map_or(crate::i18n::tr("Choose"),|p|p.name.clone())).touch_show_ui(ui,|ui|{for entry in &entries{if ui.selectable_label(state.selected==Some(entry.id),&entry.name).clicked(){state.selected=Some(entry.id);state.name=entry.name.clone();load=Some(entry.parameters.clone());}}});
            if let Some(id)=state.selected&&ui.button(crate::i18n::tr("Delete configuration")).clicked(){state.send(Job::Remove(id));state.selected=None;}
            egui::ComboBox::from_id_source("recent-tool-configurations").width(230.).selected_text(crate::i18n::tr("Recent parameters")).touch_show_ui(ui,|ui|{if let Some(library)=&state.library{for (i,value) in library.recent.iter().enumerate(){let page=match value["tools"]["page"].as_str().unwrap_or("Selection"){"Transform"=>"Transform / arrange","Values"=>"Batch values","Notes"=>"Note generator","Events"=>"Event tools","Motion"=>"Motion generator","Text"=>"Text from notes / lines","Story"=>"Storyboard tools",_=>"Selection"};if ui.selectable_label(false,format!("{} · {}",i+1,crate::i18n::tr(page))).clicked(){load=Some(value.clone());}}}});
        });
        self.presets = state;
        if let Some(value) = load
            && let Err(error) = self.load_parameters(value)
        {
            self.error = Some(error);
        }
    }
}
impl PresetsState {
    pub(crate) fn remember(&mut self, value: Value) {
        self.send(Job::Recent(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phistudio_core::{ChartDocument, NoteId, tools::Selection};
    #[test]
    fn loading_parameters_keeps_chart_selection_clipboard_and_history() {
        let mut app = Workbench::new(ChartDocument::empty("Parameters", 120.).unwrap(), None);
        app.beat = 5.75;
        app.playing = true;
        app.tools.selection = Selection {
            notes: vec![NoteId { line: 0, index: 0 }],
            events: vec![],
        };
        let before = app.editor.document().as_json().clone();
        let revision = app.editor.revision();
        let mut value = app.parameter_snapshot().unwrap();
        assert!(value["tools"].get("selection").is_none());
        assert!(value["tools"].get("clipboard").is_none());
        value["tools"]["shift_x"] = json!(222.);
        value["tools"]["page"] = json!("Transform");
        value["story"]["count"] = json!(7);
        app.load_parameters(value.clone()).unwrap();
        assert_eq!(app.parameter_snapshot().unwrap(), value);
        assert_eq!(app.selection().notes.len(), 1);
        assert_eq!(*app.editor.document().as_json(), before);
        assert_eq!(app.editor.revision(), revision);
        assert_eq!(app.beat, 5.75);
        assert!(app.playing);
        let saved = app.parameter_snapshot().unwrap();
        value["tools"]["page"] = json!("Motion");
        value["trajectory"]["trim"] = json!([1., 0.]);
        assert!(app.load_parameters(value).is_err());
        assert_eq!(app.parameter_snapshot().unwrap(), saved);
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::*;
    #[test]
    fn library_page_saves_the_last_tool_and_restore_keeps_the_requested_form() {
        let mut app = Workbench::new(
            phistudio_core::ChartDocument::empty("Forms", 120.).unwrap(),
            None,
        );
        app.tools =
            serde_json::from_value(json!({"page":"Configurations","last_page":"Motion"})).unwrap();
        app.tools.show = true;
        let value = app.parameter_snapshot().unwrap();
        assert_eq!(value["tools"]["page"], "Motion");
        app.load_parameters(value).unwrap();
        assert_eq!(app.tools.page(), crate::toolkit::Page::Motion);
        let mut value = app.parameter_snapshot().unwrap();
        value["tools"]["page"] = json!("Story");
        value["tools"]["scope"] = json!("Events");
        let revision = app.editor.revision();
        app.load_parameters(value).unwrap();
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(873., 393.),
                    )),
                    ..Default::default()
                },
                |ctx| app.ui(ctx, false),
            );
        }
        assert_eq!(app.tools.page(), crate::toolkit::Page::Story);
        assert_eq!(revision, app.editor.revision());
    }
}
