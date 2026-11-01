use super::*;

impl Preview {
    /// `viewport` uses physical GL pixels, with its origin at the bottom left.
    /// `chart_seconds` is the chart cursor; the transport applies audio offsets.
    pub fn render(&mut self, chart_seconds: f64, viewport: (i32, i32, i32, i32)) {
        if viewport.2 <= 0 || viewport.3 <= 0 || !chart_seconds.is_finite() {
            return;
        }
        let seeking = self.offline_step.is_none()
            && self.playtest.is_none()
            && (self.needs_seek
                || chart_seconds < self.last_time
                || (!self.transport.playing() && chart_seconds != self.last_time));
        if self.playtest.is_some() {
            self.needs_seek = false;
        }
        if seeking {
            self.seek_sound_schedule(chart_seconds);
            autoplay::seek(&mut self.chart, &mut self.judge, chart_seconds);
            self.clear_particles = true;
            self.needs_seek = false;
        }
        self.last_time = chart_seconds;
        let now = get_time();
        let dt = if let Some(step) = self.offline_step {
            step
        } else if self.transport.playing() && !seeking {
            (now - self.last_render_wall).max(0.) as f32
        } else {
            0.
        };
        self.last_render_wall = now;
        self.resource.update_size(viewport);
        self.resource.camera.zoom.y = -self.resource.aspect_ratio;
        self.resource.camera.render_target = Some(self.effect_target());
        self.resource.time = chart_seconds;
        set_camera(&self.resource.camera);
        if self.clear_particles {
            self.resource
                .emitter
                .draw(self.resource.res_pack.info.hit_fx_duration + 1.);
            self.clear_particles = false;
        }
        self.resource.config.particle = self.editor_options.preview_particles
            && (self.transport.playing() || self.offline_step.is_some())
            && !seeking;
        let delay_sounds = self.playtest.is_none()
            && (self.editor_options.hitsound_delay_ms != 0.
                || self.editor_options.drag_delay_ms != 0.);
        let sound_volume = self.resource.config.volume_sfx;
        if delay_sounds {
            self.resource.config.volume_sfx = 0.;
        }
        if (self.transport.playing() || self.offline_step.is_some()) && !seeking {
            let old = unsafe { get_internal_gl() }.quad_gl.get_viewport();
            if self.playtest.is_some() {
                unsafe { get_internal_gl() }
                    .quad_gl
                    .viewport(self.playtest_viewport);
            }
            let filter_touches = self.playtest.is_some() && self.noise_fields.enabled();
            let mut unused = vec![];
            let bad_notes = match &mut self.playtest {
                Some(session) => &mut session.bad_notes,
                None => &mut unused,
            };
            if filter_touches {
                let aspect = self.resource.aspect_ratio;
                self.judge.update_with_touch_filter(
                    &mut self.resource,
                    &mut self.chart,
                    bad_notes,
                    |touches| {
                        self.noise_fields
                            .filter_touches(touches, chart_seconds as f32, aspect, dt)
                    },
                );
                self.judge
                    .trackers
                    .retain(|id, _| !self.noise_fields.suppressed(*id));
            } else {
                self.judge
                    .update(&mut self.resource, &mut self.chart, bad_notes);
            }
            unsafe { get_internal_gl() }.quad_gl.viewport(old);
        }
        if let Err(error) = self
            .transport
            .update_noise_filter(self.playtest.is_some() && self.noise_fields.touching(), dt)
        {
            self.audio_warning = Some(format!("Noise Field audio filter failed: {error:#}"));
        }
        self.resource.config.volume_sfx = sound_volume;
        if delay_sounds && self.offline_step.is_none() && self.transport.playing() && !seeking {
            editor_audio::play_due(
                &mut self.sound_schedule,
                &self.chart,
                &mut self.resource,
                chart_seconds,
            );
        }
        self.resource.config.particle = self.editor_options.preview_particles
            && (self.transport.playing() || self.offline_step.is_some())
            && !seeking;
        if self.playtest.is_some() {
            self.update_playtest_line_color();
        }
        self.chart.update(&mut self.resource);
        self.resource.config.particle = self.editor_options.preview_particles;
        self.draw_frame(viewport, dt);
    }

