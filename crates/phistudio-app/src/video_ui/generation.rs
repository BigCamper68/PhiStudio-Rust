use super::*;

impl Workbench {
    pub(crate) fn suspend_movie(&mut self) {
        if let Some(job) = &self.video.job {
            job.cancel.store(true, Ordering::Relaxed);
            crate::video_encoder::cancel();
        }
    }

    pub(crate) fn video_back(&mut self) -> bool {
        if !self.video.open {
            return false;
        }
        if let Some(job) = &self.video.job {
            job.cancel.store(true, Ordering::Relaxed);
            crate::video_encoder::cancel();
        } else {
            self.video.open = false;
            self.video.completed = None;
        }
        true
    }

    pub(crate) async fn update_movie(&mut self, source: Option<&phistudio_prpr::Preview>) {
        if std::mem::take(&mut self.video.start) {
            match self.create_movie(source).await {
                Ok(job) => self.video.job = Some(job),
                Err(e) => {
                    crate::video_encoder::release();
                    self.video.error = Some(format!("Could not export MP4: {e:#}"));
                }
            }
        }
        let result = self.video.job.as_mut().and_then(MovieJob::update);
        if let Some(result) = result {
            let canceled = self
                .video
                .job
                .as_ref()
                .is_some_and(|j| j.cancel.load(Ordering::Relaxed));
            self.video.job = None;
            crate::video_encoder::release();
            if canceled {
                self.video.message = Some("Video export canceled");
            } else {
                match result {
                    Ok(file) => {
                        self.video.completed = Some(file);
                        self.video.message = Some("MP4 is ready. Choose where to save it.");
                    }
                    Err(e) => self.video.error = Some(format!("Could not export MP4: {e}")),
                }
            }
        }
    }

    pub(super) async fn create_movie(
        &mut self,
        source: Option<&phistudio_prpr::Preview>,
    ) -> Result<MovieJob> {
        self.video.error = None;
        self.video.message = None;
        let settings = self.video.settings.clone();
        let mut options = self.options.clone();
        options.mark_active_line_id = false;
        options.highlight_current_line = false;
        options.preview_hud = settings.hud;
        options.preview_particles = settings.particles;
        options.preview_shaders = settings.shaders;
        let renderer = if let Some(source) = source {
            phistudio_prpr::Preview::new_movie(
                self.editor.document(),
                self.asset_root(),
                &self.project_options,
                source,
            )
            .await?
        } else {
            phistudio_prpr::Preview::new(
                self.editor.document(),
                self.asset_root(),
                &self.project_options,
            )
            .await?
        };
        let mut renderer = MovieRenderer(renderer);
        if let Some(id) = &self.options.skin {
            let path = self
                .skins
                .entries
                .iter()
                .find(|s| &s.id == id)
                .context("Selected video skin is unavailable")?
                .assets
                .clone();
            renderer.set_skin(Some(&path)).await?;
        }
        renderer.set_editor_options(&options, usize::MAX)?;
        let plan = settings.plan(renderer.chart_duration())?;
        let audio = if settings.audio {
            Some(renderer.movie_audio(&settings)?)
        } else {
            None
        };
        renderer.begin_movie(plan)?;
        let mut builder = tempfile::Builder::new();
        builder.prefix("phistudio-video-");
        #[cfg(target_os = "android")]
        let directory = {
            let cache = std::env::current_dir()?.join("export-cache");
            std::fs::create_dir_all(&cache)?;
            builder.tempdir_in(cache)?
        };
        #[cfg(not(target_os = "android"))]
        let directory = builder.tempdir()?;
        let name = movie_name(&self.video.name);
        let cancel = Arc::new(AtomicBool::new(false));
        let phase = Arc::new(AtomicU8::new(0));
        let encoded = Arc::new(AtomicU64::new(0));
        let (sender, frames) = mpsc::sync_channel::<Option<Vec<u8>>>(1);
        let (result, receiver) = mpsc::channel();
        let (stop, state, count) = (cancel.clone(), phase.clone(), encoded.clone());
        crate::video_encoder::reset();
        std::thread::spawn(move || {
            let run = || -> Result<MovieFile> {
                let path = directory.path().join("movie.mp4");
                let wav = directory.path().join("audio.wav");
                if let Some(audio) = audio {
                    audio.write_wav(&wav, plan, &stop)?;
                }
                anyhow::ensure!(!stop.load(Ordering::Relaxed), "Video export canceled");
                let mut encoder = crate::video_encoder::Encoder::new(
                    &path,
                    settings.audio.then_some(wav.as_path()),
                    &settings,
                    plan,
                )?;
                state.store(1, Ordering::Release);
                let mut index = 0;
                loop {
                    let frame = frames.recv().context("Video export canceled")?;
                    anyhow::ensure!(!stop.load(Ordering::Relaxed), "Video export canceled");
                    let Some(rgba) = frame else {
                        break;
                    };
                    encoder.frame(&rgba, index)?;
                    index += 1;
                    count.store(index, Ordering::Relaxed);
                }
                anyhow::ensure!(index == plan.frames, "Incomplete movie frame sequence");
                state.store(2, Ordering::Release);
                encoder.finish()?;
                drop(encoder);
                anyhow::ensure!(!stop.load(Ordering::Relaxed), "Video export canceled");
                anyhow::ensure!(
                    path.metadata()?.len() > 0,
                    "Video encoder produced an empty file"
                );
                let _ = std::fs::remove_file(wav);
                Ok(MovieFile { directory, name })
            };
            let _ = result.send(run().map_err(|e| format!("{e:#}")));
        });
        Ok(MovieJob {
            renderer,
            plan,
            warm: 0,
            next: 0,
            last_time: None,
            pending: None,
            sender: Some(sender),
            receiver,
            cancel,
            encoded,
            phase,
        })
    }
}
