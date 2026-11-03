use super::*;

enum Domain {
    Grid { grid: Grid, area: Rect },
    Fixed(Region),
}
impl Domain {
    fn owns(&self, region: &Region) -> bool {
        match self {
            Self::Grid { .. } => region.surface.instance.starts_with("grid-"),
            Self::Fixed(current) => {
                current.surface.kind == region.surface.kind
                    && current.surface.instance == region.surface.instance
                    && current.surface.id == region.surface.id
            }
        }
    }
    fn at(&self, app: &mut Workbench, pos: Pos2) -> Option<Region> {
        match self {
            Self::Grid { grid, area } if area.contains(pos) => app.plugin_grid_region(*grid, pos),
            Self::Fixed(region) if region.rect.contains(pos) => Some(region.clone()),
            _ => None,
        }
    }
}
impl Workbench {
    pub(crate) fn plugin_grid_input(
        &mut self,
        events: &[Event],
        grid: Grid,
        area: Rect,
        eligible: &impl Fn(Pos2) -> bool,
        canceled: bool,
    ) -> Option<Vec<Event>> {
        self.plugins.input.tool.as_ref()?;
        Some(self.route_plugin_input(events, Domain::Grid { grid, area }, eligible, canceled))
    }
    pub(super) fn plugin_region_input(
        &mut self,
        events: &[Event],
        region: Region,
        eligible: &impl Fn(Pos2) -> bool,
    ) -> Vec<Event> {
        if self.plugins.input.tool.is_some() {
            self.route_plugin_input(
                events,
                Domain::Fixed(region),
                eligible,
                self.plugins.input.canceled,
            )
        } else {
            events.to_vec()
        }
    }
    fn route_plugin_input(
        &mut self,
        events: &[Event],
        domain: Domain,
        eligible: &impl Fn(Pos2) -> bool,
        canceled: bool,
    ) -> Vec<Event> {
        let canceled = canceled || self.plugins.input.canceled;
        if canceled {
            self.cancel_plugin_input();
        }
        let frozen = self.plugins.input.capture.as_ref().and_then(|c| {
            domain
                .owns(&c.region)
                .then_some((c.region.clone(), c.origin))
        });
        if let Some((frozen, origin)) = frozen {
            let current = domain.at(self, origin);
            if current.is_none_or(|r| {
                r.rect != frozen.rect || r.surface != frozen.surface || r.line != frozen.line
            }) {
                self.cancel_plugin_input();
            }
        }
        let main = matches!(domain, Domain::Grid { .. });
        let touch_frame = events.iter().any(|e| matches!(e, Event::Touch { .. }));
        let mut suppress_touch_pointer = self
            .plugins
            .input
            .capture
            .as_ref()
            .is_some_and(|c| domain.owns(&c.region) && c.pointer.kind == PointerKind::Touch);
        let mut output = Vec::with_capacity(events.len());
        for event in events {
            let owns = self
                .plugins
                .input
                .capture
                .as_ref()
                .is_some_and(|c| domain.owns(&c.region));
            let mut consume = false;
            match *event {
                Event::Touch {
                    device_id,
                    id,
                    phase,
                    pos,
                    force,
                } => {
                    let key = (device_id.0, id.0);
                    if phase == egui::TouchPhase::Start {
                        self.plugins.input.touches.insert(key);
                        if self.plugins.input.touches.len() > 1 {
                            self.plugins.input.blocked = true;
                            self.cancel_plugin_input();
                        }
                        if eligible(pos)
                            && let Some(region) = domain.at(self, pos)
                            && self.plugins.input.tool.as_ref().is_some_and(|t| {
                                t.surfaces.contains(&region.surface.kind)
                                    && t.buttons.contains(&InputButton::Primary)
                            })
                        {
                            suppress_touch_pointer = true;
                            if !canceled && !self.plugins.input.blocked {
                                self.begin_plugin_input(
                                    region,
                                    pos,
                                    InputPointer {
                                        kind: PointerKind::Touch,
                                        device: device_id.0,
                                        id: id.0,
                                        button: InputButton::Primary,
                                    },
                                    self.plugins.input.modifiers,
                                    pressure(force),
                                );
                            }
                        }
                    } else {
                        let matches = self.plugins.input.capture.as_ref().is_some_and(|c| {
                            domain.owns(&c.region)
                                && c.pointer.kind == PointerKind::Touch
                                && c.pointer.id == id.0
                                && c.pointer.device == device_id.0
                                && !c.ended
                        });
                        if matches {
                            suppress_touch_pointer = true;
                            match phase {
                                egui::TouchPhase::Move => {
                                    self.move_plugin_input(
                                        pos,
                                        self.plugins.input.modifiers,
                                        pressure(force),
                                        InputPhase::Update,
                                    );
                                }
                                egui::TouchPhase::End => {
                                    self.move_plugin_input(
                                        pos,
                                        self.plugins.input.modifiers,
                                        pressure(force),
                                        InputPhase::End,
                                    );
                                }
                                egui::TouchPhase::Cancel => self.cancel_plugin_input(),
                                _ => {}
                            }
                        }
                    }
                    if matches!(phase, egui::TouchPhase::End | egui::TouchPhase::Cancel) {
                        self.plugins.input.touches.remove(&key);
                        if self.plugins.input.touches.is_empty() {
                            self.plugins.input.blocked = false;
                        }
                    }
                    // The native grid still receives raw touches for its pinch
                    // navigator; synthesized clicks on the owned surface do not.
                    consume =
                        !main && (owns || suppress_touch_pointer) && !self.plugins.input.navigation;
                }
                Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                } => {
                    self.plugins.input.modifiers = modifiers;
                    if touch_frame && suppress_touch_pointer && !self.plugins.input.navigation {
                        consume = true;
                    } else if self.plugins.input.navigation {
                        if !pressed {
                            self.plugins.input.navigation = false;
                        }
                    } else if !canceled {
                        if pressed
                            && eligible(pos)
                            && let Some(region) = domain.at(self, pos)
                            && self.plugins.input.tool.as_ref().is_some_and(|t| {
                                t.surfaces.contains(&region.surface.kind)
                                    && t.buttons.contains(&button_kind(button))
                            })
                        {
                            consume = true;
                            self.begin_plugin_input(
                                region,
                                pos,
                                InputPointer {
                                    kind: PointerKind::Mouse,
                                    device: 0,
                                    id: 0,
                                    button: button_kind(button),
                                },
                                modifiers,
                                None,
                            );
                        } else if !pressed
                            && owns
                            && self.plugins.input.capture.as_ref().is_some_and(|c| {
                                c.pointer.kind == PointerKind::Mouse
                                    && c.pointer.button == button_kind(button)
                                    && !c.ended
                            })
                        {
                            consume = self.move_plugin_input(pos, modifiers, None, InputPhase::End);
                        }
                    }
                }
                Event::PointerMoved(pos) => {
                    if touch_frame && suppress_touch_pointer && !self.plugins.input.navigation {
                        consume = true;
                    } else if owns
                        && self
                            .plugins
                            .input
                            .capture
                            .as_ref()
                            .is_some_and(|c| c.pointer.kind == PointerKind::Mouse && !c.ended)
                    {
                        consume = self.move_plugin_input(
                            pos,
                            self.plugins.input.modifiers,
                            None,
                            InputPhase::Update,
                        );
                    }
                }
                Event::PointerGone => {
                    if owns
                        && self
                            .plugins
                            .input
                            .capture
                            .as_ref()
                            .is_some_and(|c| !c.ended)
                    {
                        self.cancel_plugin_input();
                        consume = true;
                    }
                }
                Event::Scroll(_) | Event::Zoom(_) if owns => self.cancel_plugin_input(),
                Event::Key { modifiers, .. } => self.plugins.input.modifiers = modifiers,
                _ => {}
            }
            if canceled && matches!(event, Event::PointerButton { .. } | Event::PointerMoved(_)) {
                consume = true;
            }
            if !consume {
                output.push(event.clone());
            }
        }
        self.pump_plugin_input();
        output
    }
    fn move_plugin_input(
        &mut self,
        pos: Pos2,
        modifiers: egui::Modifiers,
        pressure: Option<f32>,
        phase: InputPhase,
    ) -> bool {
        let Some(c) = &mut self.plugins.input.capture else {
            return false;
        };
        if c.ended {
            return true;
        }
        c.moved |= pos.distance(c.origin) > 8.;
        c.last = pos;
        let tap = self
            .plugins
            .input
            .tool
            .as_ref()
            .is_some_and(|t| t.gesture == ToolGesture::Tap);
        if tap && c.moved {
            let navigation = (c.region.grid, c.origin, c.region.surface.kind, c.pointer);
            self.cancel_plugin_input();
            let (grid, origin, kind, pointer) = navigation;
            if let Some(grid) = grid {
                let intent = if matches!(
                    kind,
                    InputSurfaceKind::Events | InputSurfaceKind::Storyboard
                ) {
                    crate::grid_input::Intent::SelectEvent(None, false)
                } else if kind == InputSurfaceKind::ShaderEffects {
                    crate::grid_input::Intent::SelectShader(None)
                } else {
                    crate::grid_input::Intent::Pan
                };
                crate::grid_input::begin_navigation(self, grid, origin, intent);
                self.plugins.input.navigation = true;
                return false;
            }
            if kind == InputSurfaceKind::Preview && self.preview.fullscreen {
                self.preview.drag = Some(crate::preview_ui::PreviewDrag {
                    origin,
                    last: origin,
                    vertical: None,
                    moved: false,
                    exit: false,
                });
                if pointer.kind == PointerKind::Touch {
                    self.preview.touches.insert(pointer.id);
                }
                self.plugins.input.navigation = true;
                return false;
            }
            return true;
        }
        let position = c.region.point(pos, pressure);
        c.push(PointerInput {
            phase,
            position,
            modifiers: mods(modifiers),
            moved: c.moved,
        });
        c.ended = phase == InputPhase::End;
        true
    }
}
fn pressure(force: f32) -> Option<f32> {
    force
        .is_finite()
        .then_some(force.clamp(0., 1.))
        .filter(|v| *v > 0.)
}
fn button_kind(button: egui::PointerButton) -> InputButton {
    match button {
        egui::PointerButton::Primary => InputButton::Primary,
        egui::PointerButton::Secondary => InputButton::Secondary,
        egui::PointerButton::Middle => InputButton::Middle,
        egui::PointerButton::Extra1 => InputButton::Extra1,
        egui::PointerButton::Extra2 => InputButton::Extra2,
    }
}