    pub(crate) fn draw_frame(&mut self, viewport: (i32, i32, i32, i32), dt: f32) {
        self.resource.camera.render_target = Some(self.effect_target());
        let mut clear_camera = self.resource.camera;
        clear_camera.viewport = None;
        set_camera(&clear_camera);
        clear_background(Color::from_rgba(10, 15, 27, 255));
        if self.has_background {
            let aspect = viewport.2 as f32 / viewport.3 as f32;
            set_camera(&Camera2D {
                zoom: vec2(1., -aspect),
                viewport: Some(viewport),
                render_target: Some(self.effect_target()),
                ..Default::default()
            });
            let h = 1. / aspect;
            prpr::ext::draw_image(
                *self.resource.background,
                Rect::new(-1., -h, 2., 2. * h),
                prpr::ext::ScaleType::CropCenter,
            );
            // Phira scene::draw_background applies this before ChartInfo dim.
            draw_rectangle(-1., -h, 2., 2. * h, Color::new(0., 0., 0., 0.3));
        }
        let actual = self.resource.camera.viewport.unwrap_or(viewport);
        // Storyboard Text uses the original Phira face without an emoji fallback.
        let mut ui = Ui::new(&mut self.chart_text, Some(actual));
        set_camera(&self.resource.camera);
        if self.has_background {
            let h = 1. / self.resource.aspect_ratio;
            draw_rectangle(
                -1.,
                -h,
                2.,
                2. * h,
                Color::new(0., 0., 0., self.resource.info.background_dim),
            );
        }
        self.anchors
            .update(&mut self.chart, &self.resource, &mut ui);
        let active_line = self.anchors.visual_line(self.active_line);
        let old_color = if self.playtest.is_none() && self.editor_options.highlight_current_line {
            self.chart.lines.get_mut(active_line).map(|line| {
                std::mem::replace(
                    &mut line.color,
                    prpr::core::Anim::fixed(Color::from_rgba(91, 211, 172, 255)),
                )
            })
        } else {
            None
        };
        let source_ids = self.resource.config.chart_debug && self.anchors.has_visuals();
        if source_ids {
            self.resource.config.chart_debug = false;
        }
        let no_effect = self.resource.no_effect;
        self.resource.no_effect |= !self.editor_options.preview_shaders;
        self.chart.render(&mut ui, &mut self.resource);
        self.resource.no_effect = no_effect;
        if source_ids {
            self.resource.config.chart_debug = true;
        }
        if let Some(color) = old_color {
            self.chart.lines[active_line].color = color;
        }
        if !self.resource.no_effect {
            self.resource.camera.render_target =
                Some(self.resource.chart_target.as_ref().unwrap().output());
            set_camera(&self.resource.camera);
        }
        if source_ids {
            self.anchors
                .draw_source_ids(&self.chart, &mut self.resource, &mut ui);
        }
        if let Some(session) = &mut self.playtest {
            session
                .bad_notes
                .retain(|note| note.render(&mut self.resource));
        }
        if self.editor_options.preview_particles
            && (self.transport.playing() || self.playtest.is_some() || self.offline_step.is_some())
        {
            self.resource.emitter.draw(dt);
        }
        self.noise_fields.render(&mut self.resource, viewport);
        self.resource.camera.render_target = Some(
            self.resource
                .chart_target
                .as_ref()
                .map_or(self.target, |t| t.output()),
        );
        set_camera(&self.resource.camera);
        self.pause_bounds = hud::pause_bounds(&self.resource);
        if self.editor_options.preview_hud {
            ui.text_painter = &mut self.text;
            hud::render(&mut ui, &self.chart, &self.judge, &self.resource);
        }
        self.finish_effects(viewport);
        // prpr draws with ordinary alpha blending. Egui composites the finished
        // framebuffer as one opaque image; retain RGB and finish its alpha so
        // translucent notes/dimming do not blend against the editor a second time.
        let mut gl = unsafe { get_internal_gl() };
        gl.flush();
        gl.quad_context.begin_pass(
            self.target.render_pass,
            macroquad::miniquad::PassAction::Nothing,
        );
        gl.quad_context.set_color_write((false, false, false, true));
        gl.quad_context.clear(Some((0., 0., 0., 1.)), None, None);
        gl.quad_context.set_color_write((true, true, true, true));
        gl.quad_context.end_render_pass();
        set_default_camera();
        prpr::ext::flush_pending_texture_deletions();
    }
}
