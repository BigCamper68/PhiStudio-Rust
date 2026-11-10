//! Semantic feedback wins over widget clicks; decoding never runs in a UI callback.
#[cfg(test)]
mod tests;
mod ui;
use crate::workbench::Workbench;
use phistudio_core::{Command, EditorSoundEvent as Event, EditorSoundOptions, EditorSoundStyle};
use phistudio_prpr::{InterfaceAudio, ResourceAudioClip};
use std::{
    collections::BTreeSet,
    sync::mpsc::{self, Receiver},
    time::Instant,
};

pub(crate) struct State {
    pending: BTreeSet<Event>,
    preview: Option<Event>,
    selected: Event,
    pub warning: Option<String>,
    inactive: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            pending: BTreeSet::new(),
            preview: None,
            selected: Event::Click,
            warning: None,
            inactive: false,
        }
    }
}
impl State {
    pub fn emit(&mut self, event: Event) {
        self.pending.insert(event);
    }
}
pub(crate) fn command_event(command: &Command) -> Event {
    match command {
        Command::AddNote { .. }
        | Command::InsertNote { .. }
        | Command::AddEvent { .. }
        | Command::InsertEvent { .. }
        | Command::AddBpm(_)
        | Command::AddLine { .. }
        | Command::AddLines { .. }
        | Command::DuplicateLine(_) => Event::Place,
        Command::DeleteNote(_)
        | Command::DeleteEvent(_)
        | Command::DeleteBpm(_)
        | Command::DeleteLine(_)
        | Command::EraseStoryboard(_) => Event::Delete,
        Command::EditStoryboard {
            index: None,
            event: Some(_),
            ..
        } => Event::Place,
        Command::EditStoryboard { event: None, .. } => Event::Delete,
        _ => Event::Edit,
    }
}
fn widget_event(event: &egui::output::OutputEvent) -> Option<Event> {
    use egui::{WidgetType as W, output::OutputEvent as O};
    let info = event.widget_info();
    if !info.enabled {
        return None;
    }
    match event {
        O::Clicked(_) | O::DoubleClicked(_) | O::TripleClicked(_) | O::ValueChanged(_) => {
            match info.typ {
                W::Button | W::ImageButton | W::Link => Some(Event::Click),
                W::Checkbox | W::RadioButton => Some(Event::Toggle),
                W::SelectableLabel => Some(Event::Select),
                W::ComboBox | W::CollapsingHeader | W::ColorButton => Some(Event::Expand),
                W::Slider | W::DragValue if matches!(event, O::ValueChanged(_)) => {
                    Some(Event::Value)
                }
                _ => None,
            }
        }
        _ => None,
    }
}
// One sound per completed action, including when a button also edits/clears selection.
fn priority(event: Event) -> u8 {
    match event {
        Event::Error => 100,
        Event::Save => 95,
        Event::Undo | Event::Redo => 90,
        Event::Copy | Event::Paste => 85,
        Event::Delete => 80,
        Event::Place => 75,
        Event::MoveResize => 74,
        Event::Edit => 70,
        Event::Tool => 65,
        Event::Playback => 60,
        Event::Seek => 55,
        Event::Select | Event::Deselect => 50,
        Event::Value => 40,
        Event::Toggle => 30,
        Event::Expand => 20,
        Event::Click => 10,
    }
}
impl Workbench {
    pub(crate) fn sound_ui(&mut self, ctx: &egui::Context, canceled: bool) {
        let revision = self.editor.revision();
        let session = self.session;
        let selection = (
            self.selected,
            self.events.selected,
            self.advanced.story_selected,
            self.tools.selection.notes.len(),
            self.tools.selection.events.len(),
            self.advanced.story_selection.len(),
        );
        let tool = self.timeline.tool;
        let playing = self.transport.playing;
        let beat = self.transport.beat;
        let error = self.error.clone();
        self.ui(ctx, canceled);
        let canceled = canceled
            || ctx.input(|i| {
                !i.raw.has_focus
                    || i.events.iter().any(|e| {
                        matches!(
                            e,
                            egui::Event::Touch {
                                phase: egui::TouchPhase::Cancel,
                                ..
                            }
                        )
                    })
            });
        self.sounds.inactive = ctx.input(|i| !i.raw.has_focus);
        if canceled {
            self.sounds.preview = None;
            self.sounds.pending.clear();
            return;
        }
        if session == self.session {
            if revision != self.editor.revision() {
                self.sounds.emit(Event::Edit);
            }
            let current = (
                self.selected,
                self.events.selected,
                self.advanced.story_selected,
                self.tools.selection.notes.len(),
                self.tools.selection.events.len(),
                self.advanced.story_selection.len(),
            );
            if selection != current {
                self.sounds.emit(
                    if current.0.is_some()
                        || current.1.is_some()
                        || current.2.is_some()
                        || current.3 + current.4 + current.5 > 0
                    {
                        Event::Select
                    } else {
                        Event::Deselect
                    },
                );
            }
            if tool != self.timeline.tool {
                self.sounds.emit(Event::Tool);
            }
            if playing != self.transport.playing {
                self.sounds.emit(Event::Playback);
            }
            if beat != self.transport.beat && !playing && !self.transport.playing {
                self.sounds.emit(Event::Seek);
            }
        }
        if self.error.is_some() && self.error != error {
            self.sounds.emit(Event::Error);
        }
        ctx.output(|o| {
            for event in &o.events {
                if let Some(event) = widget_event(event) {
                    self.sounds.emit(event);
                }
            }
        });
    }
}

