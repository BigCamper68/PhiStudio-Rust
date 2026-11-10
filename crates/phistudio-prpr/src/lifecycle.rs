use super::*;

impl Preview {
    pub async fn new(
        document: &ChartDocument,
        asset_root: Option<&Path>,
        options: &ProjectOptions,
    ) -> Result<Self> {
        Self::build(document, asset_root, options, None).await
    }

    /// A separate native scene, sharing only immutable decoded music with Preview.
    pub async fn new_movie(
        document: &ChartDocument,
        asset_root: Option<&Path>,
        options: &ProjectOptions,
        source: &Self,
    ) -> Result<Self> {
        Self::build(document, asset_root, options, Some(source)).await
    }

    pub(super) async fn build(
        document: &ChartDocument,
        asset_root: Option<&Path>,
        options: &ProjectOptions,
        source: Option<&Self>,
    ) -> Result<Self> {
        prpr_l10n::set_prefered_locale(Some("en-US".parse()?));
        let mut fs = assets::ChartAssets::new(asset_root)?;
        let (extra, shader_warning, shader_key) = effects::load(document, asset_root).await;
        let mut chart = prpr::parse::parse_rpe(
            &compatibility::render_source(document)?,
            &mut fs,
            extra,
            options.use_rpe_170_speed,
        )
        .await?;
        compatibility::apply(document, &mut chart);
        let source_ids = picking::source_map(document, &chart);
        let anchors = anchors::Anchors::install(document, &mut chart);
        let font = FontArc::try_from_slice(TEXT_FONT_BYTES)?;
        let emoji_font = FontArc::try_from_slice(EMOJI_FONT_BYTES)?;
        let pgr_font =
            FontArc::try_from_slice(include_bytes!("../../../assets/fonts/phigros.ttf"))?;
        PGR_FONT.with(|p| *p.borrow_mut() = Some(TextPainter::new(pgr_font, Some(font.clone()))));
        BOLD_FONT.with(|p| {
            *p.borrow_mut() = Some(TextPainter::new(font.clone(), Some(emoji_font.clone())))
        });
        let judge = Judge::new(&chart);
        let (mut resource, audio_warning) = resources::editor_resource().await?;
        if let Some(source) = source {
            resource.music = source.resource.music.clone();
            resource.background = source.resource.background.clone();
            resource.illustration = source.resource.illustration.clone();
        }
        let mut noise_fields = noise_fields::NoiseFields::default();
        noise_fields.load(document).await?;
        let mut preview = Self {
            speed_curve_horizon: crate::speed_curve::clock::native_horizon(document),
            extension_effects: Default::default(),
            noise_fields,
            anchors,
            offline_step: None,
            resource_music: None,
            maximum_target_edge: 0,
            input_pump: None,
            playtest: None,
            playtest_viewport: None,
            shader_warning,
            shader_key,
            picking: picking::PickingState::new(source_ids),
            sound_schedule: Default::default(),
            hitsound_audition: Default::default(),
            editor_options: phistudio_core::EditorOptions {
                mark_active_line_id: false,
                highlight_current_line: false,
                ..Default::default()
            },
            active_line: usize::MAX,
            package_force_aspect: true,
            package_dim: 0.6,
            pause_bounds: [0., 0., 0.09, 0.15],
            chart,
            judge,
            resource,
            transport: transport::Transport::new(),
            chart_text: TextPainter::new(font.clone(), None),
            text: TextPainter::new(font, Some(emoji_font)),
            last_time: f64::NEG_INFINITY,
            target: render_target(1280, 720),
            background_key: source.and_then(|s| s.background_key.clone()),
            music_key: source.and_then(|s| s.music_key.clone()),
            music_offset: 0.,
            music_length: source.and_then(|s| s.music_length),
            audio_available: audio_warning.is_none(),
            audio_warning,
            music_warning: None,
            illustration_warning: source.and_then(|s| s.illustration_warning.clone()),
            needs_seek: true,
            clear_particles: false,
            last_render_wall: get_time(),
            has_background: source.is_some_and(|s| s.has_background),
            resource_warning: None,
        };
        preview.configure_effects();
        preview.apply_options(document, asset_root, options).await;
        Ok(preview)
    }

    pub async fn replace_chart(
        &mut self,
        document: &ChartDocument,
        asset_root: Option<&Path>,
        options: &ProjectOptions,
    ) -> Result<()> {
        let mut fs = assets::ChartAssets::new(asset_root)?;
        let mut chart = prpr::parse::parse_rpe(
            &compatibility::render_source(document)?,
            &mut fs,
            prpr::core::ChartExtra::default(),
            options.use_rpe_170_speed,
        )
        .await?;
        self.noise_fields.load(document).await?;
        // Keep the last valid chart if parsing fails, and let the UI report it.
        compatibility::apply(document, &mut chart);
        self.sound_schedule.ready = false;
        self.picking
            .replace_sources(picking::source_map(document, &chart));
        self.anchors = anchors::Anchors::install(document, &mut chart);
        self.judge = Judge::new(&chart);
        effects::replace(self, &mut chart, document, asset_root).await;
        self.speed_curve_horizon = crate::speed_curve::clock::native_horizon(document);
        self.chart = chart;
        self.hitsound_audition = Default::default();
        self.configure_effects();
        self.apply_options(document, asset_root, options).await;
        self.last_time = f64::NEG_INFINITY;
        self.needs_seek = true;
        self.clear_particles = true;
        Ok(())
    }

