//! Selected extension tools own versioned gestures; built-in tools keep their router.
mod geometry;
mod routing;
mod surfaces;
use super::*;
use crate::grid_input::Grid;
use egui::{Event, Pos2, Rect};
use phistudio_plugins::Interaction;
use phistudio_sdk::{
    InputBatch, InputButton, InputModifiers, InputPhase, InputPoint, InputPointer, InputSurface,
    InputSurfaceKind, PointerInput, PointerKind, ToolGesture,
};

#[derive(Clone)]
struct Tool {
    origin: Origin,
    id: String,
    surfaces: Vec<InputSurfaceKind>,
    buttons: Vec<InputButton>,
    gesture: ToolGesture,
}
#[derive(Clone)]
struct Region {
    rect: Rect,
    surface: InputSurface,
    grid: Option<Grid>,
    line: usize,
}
struct Capture {
    scope: Interaction,
    region: Region,
    context: EditorContext,
    pointer: InputPointer,
    origin: Pos2,
    last: Pos2,
    start: InputPoint,
    events: Vec<PointerInput>,
    ended: bool,
    moved: bool,
    pending: Option<u64>,
    screen: Option<Rect>,
    grid_area: Option<Rect>,
}
#[derive(Default)]
pub(super) struct Input {
    tool: Option<Tool>,
    capture: Option<Capture>,
    pub(super) jobs: BTreeMap<u64, Interaction>,
    touches: BTreeSet<(u64, u64)>,
    blocked: bool,
    canceled: bool,
    navigation: bool,
    screen: Option<Rect>,
    modifiers: egui::Modifiers,
}
impl Input {
    pub(super) fn complete(&mut self, job: u64) -> Option<Interaction> {
        let scope = self.jobs.remove(&job)?;
        if let Some(c) = &mut self.capture
            && c.pending == Some(job)
        {
            c.pending = None;
        }
        Some(scope)
    }
}
impl Workbench {
    pub(in crate::plugins) fn plugin_track_tool_active(&self) -> bool {
        self.plugins
            .input
            .tool
            .as_ref()
            .is_some_and(|tool| tool.surfaces.contains(&InputSurfaceKind::Track))
    }
    pub(crate) fn plugin_tool_active(&self) -> bool {
        self.plugins.input.tool.is_some()
    }
    pub(crate) fn stop_plugin_tool(&mut self) {
        self.cancel_plugin_input();
        self.plugins.input.tool = None;
    }
    pub(super) fn select_plugin_tool(&mut self, origin: &Origin, id: Option<&str>) {
        if id.is_none() {
            if self
                .plugins
                .input
                .tool
                .as_ref()
                .is_some_and(|t| t.origin == *origin)
            {
                self.stop_plugin_tool();
            }
            return;
        }
        let Some(id) = id else {
            return;
        };
        let Some((surfaces, buttons, gesture)) = self
            .plugins
            .registry
            .plugins
            .iter()
            .filter(|p| p.origin.as_ref() == Some(origin))
            .flat_map(|p| &p.contributions)
            .find_map(|c| match c {
                Contribution::Tool {
                    id: registered,
                    surfaces,
                    buttons,
                    gesture,
                    ..
                } if registered == id => Some((surfaces.clone(), buttons.clone(), *gesture)),
                _ => None,
            })
        else {
            return;
        };
        if self
            .plugins
            .runtime
            .as_ref()
            .is_none_or(|r| !r.is_current(origin))
        {
            return;
        }
        self.stop_plugin_tool();
        self.timeline.cancel_gesture();
        self.preview.drag = None;
        self.plugins.input.tool = Some(Tool {
            origin: origin.clone(),
            id: id.into(),
            surfaces,
            buttons,
            gesture,
        });
    }
    pub(crate) fn plugin_tools_toolbar(&mut self, ui: &mut egui::Ui) {
        let tools: Vec<_> = self
            .plugins
            .registry
            .plugins
            .iter()
            .flat_map(|p| {
                p.contributions.iter().filter_map(move |c| match c {
                    Contribution::Tool { id, title, .. } => {
                        Some((p.origin.clone()?, id.clone(), title.clone()))
                    }
                    _ => None,
                })
            })
            .collect();
        if tools.is_empty() {
            return;
        }
        ui.separator();
        for (origin, id, title) in tools {
            if self
                .plugins
                .runtime
                .as_ref()
                .is_none_or(|r| !r.is_current(&origin))
            {
                continue;
            }
            let selected = self
                .plugins
                .input
                .tool
                .as_ref()
                .is_some_and(|t| t.origin == origin && t.id == id);
            if ui
                .selectable_label(selected, crate::i18n::tr(&title))
                .clicked()
            {
                if selected {
                    self.stop_plugin_tool();
                } else {
                    self.select_plugin_tool(&origin, Some(&id));
                }
            }
        }
    }
    pub(crate) fn cancel_plugin_input(&mut self) {
        if self.plugins.input.navigation {
            self.timeline.cancel_gesture();
            self.preview.drag = None;
            self.preview.touches.clear();
            self.plugins.input.navigation = false;
        }
        let Some(mut capture) = self.plugins.input.capture.take() else {
            return;
        };
        capture.scope.cancel();
        if self
            .plugins
            .runtime
            .as_ref()
            .is_some_and(|r| r.is_current(capture.scope.origin()))
        {
            let position = capture.region.point(capture.last, None);
            capture.events = vec![PointerInput {
                phase: InputPhase::Cancel,
                position,
                modifiers: InputModifiers::default(),
                moved: capture.moved,
            }];
            let batch = capture.batch();
            if let Ok(id) = self.plugins.submit(
                Action::Input {
                    interaction: capture.scope.clone(),
                    input: batch,
                    context: capture.context.clone(),
                },
                Some(capture.context.version),
            ) {
                self.plugins.background.insert(id);
                self.plugins.input.jobs.insert(id, capture.scope);
            }
        }
    }
    pub(crate) fn plugin_input_preflight(&mut self, ctx: &egui::Context, canceled: bool) {
        let canceled = canceled || !ctx.input(|i| i.raw.has_focus);
        self.plugins.input.canceled = canceled;
        self.plugins.input.screen = Some(ctx.screen_rect());
        if canceled {
            self.cancel_plugin_input();
        }
        ctx.input(|i| {
            self.plugins.input.modifiers = i.modifiers;
            for event in &i.events {
                if let Event::Touch {
                    device_id,
                    id,
                    phase,
                    ..
                } = event
                {
                    if *phase == egui::TouchPhase::Start {
                        self.plugins.input.touches.insert((device_id.0, id.0));
                    }
                    if matches!(phase, egui::TouchPhase::End | egui::TouchPhase::Cancel) {
                        self.plugins.input.touches.remove(&(device_id.0, id.0));
                    }
                }
            }
        });
        if self.plugins.input.touches.is_empty() {
            self.plugins.input.blocked = false;
        }
        let Some(c) = &self.plugins.input.capture else {
            return;
        };
        let invalid = canceled
            || c.context.version != self.plugin_version()
            || c.context.line != self.line
            || self.modal_open_except_editor_action()
            || (self.plugin_editor_action_modal()
                && !self.plugin_editor_action_holds_interaction(&c.scope))
            || self.playtest.active()
            || self.show_projects
            || !self.plugin_input_layout_current(c)
            || !c.scope.is_active()
            || self
                .plugins
                .runtime
                .as_ref()
                .is_none_or(|r| !r.is_current(c.scope.origin()))
            || c.screen.is_some_and(|screen| screen != ctx.screen_rect())
            || ctx.input(|i| {
                i.events.iter().any(|event| match event {
                    Event::Touch {
                        phase: egui::TouchPhase::Cancel,
                        ..
                    } => true,
                    Event::Touch {
                        phase: egui::TouchPhase::Start,
                        id,
                        device_id,
                        ..
                    } => {
                        c.pointer.kind == PointerKind::Touch
                            && (id.0 != c.pointer.id || device_id.0 != c.pointer.device)
                    }
                    Event::Key {
                        key: egui::Key::Escape,
                        pressed: true,
                        ..
                    } => true,
                    Event::PointerGone => {
                        !c.ended
                            && !i.events.iter().any(|e| {
                                matches!(
                                    e,
                                    Event::PointerButton { pressed: false, .. }
                                        | Event::Touch {
                                            phase: egui::TouchPhase::End,
                                            ..
                                        }
                                )
                            })
                    }
                    _ => false,
                })
            });
        if invalid {
            self.cancel_plugin_input();
        }
    }
    pub(super) fn maintain_plugin_input(&mut self) {
        let invalid_tool = self.plugins.input.tool.as_ref().is_some_and(|t| {
            self.plugins
                .runtime
                .as_ref()
                .is_none_or(|r| !r.is_current(&t.origin))
        });
        if invalid_tool {
            self.stop_plugin_tool();
        }
        let invalid = self.plugins.input.capture.as_ref().is_some_and(|c| {
            !c.scope.is_active()
                || (c.context.version != self.plugin_version() && !c.scope.committed())
        });
        if invalid {
            self.cancel_plugin_input();
        }
        let finished = self.plugins.input.capture.as_ref().is_some_and(|c| {
            c.ended
                && c.events.is_empty()
                && c.pending.is_none()
                && !self.plugins.input.jobs.values().any(|i| i == &c.scope)
                && !self.plugin_editor_action_holds_interaction(&c.scope)
                && !self
                    .plugins
                    .calls
                    .iter()
                    .any(|call| call.interaction() == Some(&c.scope))
        });
        if finished {
            self.plugins.input.capture = None;
        }
        self.pump_plugin_input();
    }
    fn pump_plugin_input(&mut self) {
        let Some(c) = &self.plugins.input.capture else {
            return;
        };
        if c.pending.is_some() || c.events.is_empty() || !c.scope.is_active() {
            return;
        }
        let batch = c.batch();
        let background = batch.phase() != Some(InputPhase::End);
        let scope = c.scope.clone();
        let context = c.context.clone();
        match self.plugins.submit(
            Action::Input {
                interaction: scope.clone(),
                input: batch,
                context,
            },
            Some(scope.version()),
        ) {
            Ok(id) => {
                if background {
                    self.plugins.background.insert(id);
                }
                self.plugins.input.jobs.insert(id, scope);
                if let Some(c) = &mut self.plugins.input.capture {
                    c.pending = Some(id);
                    c.events.clear();
                }
            }
            Err(error) => {
                self.error = Some(error);
                self.cancel_plugin_input();
            }
        }
    }
    fn begin_plugin_input(
        &mut self,
        region: Region,
        pos: Pos2,
        pointer: InputPointer,
        modifiers: egui::Modifiers,
        pressure: Option<f32>,
    ) {
        if self.plugins.input.capture.is_some() || self.plugins.busy() || self.modal_open() {
            return;
        }
        let Some(tool) = self.plugins.input.tool.clone() else {
            return;
        };
        if !tool.surfaces.contains(&region.surface.kind) || !tool.buttons.contains(&pointer.button)
        {
            return;
        }
        let mut context = self.plugin_metadata_context();
        context.line = region.line;
        let Some(runtime) = &mut self.plugins.runtime else {
            return;
        };
        match runtime.begin_interaction(
            &tool.origin,
            &tool.id,
            context.version,
            region.surface.clone(),
        ) {
            Ok(scope) => {
                self.timeline.cancel_gesture();
                self.transport.playing = false;
                let start = region.point(pos, pressure);
                let grid_area = region
                    .surface
                    .instance
                    .starts_with("grid-")
                    .then_some(self.timeline.area);
                self.plugins.input.capture = Some(Capture {
                    scope,
                    region,
                    context,
                    pointer,
                    origin: pos,
                    last: pos,
                    start: start.clone(),
                    events: vec![PointerInput {
                        phase: InputPhase::Begin,
                        position: start,
                        modifiers: mods(modifiers),
                        moved: false,
                    }],
                    ended: false,
                    moved: false,
                    pending: None,
                    screen: self.plugins.input.screen,
                    grid_area,
                });
            }
            Err(error) => self.error = Some(error),
        }
    }
}
impl Capture {
    fn batch(&self) -> InputBatch {
        InputBatch {
            gesture: self.scope.id(),
            surface: self.region.surface.clone(),
            pointer: self.pointer,
            start: self.start.clone(),
            events: self.events.clone(),
        }
    }
    fn push(&mut self, event: PointerInput) {
        if self
            .events
            .last()
            .is_some_and(|e| e.phase == InputPhase::Update)
        {
            self.events.pop();
        }
        self.events.push(event);
    }
}
fn mods(m: egui::Modifiers) -> InputModifiers {
    InputModifiers {
        alt: m.alt,
        ctrl: m.ctrl,
        shift: m.shift,
        command: m.command,
    }
}

#[cfg(test)]
mod tests;