#[derive(Clone, PartialEq)]
struct Sources {
    style: EditorSoundStyle,
    files: Vec<Option<String>>,
}
impl Sources {
    fn new(options: &EditorSoundOptions) -> Self {
        Self {
            style: options.style,
            files: Event::ALL
                .iter()
                .map(|e| options.events.get(e).and_then(|r| r.file.clone()))
                .collect(),
        }
    }
}
struct Prepared {
    sources: Sources,
    clips: Vec<ResourceAudioClip>,
    warning: Option<String>,
}
fn prepare(sources: Sources) -> Prepared {
    let mut warning = Vec::new();
    let clips = Event::ALL
        .iter()
        .zip(&sources.files)
        .map(|(event, file)| {
            file.as_ref().and_then(|path| {
                match phistudio_prpr::load_interface_sound(std::path::Path::new(path)) {
                    Ok(clip) => Some(clip),
                    Err(error) => {
                        warning.push(format!("{}: {error}", event.label()));
                        None
                    }
                }
            })
        })
        .collect::<Vec<_>>();
    // Tone overrides use the same small bank; each custom file is decoded only once per reload.
    let bank = phistudio_core::EditorSoundTone::ALL
        .iter()
        .map(|tone| phistudio_prpr::interface_tone(sources.style, *tone))
        .collect::<Vec<_>>();
    let clips = clips
        .into_iter()
        .zip(Event::ALL)
        .map(|(clip, event)| clip.unwrap_or_else(|| bank[event.default_tone() as usize].clone()))
        .chain(bank.iter().cloned())
        .collect();
    Prepared {
        sources,
        clips,
        warning: (!warning.is_empty()).then(|| warning.join("\n")),
    }
}
pub(crate) struct Engine {
    output: InterfaceAudio,
    ready: Option<Prepared>,
    loading: Option<Receiver<Prepared>>,
    last: Option<(Event, Instant)>,
    preview: Option<Event>,
    error: Option<String>,
}
impl Engine {
    pub fn new(output: InterfaceAudio) -> Self {
        Self {
            output,
            ready: None,
            loading: None,
            last: None,
            preview: None,
            error: None,
        }
    }
    pub fn update(&mut self, app: &mut Workbench) {
        let sources = Sources::new(&app.options.editor_sounds);
        if let Some(receiver) = &self.loading {
            match receiver.try_recv() {
                Ok(ready) => {
                    if ready.sources == sources {
                        app.sounds.warning = ready.warning.clone();
                    }
                    self.ready = Some(ready);
                    self.loading = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.loading = None;
                    app.sounds.warning = Some("Editor sound loading interrupted".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.loading.is_none() && self.ready.as_ref().is_none_or(|r| r.sources != sources) {
            let (sender, receiver) = mpsc::channel();
            self.loading = Some(receiver);
            let sources = sources.clone();
            std::thread::spawn(move || {
                let _ = sender.send(prepare(sources));
            });
        }
        self.preview = app.sounds.preview.take().or(self.preview);
        if app.error.is_some() && self.error != app.error {
            app.sounds.emit(Event::Error);
        }
        self.error = app.error.clone();
        let event = app
            .sounds
            .pending
            .iter()
            .copied()
            .max_by_key(|e| priority(*e));
        app.sounds.pending.clear();
        let options = &app.options.editor_sounds;
        let muted =
            app.playtest.active() || (options.mute_during_playback && app.transport.playing);
        self.output
            .set_muted(muted || (!options.enabled && self.preview.is_none()));
        if muted {
            self.preview = None;
            return;
        }
        let Some(ready) = self.ready.as_ref().filter(|r| r.sources == sources) else {
            return;
        };
        let preview = self.preview.take();
        let Some(event) = preview.or(event) else {
            return;
        };
        let rule = options.rule(event);
        if preview.is_none() && (!options.enabled || !rule.enabled) {
            return;
        }
        let cooldown = options
            .cooldown_ms
            .max(if event == Event::Value || event == Event::Seek {
                90
            } else {
                20
            });
        if preview.is_none()
            && self.last.is_some_and(|(last, time)| {
                last == event && time.elapsed().as_millis() < cooldown as u128
            })
        {
            return;
        }
        let clip = if rule.file.is_some() {
            &ready.clips[event as usize]
        } else {
            &ready.clips[Event::ALL.len() + rule.tone.unwrap_or(event.default_tone()) as usize]
        };
        self.output.play(
            clip.clone(),
            options.volume * rule.volume,
            options.pitch * rule.pitch,
        );
        self.last = Some((event, Instant::now()));
    }
}