    pub(super) async fn apply_options(
        &mut self,
        document: &ChartDocument,
        root: Option<&Path>,
        options: &ProjectOptions,
    ) {
        self.package_dim = options.background_dim(document);
        self.package_force_aspect = options.force_aspect_ratio;
        self.resource.info.background_dim =
            self.editor_options.preview_dim.unwrap_or(self.package_dim);
        self.resource.info.note_uniform_scale = options.note_uniform_scale;
        self.resource.info.hold_partial_cover = options.hold_partial_cover;
        self.resource.info.force_aspect_ratio = options.force_aspect_ratio;
        self.resource.info.use_attach_ui_fix = options.use_attach_ui_fix;
        self.resource.info.name = options.display_field(document, "name").into();
        self.resource.info.level = options.display_field(document, "level").into();
        self.resource.info.charter = options.display_field(document, "charter").into();
        self.resource.info.composer = options.display_field(document, "composer").into();
        self.resource.info.music = options.music.clone().unwrap_or_default();
        self.resource.info.offset = options.manifest_offset_ms as f32 / 1000.;
        self.chart.settings.hold_partial_cover = options.hold_partial_cover;
        self.resource.info.aspect_ratio = options.aspect_ratio.unwrap_or(16. / 9.);
        self.resource.camera.zoom.y = -self.resource.info.aspect_ratio;
        self.resource.last_vp = (0, 0, 0, 0);
        let path = options.illustration.clone().or_else(|| {
            document.as_json()["META"]["background"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        });
        let key = (root.map(Path::to_owned), path);
        if self.background_key.as_ref() != Some(&key) {
            self.has_background = false;
            self.illustration_warning = None;
            if let Some(path) = &key.1 {
                let loaded = async {
                    let mut fs = assets::ChartAssets::new(root)?;
                    let (illustration, background) = background::decode(fs.load_file(path).await?)?;
                    let mut maximum = 0;
                    // Preserve Phira's full artwork resolution whenever the
                    // current GPU supports it. Resize only unsupported textures.
                    unsafe {
                        macroquad::miniquad::gl::glGetIntegerv(
                            macroquad::miniquad::gl::GL_MAX_TEXTURE_SIZE,
                            &mut maximum,
                        );
                    }
                    let maximum = maximum.max(1) as u32;
                    let fit = |image: image::DynamicImage| {
                        if image.width() > maximum || image.height() > maximum {
                            image.resize(maximum, maximum, image::imageops::FilterType::Triangle)
                        } else {
                            image
                        }
                    };
                    Ok::<_, anyhow::Error>((
                        prpr::ext::SafeTexture::from(fit(illustration)),
                        prpr::ext::SafeTexture::from(fit(background.into())),
                    ))
                }
                .await;
                match loaded {
                    Ok((illustration, background)) => {
                        self.resource.background = background;
                        self.resource.illustration = illustration;
                        self.has_background = true;
                    }
                    Err(error) => {
                        self.illustration_warning =
                            Some(format!("Could not load illustration {path}: {error:#}"))
                    }
                }
            }
            self.background_key = Some(key);
        }
        self.music_offset = options.music_offset_seconds(document);
        let key = (root.map(Path::to_owned), options.music.clone());
        if self.music_key.as_ref() != Some(&key) {
            self.music_warning = None;
            let mut music = None;
            if let Some(path) = &key.1 {
                let loaded = async {
                    let mut fs = assets::ChartAssets::new(root)?;
                    sasa::AudioClip::new(fs.load_file(path).await?)
                }
                .await;
                match loaded {
                    Ok(clip) => {
                        self.resource.music = clip.clone();
                        music = Some(clip);
                    }
                    Err(error) => {
                        self.music_warning = Some(format!("Could not load music {path}: {error:#}"))
                    }
                }
            }
            self.music_length = music.as_ref().map(sasa::AudioClip::length);
            if !self.audio_available {
                music = None;
            }
            if let Err(error) = self.transport.set_music(&mut self.resource.audio, music) {
                self.music_warning = Some(format!("Could not start music: {error:#}"));
            }
            self.music_key = Some(key);
        }
        self.resource.track_length = self.music_length.unwrap_or_else(|| {
            self.chart
                .lines
                .iter()
                .flat_map(|line| &line.notes)
                .map(|note| match note.kind {
                    prpr::core::NoteKind::Hold { end_time, .. } => end_time,
                    _ => note.time,
                })
                .fold(0., f64::max)
                + 1.
        });
        self.resource.extra_sfxs.clear();
        if self.audio_available {
            for (name, clip) in &self.chart.hitsounds {
                match self
                    .resource
                    .audio
                    .create_sfx(clip.clone(), Some(prpr::core::BUFFER_SIZE))
                {
                    Ok(sfx) => {
                        self.resource.extra_sfxs.insert(name.clone(), sfx);
                    }
                    Err(error) => {
                        self.music_warning =
                            Some(format!("Could not load hitsound {name}: {error:#}"))
                    }
                }
            }
        }
        self.refresh_warning();
    }

    pub(super) fn refresh_warning(&mut self) {
        let warnings: Vec<_> = [
            &self.illustration_warning,
            &self.music_warning,
            &self.audio_warning,
            &self.shader_warning,
        ]
        .into_iter()
        .filter_map(|s| s.as_deref())
        .collect();
        self.resource_warning = (!warnings.is_empty()).then(|| warnings.join("\n"));
    }

    pub fn invalidate_resources(&mut self) {
        self.background_key = None;
        self.music_key = None;
        if let Err(error) = self
            .transport
            .synchronize(0., false, true, self.music_offset)
        {
            self.audio_warning = Some(format!("Could not pause audio: {error:#}"));
            self.refresh_warning();
        }
    }
}
