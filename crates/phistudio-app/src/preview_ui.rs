use crate::{theme::*, workbench::Workbench};
use egui::{self, Rect, Sense, Vec2, pos2, vec2};
pub(crate) struct PreviewState {
    pub rect: Rect,
    pub texture: Option<egui::TextureId>,
    pub size: [u32; 2],
    pub fullscreen: bool,
    pub floating: bool,
    pub exit_bounds: [f32; 4],
    pub touches: std::collections::BTreeSet<u64>,
    pub blocked: bool,
    pub pick: Option<[f32; 2]>,
    pub drag: Option<PreviewDrag>,
}
impl Default for PreviewState {
    fn default() -> Self {
        Self {
            rect: Rect::NOTHING,
            texture: None,
            size: [1280, 720],
            fullscreen: false,
            floating: false,
            exit_bounds: [0., 0., 0.09, 0.15],
            touches: Default::default(),
            blocked: false,
            pick: None,
            drag: None,
        }
    }
}
pub(crate) struct PreviewDrag {
    pub origin: egui::Pos2,
    pub last: egui::Pos2,
    pub vertical: Option<bool>,
    pub moved: bool,
    pub exit: bool,
}
impl Workbench {
    pub(crate) fn open_fullscreen_preview(&mut self) {
        self.preview.drag = None;
        self.preview.touches.clear();
        self.preview.blocked = false;
        self.preview.fullscreen = true;
        self.drag = None;
        self.grid_input.cancel();
        self.timeline_scrub = false;
    }
    pub(crate) fn paint_grid_preview(&mut self, painter: &egui::Painter, area: Rect) {
        if !area.is_positive() {
            self.preview.rect = Rect::NOTHING;
            return;
        }
        if self.render_error.is_some() {
            return;
        }
        if let Some(texture) = self.preview.texture {
            let aspect = self.preview_aspect();
            let size = if area.width() / area.height() > aspect {
                vec2(area.height() * aspect, area.height())
            } else {
                vec2(area.width(), area.width() / aspect)
            };
            let rect = Rect::from_center_size(area.center(), size);
            painter.with_clip_rect(area).image(
                texture,
                rect,
                self.preview_uv(),
                egui::Color32::from_white_alpha(
                    (self.options.preview_grid_brightness * 255.).round() as u8,
                ),
            );
            self.preview.rect = rect;
        }
    }
    pub(crate) fn preview_aspect(&self) -> f32 {
        let aspect = self
            .options
            .preview_aspect
            .ratio()
            .or(self.project_options.aspect_ratio)
            .unwrap_or(16. / 9.);
        if self.options.preview_aspect.ratio().is_none() && !self.project_options.force_aspect_ratio
        {
            aspect.min(16. / 9.)
        } else {
            aspect
        }
    }
    pub(crate) fn preview_uv(&self) -> Rect {
        let aspect = self.preview_aspect();
        let [width, height] = self.preview.size.map(|v| v as f32);
        let (w, h) = if aspect > width / height {
            (width, width / aspect)
        } else {
            (height * aspect, height)
        };
        let x = ((width - w) / 2.).round();
        let y = ((height - h) / 2.).round();
        Rect::from_min_max(
            pos2(x / width, 1. - y / height),
            pos2((x + w.floor()) / width, 1. - (y + h.floor()) / height),
        )
    }
    pub(crate) fn paint_preview(&mut self, ui: &mut egui::Ui, rect: Rect) {
        self.preview.rect = rect;
        if self.render_error.is_none()
            && let Some(texture) = self.preview.texture
        {
            ui.painter()
                .image(texture, rect, self.preview_uv(), egui::Color32::WHITE);
        } else {
            ui.painter().rect_filled(rect, 0., base());
        }
    }
    pub(crate) fn sidebar_preview(&mut self, ui: &mut egui::Ui) {
        if self.options.preview_behind_grid {
            if ui.button(crate::i18n::tr("Fullscreen Preview ↗")).clicked() {
                self.open_fullscreen_preview();
            }
            ui.separator();
        } else if !self.preview.floating {
            let height = ui.spacing().interact_size.y;
            ui.spacing_mut().interact_size.y = 18.;
            ui.horizontal(|ui| {
                ui.small(egui::RichText::new(crate::i18n::tr("PREVIEW")).color(muted()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("↗")
                        .on_hover_text(crate::i18n::tr("Open in a floating window"))
                        .clicked()
                    {
                        self.preview.floating = true;
                    }
                });
            });
            ui.spacing_mut().interact_size.y = height;
            self.preview_panel_contents(ui, true);
            ui.separator();
        }
    }
    pub(crate) fn preview_panel_impl(&mut self, ui: &mut egui::Ui) {
        self.preview_panel_contents(ui, false);
    }
    fn preview_panel_contents(&mut self, ui: &mut egui::Ui, fill_width: bool) {
        let aspect = self.preview_aspect();
        let width = ui.available_width().max(1.);
        let height = if fill_width {
            width / aspect
        } else {
            (width / aspect).min((ui.ctx().screen_rect().height() * 0.4).clamp(90., 240.))
        };
        let size = vec2((height * aspect).min(width), height);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        self.paint_preview(ui, rect);
        ui.painter()
            .rect_stroke(rect, 0., egui::Stroke::new(1_f32, border()));
        if response.clicked() && !self.modal_open() {
            self.open_fullscreen_preview();
        }
    }
    pub(crate) fn close_fullscreen_preview(&mut self) {
        self.preview.drag = None;
        self.preview.touches.clear();
        self.preview.blocked = false;
        self.preview.fullscreen = false;
        self.drag = None;
        self.grid_input.cancel();
        self.timeline_scrub = false;
    }
    pub(crate) fn fullscreen_exit_rect(&self, rect: Rect) -> Rect {
        let [l, t, r, b] = self.preview.exit_bounds;
        let bounds = Rect::from_min_max(
            pos2(
                rect.left() + l * rect.width(),
                rect.top() + t * rect.height(),
            ),
            pos2(
                rect.left() + r * rect.width(),
                rect.top() + b * rect.height(),
            ),
        );
        let size = bounds.size().max(Vec2::splat(44.)).min(rect.size());
        let center = pos2(
            bounds
                .center()
                .x
                .clamp(rect.left() + size.x / 2., rect.right() - size.x / 2.),
            bounds
                .center()
                .y
                .clamp(rect.top() + size.y / 2., rect.bottom() - size.y / 2.),
        );
        Rect::from_center_size(center, size)
    }
    pub(crate) fn fullscreen_preview(&mut self, ctx: &egui::Context) {
        self.grid_area = Rect::NOTHING;
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                self.close_fullscreen_preview();
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Space) {
                self.playing = !self.playing;
            }
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::BLACK))
            .show(ctx, |ui| {
                let area = ui.max_rect();
                let aspect = self.preview_aspect();
                let size = if area.width() / area.height() > aspect {
                    vec2(area.height() * aspect, area.height())
                } else {
                    vec2(area.width(), area.width() / aspect)
                };
                let rect = Rect::from_center_size(area.center(), size);
                self.paint_preview(ui, rect);
                let exit = self.fullscreen_exit_rect(rect);
                ui.interact(exit, egui::Id::new("fullscreen-HUD-return"), Sense::hover());
                let events = ui.input(|i| i.events.clone());
                self.fullscreen_input(&events, rect, exit);
            });
    }
    fn preview_move(&mut self, pos: egui::Pos2) {
        if let Some(d) = &mut self.preview.drag {
            if d.exit {
                return;
            }
            let delta = pos - d.origin;
            if d.vertical.is_none() && delta.length() > 8. {
                d.moved = true;
                d.vertical = Some(delta.y.abs() >= delta.x.abs());
            }
            if d.vertical == Some(true) {
                // Lock the gesture once; consume clamp overscroll so reversing at
                // beat zero immediately moves the cursor again.
                self.beat = (self.beat
                    + ((pos.y - d.last.y) * self.options.scroll_speed / self.zoom) as f64)
                    .max(0.);
            }
            if d.vertical.is_some() {
                d.last = pos;
            }
        }
    }
    fn preview_press(&mut self, pos: egui::Pos2, rect: Rect, exit: Rect) {
        if rect.contains(pos) {
            self.preview.drag = Some(crate::preview_ui::PreviewDrag {
                origin: pos,
                last: pos,
                vertical: None,
                moved: false,
                exit: exit.contains(pos),
            });
        }
    }
    fn preview_release(&mut self, pos: egui::Pos2, rect: Rect) {
        self.preview_move(pos);
        if let Some(d) = self.preview.drag.take()
            && !d.moved
        {
            if d.exit {
                if self.fullscreen_exit_rect(rect).contains(pos) {
                    self.close_fullscreen_preview();
                }
            } else if rect.contains(pos) {
                self.preview.pick = Some([
                    (pos.x - rect.left()) / rect.width(),
                    (pos.y - rect.top()) / rect.height(),
                ]);
            }
        }
    }
    fn fullscreen_input(&mut self, events: &[egui::Event], rect: Rect, exit: Rect) {
        let touch_frame = events
            .iter()
            .any(|e| matches!(e, egui::Event::Touch { .. }));
        for event in events {
            match *event {
                egui::Event::Touch { id, phase, pos, .. } => match phase {
                    egui::TouchPhase::Start => {
                        self.preview.touches.insert(id.0);
                        if self.preview.touches.len() > 1 {
                            self.preview.blocked = true;
                            self.preview.drag = None;
                        } else if !self.preview.blocked {
                            self.preview_press(pos, rect, exit);
                        }
                    }
                    egui::TouchPhase::Move
                        if !self.preview.blocked && self.preview.touches.contains(&id.0) =>
                    {
                        self.preview_move(pos)
                    }
                    egui::TouchPhase::End => {
                        let owned = self.preview.touches.remove(&id.0);
                        if owned && !self.preview.blocked {
                            self.preview_release(pos, rect);
                        }
                        if self.preview.touches.is_empty() {
                            self.preview.blocked = false;
                        }
                    }
                    egui::TouchPhase::Cancel => {
                        self.preview.touches.clear();
                        self.preview.drag = None;
                        self.preview.blocked = false;
                        self.playing = false;
                    }
                    _ => {}
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    ..
                } if !touch_frame => {
                    if pressed {
                        self.preview_press(pos, rect, exit);
                    } else {
                        self.preview_release(pos, rect);
                    }
                }
                egui::Event::PointerMoved(pos) if !touch_frame => self.preview_move(pos),
                egui::Event::Scroll(delta) => {
                    self.beat = (self.beat
                        + (delta.y * self.options.scroll_speed / self.zoom) as f64)
                        .max(0.);
                }
                _ => {}
            }
        }
    }
}
