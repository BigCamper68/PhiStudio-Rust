//! Semantic feedback wins over widget clicks; decoding never runs in a UI callback.
pub(crate) mod archive;
const MAX_PACK_FILES: usize = Event::ALL.len() * 4;
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
    preview_slot: Option<usize>,
    selected: Event,
    pub warning: Option<String>,
    inactive: bool,
    gesture_committed: bool,
    pub open: bool,
    files: Vec<FileInfo>,
    import_target: Option<String>,
    export: Option<Receiver<Result<phistudio_core::ProjectExport, String>>>,
    delete_confirm: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            pending: BTreeSet::new(),
            preview: None,
            preview_slot: None,
            selected: Event::Click,
            warning: None,
            inactive: false,
            gesture_committed: false,
            open: false,
            files: Vec::new(),
            import_target: None,
            export: None,
            delete_confirm: false,
        }
    }
}
impl State {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub(crate) fn select_for_probe(&mut self, event: Event) {
        self.selected = event;
    }
    pub fn suppress_gesture_commit(&mut self) {
        self.pending.remove(&Event::Edit);
        self.gesture_committed = true;
    }
    pub fn emit(&mut self, event: Event) {
        self.pending.insert(event);
    }
}
pub(crate) fn command_event(command: &Command) -> Event {
    match command {
        Command::AddNote { note, .. } => match note.kind {
            phistudio_core::NoteKind::Tap => Event::PlaceTap,
            phistudio_core::NoteKind::Hold => Event::PlaceHold,
            phistudio_core::NoteKind::Flick => Event::PlaceFlick,
            phistudio_core::NoteKind::Drag => Event::PlaceDrag,
        },
        Command::AddEvent { .. } | Command::InsertEvent { .. } => Event::PlaceEvent,
        Command::InsertNote { .. }
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
        Event::PlaceTap
        | Event::PlaceHold
        | Event::PlaceFlick
        | Event::PlaceDrag
        | Event::PlaceEvent => 76,
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
        self.sounds.gesture_committed = false;
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
            if revision != self.editor.revision() && !self.sounds.gesture_committed {
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
            if selection != current && !self.sounds.gesture_committed {
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
    files: Vec<Vec<String>>,
}
impl Sources {
    fn new(options: &EditorSoundOptions) -> Self {
        let pack = options.pack();
        Self {
            style: pack.style,
            files: Event::ALL
                .iter()
                .map(|e| pack.rule(*e).paths().take(4).cloned().collect())
                .collect(),
        }
    }
}
#[derive(Clone)]
struct FileInfo {
    path: String,
    duration: f64,
    bytes: u64,
    peaks: Vec<f32>,
}
struct Prepared {
    sources: Sources,
    clips: Vec<Vec<Option<ResourceAudioClip>>>,
    bank: Vec<ResourceAudioClip>,
    files: Vec<FileInfo>,
    warning: Option<String>,
}
fn prepare(sources: Sources) -> Prepared {
    use std::collections::BTreeMap;
    let mut warning = Vec::new();
    let mut decoded = BTreeMap::new();
    let mut files = Vec::new();
    let mut memory = 0;
    for path in sources.files.iter().flatten() {
        if decoded.contains_key(path) {
            continue;
        }
        let clip = if decoded.len() >= MAX_PACK_FILES {
            Err(anyhow::anyhow!(
                "A sound pack supports up to {MAX_PACK_FILES} unique files"
            ))
        } else {
            phistudio_prpr::load_interface_sound(std::path::Path::new(path))
        };
        let clip = clip
            .and_then(|clip| {
                memory += std::mem::size_of_val(clip.frames());
                anyhow::ensure!(
                    memory <= 64 * 1024 * 1024,
                    "Decoded sound pack exceeds 64 MiB"
                );
                let peaks = clip
                    .frames()
                    .chunks(clip.frames().len().div_ceil(96).max(1))
                    .map(|chunk| {
                        chunk
                            .iter()
                            .map(|f| f.0.abs().max(f.1.abs()))
                            .fold(0_f32, f32::max)
                            .min(1.)
                    })
                    .collect();
                files.push(FileInfo {
                    path: path.clone(),
                    duration: clip.length(),
                    bytes: std::fs::metadata(path).map_or(0, |m| m.len()),
                    peaks,
                });
                Ok(clip)
            })
            .map_err(|error| {
                warning.push(format!(
                    "{}: {error}",
                    std::path::Path::new(path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                ));
            });
        decoded.insert(path.clone(), clip.ok());
    }
    let clips = sources
        .files
        .iter()
        .map(|paths| {
            paths
                .iter()
                .map(|p| decoded.get(p).cloned().flatten())
                .collect()
        })
        .collect();
    let bank = phistudio_core::EditorSoundTone::ALL
        .iter()
        .map(|tone| phistudio_prpr::interface_tone(sources.style, *tone))
        .collect();
    Prepared {
        sources,
        clips,
        bank,
        files,
        warning: (!warning.is_empty()).then(|| warning.join("\n")),
    }
}
pub(crate) struct Engine {
    output: InterfaceAudio,
    ready: Option<Prepared>,
    loading: Option<Receiver<Prepared>>,
    last: std::collections::BTreeMap<phistudio_core::EditorSoundCategory, Instant>,
    variants: Vec<Option<usize>>,
    random: u64,
    preview: Option<Event>,
    preview_slot: Option<usize>,
    error: Option<String>,
    preview_until: Option<Instant>,
}
impl Engine {
    pub fn new(output: InterfaceAudio) -> Self {
        Self {
            output,
            ready: None,
            loading: None,
            last: Default::default(),
            variants: vec![None; Event::ALL.len()],
            random: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(1, |d| d.as_nanos() as u64)
                .max(1),
            preview: None,
            preview_slot: None,
            error: None,
            preview_until: None,
        }
    }
    pub fn update(&mut self, app: &mut Workbench) {
        let sources = Sources::new(&app.options.editor_sounds);
        if let Some(receiver) = &self.loading {
            match receiver.try_recv() {
                Ok(ready) => {
                    if ready.sources == sources {
                        app.sounds.warning = ready.warning.clone();
                        app.sounds.files = ready.files.clone();
                    }
                    self.ready = Some(ready);
                    self.loading = None;
                    self.variants.fill(None);
                    self.last.clear();
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
        if let Some(event) = app.sounds.preview.take() {
            self.preview = Some(event);
            self.preview_slot = app.sounds.preview_slot.take();
        }
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
        let muted = app.sounds.inactive
            || app.playtest.active()
            || (options.mute_during_playback && app.transport.playing);
        self.output.set_muted(
            muted
                || (!options.enabled
                    && self.preview.is_none()
                    && self
                        .preview_until
                        .is_none_or(|until| Instant::now() >= until)),
        );
        if muted {
            self.preview_until = None;
            self.preview_slot = None;
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
        let pack = options.pack();
        let rule = pack.rule(event);
        if preview.is_none() && (!options.enabled || !rule.enabled) {
            return;
        }
        if preview.is_none()
            && self
                .last
                .get(&event.category())
                .is_some_and(|time| time.elapsed().as_millis() < pack.interval(event) as u128)
        {
            return;
        }
        let variants = &ready.clips[event as usize];
        let slot = self
            .preview_slot
            .take()
            .filter(|i| *i < variants.len())
            .or_else(|| {
                choose_variant(
                    variants.len(),
                    self.variants[event as usize],
                    rule.random,
                    &mut self.random,
                )
            });
        self.variants[event as usize] = slot;
        let clip = slot
            .and_then(|slot| variants[slot].as_ref())
            .unwrap_or(&ready.bank[rule.tone.unwrap_or(event.default_tone()) as usize]);
        if preview.is_some() {
            self.preview_until = Some(
                Instant::now()
                    + std::time::Duration::from_secs_f64(
                        clip.length() / f64::from(pack.pitch * rule.pitch),
                    ),
            );
        }
        self.output.play(
            clip.clone(),
            options.volume * rule.volume,
            pack.pitch * rule.pitch,
        );
        self.last.insert(event.category(), Instant::now());
    }
}
fn choose_variant(
    count: usize,
    previous: Option<usize>,
    random: bool,
    state: &mut u64,
) -> Option<usize> {
    if count == 0 {
        return None;
    }
    if count == 1 {
        return Some(0);
    }
    let previous = previous.filter(|p| *p < count);
    if !random {
        return Some(previous.map_or(0, |p| (p + 1) % count));
    }
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    let choices = count - usize::from(previous.is_some());
    let next = (*state % choices as u64) as usize;
    Some(if previous.is_some_and(|p| next >= p) {
        next + 1
    } else {
        next
    })
}
