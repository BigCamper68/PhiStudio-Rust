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
        self.cancel_plugin_input();
        self.preview.drag = None;
        self.preview.touches.clear();
        self.preview.blocked = false;
        self.preview.fullscreen = true;
        self.timeline.cancel_gesture();
        self.transport.scrubbing = false;
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
            self.plugin_preview_overlay(&painter.with_clip_rect(area), rect, "grid-preview");
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
        self.plugin_preview_overlay(ui.painter(), rect, "preview");
    }
    pub(crate) fn sidebar_preview(&mut self, ui: &mut egui::Ui) {
        if self.options.preview_behind_grid {
            ui.horizontal_wrapped(|ui| {
                if ui.button(crate::i18n::tr("Fullscreen Preview ↗")).clicked() {
                    self.open_fullscreen_preview();
                }
                self.hitsound_menu(ui);
            });
            ui.separator();
        } else if !self.preview.floating {
            let height = ui.spacing().interact_size.y;
            ui.spacing_mut().interact_size.y = 18.;
            ui.horizontal(|ui| {
                ui.small(egui::RichText::new(crate::i18n::tr("PREVIEW")).color(muted()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.hitsound_menu(ui);
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
        } else {
            self.hitsound_menu(ui);
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
        let plugin_input = self.plugin_preview_input(ui, rect, Rect::NOTHING).is_some();
        if response.clicked() && !self.modal_open() && !plugin_input {
            self.open_fullscreen_preview();
        }
    }
    pub(crate) fn close_fullscreen_preview(&mut self) {
        self.cancel_plugin_input();
        self.cancel_noise_preview_editor();
        self.noise.sample_target = None;
        self.preview.drag = None;
        self.preview.touches.clear();
        self.preview.blocked = false;
        self.preview.fullscreen = false;
        self.timeline.cancel_gesture();
        self.transport.scrubbing = false;
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
        self.timeline.area = Rect::NOTHING;
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                && !self.cancel_plugin_editor_action()
            {
                self.close_fullscreen_preview();
            }
            if !self.modal_open() && i.consume_key(egui::Modifiers::NONE, egui::Key::Space) {
                self.request_editor_playback(!self.transport.playing, false);
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
                if !self.modal_open() {
                    self.noise_preview_editor(ui, rect);
                    self.noise_sample_input(ui, rect);
                    let events = self
                        .plugin_preview_input(ui, rect, exit)
                        .unwrap_or_else(|| ui.input(|i| i.events.clone()));
                    self.fullscreen_input(&events, rect, exit);
                }
            });
        self.plugin_editor_action_window(ctx);
    }
    fn preview_move(&mut self, pos: egui::Pos2) {
        if self.noise_preview_move(pos) {
            return;
        }
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
                self.transport.beat = (self.transport.beat
                    + ((pos.y - d.last.y) * self.options.scroll_speed / self.timeline.zoom) as f64)
                    .max(0.);
            }
            if d.vertical.is_some() {
                d.last = pos;
            }
        }
    }
    fn preview_press(&mut self, pos: egui::Pos2, rect: Rect, exit: Rect) {
        if self.noise_preview_press(pos) {
            return;
        }
        if rect.contains(pos) {
            self.preview.drag = Some(crate::preview_ui::PreviewDrag {
                origin: pos,
                last: pos,
                vertical: None,
                moved: false,
                exit: self.noise.sample_target.is_none() && exit.contains(pos),
            });
        }
    }
    fn preview_release(&mut self, pos: egui::Pos2, rect: Rect) {
        self.preview_move(pos);
        if self.noise.sample_target.is_some() {
            self.noise_sample_release(pos);
            self.noise_preview_end_gesture();
            self.preview.drag = None;
            return;
        }
        if self.noise.preview_original.is_some() {
            self.noise_preview_end_gesture();
            self.preview.drag = None;
            return;
        }
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
                            self.cancel_noise_bounds_drag();
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
                        self.cancel_noise_bounds_drag();
                        self.preview.touches.clear();
                        self.preview.drag = None;
                        self.preview.blocked = false;
                        self.transport.playing = false;
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
                    self.transport.beat = (self.transport.beat
                        + (delta.y * self.options.scroll_speed / self.timeline.zoom) as f64)
                        .max(0.);
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod noise_edit_tests {
    use super::*;
    use phistudio_core::{
        ChartDocument,
        noise_fields::{self as noise, Format},
    };
    fn editor(format: Format) -> Workbench {
        let mut app = Workbench::new(ChartDocument::empty("Field gestures", 120.).unwrap(), None);
        app.advanced.right_mode = crate::advanced_ui::RightMode::NoiseFields;
        app.noise.format = format;
        app.add_noise();
        app.options.snap_x = false;
        app.options.snap_time = false;
        app.open_noise_preview_editor();
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1000., 700.))),
                    ..Default::default()
                },
                |ctx| app.fullscreen_preview(ctx),
            );
        }
        app
    }
    fn point_editor(
        format: Format,
        track: noise::Track,
        target: crate::noise_ui::SampleTarget,
    ) -> Workbench {
        let mut app = editor(format);
        app.close_fullscreen_preview();
        app.noise.animation = true;
        app.noise.track = track;
        app.add_noise();
        app.open_noise_sample(target);
        draw_point(&mut app);
        app
    }
    fn draw_point(app: &mut Workbench) {
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1000., 700.))),
                    ..Default::default()
                },
                |ctx| app.fullscreen_preview(ctx),
            );
        }
    }
    fn point(app: &Workbench) -> serde_json::Value {
        let (_, field) = app.noise_preview_override().unwrap();
        field[app.noise.track.key()][app.noise.key_selected.unwrap()].clone()
    }
    #[test]
    fn preview_points_stay_open_snap_and_apply_with_one_undo() {
        use crate::noise_ui::SampleTarget;
        for (format, track, target, name) in [
            (
                Format::Native,
                noise::Track::Move,
                SampleTarget::Position,
                "endPosition",
            ),
            (
                Format::Native,
                noise::Track::Scale,
                SampleTarget::Anchor,
                "anchor",
            ),
            (
                Format::Next,
                noise::Track::Rotate,
                SampleTarget::Anchor,
                "anchor",
            ),
        ] {
            let mut app = point_editor(format, track, target);
            app.timeline.lanes = 9;
            app.timeline.division = 8;
            app.options.snap_x = true;
            app.options.snap_time = true;
            let source = app.editor.document().as_json().clone();
            for p in [[0.13, 0.77], [0.86, 0.27]] {
                let pos = pos2(
                    app.preview.rect.left() + app.preview.rect.width() * p[0],
                    app.preview.rect.bottom() - app.preview.rect.height() * p[1],
                );
                input(
                    &mut app,
                    &[
                        touch(1, egui::TouchPhase::Start, pos),
                        touch(1, egui::TouchPhase::End, pos),
                    ],
                );
                assert!(app.preview.fullscreen);
                assert_eq!(app.noise.sample_target, Some(target));
                assert!(app.preview.pick.is_none());
                assert_eq!(app.editor.document().as_json(), &source);
            }
            let value = point(&app);
            let expected = if format == Format::Native {
                [0.875, 0.25]
            } else {
                [506.25, -225.]
            };
            assert_eq!(value[name]["x"], serde_json::json!(expected[0]));
            assert_eq!(value[name]["y"], serde_json::json!(expected[1]));
            assert!(app.apply_noise_sample());
            assert!(!app.preview.fullscreen);
            assert!(app.noise.sample_target.is_none());
            assert!(app.editor.undo());
            assert_eq!(app.editor.document().as_json(), &source);
        }
    }
    #[test]
    fn preview_point_drag_cancel_and_scrub_preserve_prior_staged_choice() {
        use crate::noise_ui::SampleTarget;
        let mut app = point_editor(Format::Next, noise::Track::ScaleX, SampleTarget::Anchor);
        let source = app.editor.document().as_json().clone();
        let initial = point(&app);
        let pos = pos2(
            app.preview.rect.left() + app.preview.rect.width() * 0.75,
            app.preview.rect.bottom() - app.preview.rect.height() * 0.25,
        );
        input(
            &mut app,
            &[
                touch(1, egui::TouchPhase::Start, pos),
                touch(1, egui::TouchPhase::End, pos),
            ],
        );
        let staged = point(&app);
        assert_ne!(staged, initial);
        draw_point(&mut app);
        let a = app.noise.preview_handles[0].center();
        let b = a + vec2(-80., -40.);
        input(
            &mut app,
            &[
                touch(1, egui::TouchPhase::Start, a),
                touch(1, egui::TouchPhase::Move, b),
            ],
        );
        assert_ne!(point(&app), staged);
        input(
            &mut app,
            &[
                touch(2, egui::TouchPhase::Start, b),
                touch(1, egui::TouchPhase::End, b),
                touch(2, egui::TouchPhase::End, b),
            ],
        );
        assert_eq!(point(&app), staged);
        let a = app.preview.rect.center_bottom() - vec2(200., 40.);
        let beat = app.transport.beat;
        input(
            &mut app,
            &[
                touch(1, egui::TouchPhase::Start, a),
                touch(1, egui::TouchPhase::End, a + vec2(0., 30.)),
            ],
        );
        assert!(app.transport.beat > beat);
        assert_eq!(point(&app), staged);
        app.close_fullscreen_preview();
        assert!(app.noise.sample_target.is_none());
        assert!(app.flush_noise_draft());
        assert_eq!(app.editor.document().as_json(), &source);
    }
    fn touch(id: u64, phase: egui::TouchPhase, pos: egui::Pos2) -> egui::Event {
        egui::Event::Touch {
            device_id: egui::TouchDeviceId(0),
            id: egui::TouchId(id),
            phase,
            pos,
            force: 0.,
        }
    }
    fn input(app: &mut Workbench, events: &[egui::Event]) {
        let rect = app.preview.rect;
        app.fullscreen_input(events, rect, app.fullscreen_exit_rect(rect));
    }
    #[test]
    fn field_handles_stage_both_dialects_and_commit_as_one_undo() {
        for format in [Format::Native, Format::Next] {
            let mut app = editor(format);
            let source = app.editor.document().as_json().clone();
            let original = app.noise_preview_override().unwrap().1;
            let a = app.noise.preview_handles[0].center();
            let b = a + vec2(50., -30.);
            input(
                &mut app,
                &[
                    touch(1, egui::TouchPhase::Start, a),
                    touch(1, egui::TouchPhase::Move, b),
                    touch(1, egui::TouchPhase::End, b),
                ],
            );
            let modified = app.noise_preview_override().unwrap().1;
            assert_ne!(original, modified);
            let before = noise::bounds(&original).unwrap();
            let after = noise::bounds(&modified).unwrap();
            assert_eq!(before[1], after[1]);
            assert!(after[0][0] > before[0][0] && after[0][1] > before[0][1]);
            assert_eq!(app.editor.document().as_json(), &source);
            assert!(app.preview.pick.is_none());
            assert!(app.flush_noise_draft());
            app.noise.preview_original = None;
            app.close_fullscreen_preview();
            assert!(app.editor.undo());
            assert_eq!(app.editor.document().as_json(), &source);
        }
    }
    #[test]
    fn background_swipes_scrub_and_center_handle_preserves_dimensions() {
        let mut app = editor(Format::Native);
        let original = app.noise_preview_override().unwrap().1;
        let a = app.preview.rect.center_bottom() - vec2(100., 30.);
        let b = a + vec2(0., 50.);
        let beat = app.transport.beat;
        input(
            &mut app,
            &[
                touch(1, egui::TouchPhase::Start, a),
                touch(1, egui::TouchPhase::Move, b),
                touch(1, egui::TouchPhase::End, b),
            ],
        );
        assert!(app.transport.beat > beat);
        assert_eq!(app.noise_preview_override().unwrap().1, original);
        let a = app.noise.preview_handles[2].center();
        let b = a + vec2(40., -20.);
        input(
            &mut app,
            &[
                touch(2, egui::TouchPhase::Start, a),
                touch(2, egui::TouchPhase::End, b),
            ],
        );
        let before = noise::bounds(&original).unwrap();
        let after = noise::bounds(&app.noise_preview_override().unwrap().1).unwrap();
        for axis in 0..2 {
            assert!(
                ((after[1][axis] - after[0][axis]) - (before[1][axis] - before[0][axis])).abs()
                    < 1e-9
            );
        }
        app.close_fullscreen_preview();
        assert!(app.noise_preview_override().is_none());
        assert!(app.flush_noise_draft());
        assert_eq!(noise::fields(app.editor.document()).unwrap()[0], original);
    }
    #[test]
    fn preview_grid_snap_uses_main_density_and_taps_do_not_change_geometry() {
        for format in [Format::Native, Format::Next] {
            let mut app = editor(format);
            app.timeline.lanes = 9;
            app.timeline.division = 8;
            app.options.snap_x = true;
            app.options.snap_time = true;
            let original = app.noise_preview_override().unwrap().1;
            let a = app.noise.preview_handles[0].center();
            input(
                &mut app,
                &[
                    touch(1, egui::TouchPhase::Start, a),
                    touch(1, egui::TouchPhase::End, a),
                ],
            );
            assert_eq!(app.noise_preview_override().unwrap().1, original);
            let b = a + vec2(63., -53.);
            input(
                &mut app,
                &[
                    touch(2, egui::TouchPhase::Start, a),
                    touch(2, egui::TouchPhase::End, b),
                ],
            );
            let bounds = noise::bounds(&app.noise_preview_override().unwrap().1).unwrap();
            let scales = if format == Format::Native {
                [1., 1.]
            } else {
                [1350., 900.]
            };
            for (axis, scale) in scales.into_iter().enumerate() {
                let n = (bounds[0][axis] / scale + if format == Format::Native { 0. } else { 0.5 })
                    * 8.;
                assert!((n - n.round()).abs() < 1e-9);
            }
        }
        let mut options: phistudio_core::EditorOptions = serde_json::from_str("{}").unwrap();
        assert!(options.noise_preview_grid);
        options.noise_preview_grid = false;
        let saved = serde_json::to_string(&options).unwrap();
        let restored: phistudio_core::EditorOptions = serde_json::from_str(&saved).unwrap();
        assert!(!restored.noise_preview_grid);
    }
    #[test]
    fn second_finger_and_touch_cancel_restore_only_the_current_drag() {
        for second_finger in [false, true] {
            let mut app = editor(Format::Next);
            let original = app.noise_preview_override().unwrap().1;
            let a = app.noise.preview_handles[0].center();
            let b = a + vec2(40., -20.);
            input(
                &mut app,
                &[
                    touch(1, egui::TouchPhase::Start, a),
                    touch(1, egui::TouchPhase::Move, b),
                ],
            );
            assert_ne!(app.noise_preview_override().unwrap().1, original);
            let phase = if second_finger {
                egui::TouchPhase::Start
            } else {
                egui::TouchPhase::Cancel
            };
            input(&mut app, &[touch(2, phase, b)]);
            input(
                &mut app,
                &[
                    touch(1, egui::TouchPhase::Move, b + vec2(30., 30.)),
                    touch(1, egui::TouchPhase::End, b),
                ],
            );
            assert_eq!(app.noise_preview_override().unwrap().1, original);
            assert!(app.preview.pick.is_none());
            assert!(app.noise.preview_original.is_some());
        }
    }
}
