mod smoke;
use super::*;
mod options;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn desktop_workspace() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
        .context("Cannot locate the application data directory")?;
    Ok(base.join("phistudio-rust"))
}

pub(crate) fn launch(options: Options) {
    #[cfg(target_os = "macos")]
    unsafe {
        unsafe extern "C" {
            fn phistudio_set_app_icon(bytes: *const u8, length: usize);
        }
        let png = include_bytes!("../../../../assets/icons/phistudio-512.png");
        phistudio_set_app_icon(png.as_ptr(), png.len());
    }
    let (width, height) = options.size.unwrap_or((1280, 800));
    macroquad::Window::from_config(
        Conf {
            window_title: "PhiStudio Rust".into(),
            window_width: width,
            window_height: height,
            // The pinned iOS backend already uses UIScreen.scale with high_dpi off.
            high_dpi: !cfg!(target_os = "ios"),
            sample_count: 1,
            icon: Some(macroquad::miniquad::conf::Icon {
                small: *include_bytes!("../../../../assets/icons/phistudio-16.rgba"),
                medium: *include_bytes!("../../../../assets/icons/phistudio-32.rgba"),
                big: *include_bytes!("../../../../assets/icons/phistudio-64.rgba"),
            }),
            ..Default::default()
        },
        async move {
            if let Err(e) = run(options).await {
                eprintln!("PhiStudio: {e:#}");
                std::process::exit(1);
            }
        },
    );
}

pub(crate) async fn run(options: Options) -> Result<()> {
    // Some upstream asset decoders spawn Tokio tasks. GL stays on this thread.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()?;
    let _runtime = runtime.enter();
    prevent_quit();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let mut storage_root = match options.workspace {
        Some(root) => Some(root),
        None if options.chart.is_none()
            && (options.smoke.is_none() || options.package.is_some()) =>
        {
            Some(desktop_workspace()?)
        }
        None => None,
    };
    #[cfg(target_os = "android")]
    let mut storage_root = Some(std::env::current_dir()?);
    #[cfg(target_os = "ios")]
    let mut storage_root = Some(ios::initialize()?);
    if let Some(root) = &mut storage_root {
        std::fs::create_dir_all(&root)?;
        *root = root.canonicalize()?;
    }
    let document = match &options.chart {
        Some(path) if storage_root.is_none() => load_chart(path)?,
        _ if options.chart.is_none()
            && options.package.is_none()
            && options.smoke.is_none()
            && options.playback_smoke.is_none() =>
        {
            ChartDocument::empty("Untitled", 120.)?
        }
        _ => ChartDocument::parse(DEMO)?,
    };
    let mut app = Workbench::new(
        document,
        if storage_root.is_none() {
            options.chart.clone()
        } else {
            None
        },
    );
    if let Some(root) = &storage_root {
        if let Some(path) = options.package.as_ref().or(options.chart.as_ref()) {
            let project = phistudio_core::import_project(
                path,
                &root.join("projects"),
                path.file_name()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap_or("Chart"),
                options.package.is_some(),
            )?;
            app.offer_project(project);
        }
        if options.chart.is_none()
            && options.package.is_none()
            && options.smoke.is_none()
            && options.playback_smoke.is_none()
        {
            app.open_projects();
            app.files.recovery_loading = true;
            app.files.request = Some(files::FileRequest::Startup);
        } else {
            app.files.recovery = match phistudio_core::load_recovery(root) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    app.error = Some(format!("Could not read recovery: {error}"));
                    None
                }
            };
        }
        app.managed_storage = true;
    }
    let bundled_demo = app.file.is_none() && options.chart.is_none() && options.package.is_none();
    let initial_lines = app.editor.document().line_count();
    let initial_notes = app.editor.document().note_count();
    let initial_line0 = app.editor.document().as_json()["judgeLineList"][0]["notes"]
        .as_array()
        .map_or(0, Vec::len);
    if options.smoke.is_some() {
        ensure!(
            !app.modal_open(),
            "Choose a chart or resolve pending recovery before running a smoke check"
        );
        if bundled_demo {
            ensure!(
                initial_lines == 1 && initial_notes == 4,
                "Bundled Java demo has changed"
            );
        }
        app.transport.beat = if bundled_demo {
            0.5
        } else {
            (0..initial_lines)
                .flat_map(|i| app.editor.document().notes(i))
                .filter(|(_, n)| {
                    n.kind == NoteKind::Tap && n.is_fake == 0 && n.start_time.as_f64() >= 0.5
                })
                .map(|(_, n)| n.start_time)
                .min()
                .map_or(0.5, |b| (b.as_f64() - 0.5).max(0.))
        };
    }
    let mut preview = match Preview::new(
        app.editor.document(),
        app.asset_root(),
        &app.project_options,
    )
    .await
    {
        Ok(preview) => preview,
        Err(error) => {
            // A chart/resource unsupported by preview must remain editable and
            // exportable, including when restoring the last Android project.
            app.render_error = Some(format!("{error:#}"));
            Preview::new(
                &ChartDocument::empty("Preview", 120.)?,
                None,
                &phistudio_core::ProjectOptions::default(),
            )
            .await?
        }
    };
    app.resource_warning = preview.resource_warning().map(str::to_owned);
    let mut gui = gui::Gui::new();
    app.audio_guide.set_cache_root(storage_root.clone());
    let mut files = files::FileManager::new(storage_root);
    app.preview.texture = Some(egui::TextureId::User(preview.texture_id()));
    app.skin = Some(preview.editor_skin());
    app.note_highlights = preview.highlighted_notes();
    let mut rendered = (
        app.session,
        app.editor.revision(),
        app.file.clone(),
        app.asset_revision,
    );
    let mut noise_override = None;
    let mut applied_skin: Option<String> = None;
    let mut frame = 0_u32;
    let mut input_blocked = false;
    if let Some(dir) = &options.smoke {
        std::fs::create_dir_all(dir)?;
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let mut playback_smoke = options
        .playback_smoke
        .as_deref()
        .map(playback_smoke::Smoke::new)
        .transpose()?;
    loop {
        files.update(&mut app);
        if let Err(error) = preview.pump_playtest_input() {
            app.error = Some(format!("Playtest input failed: {error:#}"));
            app.pause_playtest();
        }
        if app.skins.loaded && app.options.skin != applied_skin {
            let path = app
                .options
                .skin
                .as_ref()
                .and_then(|id| app.skins.entries.iter().find(|e| &e.id == id))
                .map(|e| e.assets.clone());
            if app.options.skin.is_some() && path.is_none() {
                app.options.skin = None;
            }
            match preview.set_skin(path.as_deref()).await {
                Ok(()) => {
                    applied_skin = app.options.skin.clone();
                    app.skin = Some(preview.editor_skin());
                }
                Err(e) => {
                    app.options.skin = applied_skin.clone();
                    app.error = Some(format!("Could not load skin: {e:#}"));
                }
            }
        }
        let blocked = app.modal_open();
        if blocked && !input_blocked {
            gui.cancel_input();
        }
        input_blocked = blocked;
        #[cfg(target_os = "android")]
        {
            use std::sync::atomic::Ordering;
            if android::WAS_SUSPENDED.swap(false, Ordering::AcqRel) {
                app.transport.playing = false;
                app.pause_playtest();
                app.suspend_movie();
                app.timeline.cancel_gesture();
                app.transport.scrubbing = false;
                app.preview.drag = None;
                gui.cancel_input();
                if let Err(error) = files.checkpoint(&app, false) {
                    app.error = Some(format!("Could not save recovery: {error}"));
                }
            }
            if android::BACK_REQUESTED.swap(false, Ordering::AcqRel) {
                app.back();
            }
        }
        #[cfg(target_os = "ios")]
        if ios::WAS_SUSPENDED.swap(false, std::sync::atomic::Ordering::AcqRel) {
            app.transport.playing = false;
            app.pause_playtest();
            app.suspend_movie();
            app.timeline.cancel_gesture();
            app.transport.scrubbing = false;
            app.preview.drag = None;
            gui.cancel_input();
            if let Err(error) = files.checkpoint(&app, false) {
                app.error = Some(format!("Could not save recovery: {error}"));
            }
        }
        if app.transport.playing && !app.is_scrubbing() && app.render_error.is_none() {
            app.transport.beat = app
                .editor
                .document()
                .tempo()
                .beat_at(preview.transport_time());
        }
        if is_quit_requested() {
            if app.editor.is_dirty() {
                app.close_requested = true;
                app.transport.playing = false;
            } else {
                break;
            }
        }
        if app.close_confirmed || (app.close_requested && !app.editor.is_dirty()) {
            break;
        }
        let background = theme::base().to_array();
        clear_background(Color::from_rgba(
            background[0],
            background[1],
            background[2],
            background[3],
        ));
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        if let Some(smoke) = &mut playback_smoke {
            let (events, canceled) = smoke.take_input(&app);
            gui.probe_input(events, canceled);
        }
        let before_ui = app.transport.beat;
        if !app.show_projects && app.preview.rect.is_positive() {
            let size = app.preview.rect.size() * app.pixels_per_point;
            preview.fit_display([size.x, size.y], app.preview_aspect());
        }
        app.preview.size = preview.render_size();
        app.preview.texture = Some(egui::TextureId::User(preview.texture_id()));
        gui.set_ui_scale(app.options.ui_scale);
        #[cfg(target_os = "ios")]
        ios::set_language(app.options.language);
        gui.run(|ctx, canceled| app.ui(ctx, canceled));
        if app.workspace.view != Some(workspace_ui::View::Resources) || app.transport.playing {
            let _ = preview.audition_resource(None);
        }
        if let Some(clip) = app.workspace.audition.take()
            && let Err(error) = preview.audition_resource(clip)
        {
            app.error = Some(format!("Could not play resource: {error:#}"));
        }
        if let Some(command) = app.playtest.command.take() {
            match command {
                playtest_ui::PlayCommand::Start(start) => {
                    if let Err(error) = preview.begin_playtest(start).await {
                        app.exit_playtest();
                        app.playtest.command = None;
                        preview.end_playtest();
                        app.error = Some(format!("Could not start Playtest: {error:#}"));
                    }
                }
                playtest_ui::PlayCommand::Exit => preview.end_playtest(),
            }
        }
        if app.playtest.active() {
            let r = app.preview.rect;
            let p = app.pixels_per_point;
            preview.set_playtest_viewport((
                (r.left() * p).round() as i32,
                (screen_height() - r.bottom() * p).round() as i32,
                (r.width() * p).round() as i32,
                (r.height() * p).round() as i32,
            ));
        }
        if let Some(point) = app.preview.pick.take() {
            let radius = [
                12. / app.preview.rect.width(),
                12. / app.preview.rect.height(),
            ];
            match preview.pick_note(point, radius) {
                Ok(Some(id)) => app.go_to_note(id),
                Ok(None) => {}
                Err(error) => app.error = Some(format!("Could not pick preview note: {error:#}")),
            }
        }
        let key = (
            app.session,
            app.editor.revision(),
            app.file.clone(),
            app.asset_revision,
        );
        let chart_changed = key != rendered;
        let seek = before_ui != app.transport.beat;
        if chart_changed {
            if key.0 != rendered.0 || key.3 != rendered.3 {
                preview.invalidate_resources();
            }
            app.render_error = preview
                .replace_chart(
                    app.editor.document(),
                    app.asset_root(),
                    &app.project_options,
                )
                .await
                .err()
                .map(|e| format!("{e:#}"));
            app.resource_warning = preview.resource_warning().map(str::to_owned);
            app.note_highlights = preview.highlighted_notes();
            rendered = key;
        }
        if let Err(error) = app.sync_noise_preview(&mut preview, &mut noise_override, chart_changed)
        {
            app.error = Some(format!("Could not update field Preview: {error}"));
        }
        app.audio_guide.generation_requested = app.audio_notes.show;
        app.audio_guide.update(
            app.session,
            app.asset_revision,
            app.project_options.music.as_deref(),
            preview.analysis_music(),
            &app.options.audio_guide,
        );
        app.transport.music_duration = preview.music_duration();
        if let Err(error) = preview.set_editor_options(&app.options, app.line) {
            app.error = Some(format!("Could not configure preview: {error:#}"));
        }
        if let Some((reference, kind)) = app.inspector.audition.take()
            && let Err(error) = preview.audition(reference.as_deref(), kind)
        {
            app.error = Some(format!("Could not play hitsound: {error:#}"));
        }
        if let Err(error) = preview.set_playback_rate(app.advanced.playback_rate) {
            app.error = Some(format!("Could not change playback speed: {error:#}"));
        }
        if let Err(error) =
            preview.set_audio_levels(app.transport.music_volume, app.transport.hitsound_volume)
        {
            app.error = Some(format!("Could not update audio levels: {error:#}"));
        }
        // Read the music clock after UI work unless it supplied a new cursor.
        if app.transport.playing
            && !app.is_scrubbing()
            && !seek
            && !chart_changed
            && app.render_error.is_none()
        {
            match preview.advance_transport() {
                Ok(seconds) => app.transport.beat = app.editor.document().tempo().beat_at(seconds),
                Err(error) => {
                    app.error = Some(format!("Audio playback failed: {error:#}"));
                    app.transport.playing = false;
                }
            }
        }
        let seconds = app.editor.document().tempo().seconds_at(app.transport.beat);
        if let Err(error) = preview.set_playback(
            seconds,
            app.transport.playing && !app.is_scrubbing() && app.render_error.is_none(),
            seek,
        ) {
            app.error = Some(format!("Audio playback failed: {error:#}"));
            app.transport.playing = false;
        }
        if let Err(error) =
            app.audition_audio_notes(seconds, seek, |kind| preview.audition(None, kind))
        {
            app.error = Some(format!("Could not play hitsound: {error:#}"));
        }
        if !app.show_projects && app.render_error.is_none() {
            preview.render(
                app.editor.document().tempo().seconds_at(app.transport.beat),
                preview.target_viewport(),
            );
        }
        if app.advanced.monitor || app.advanced.line_filter == 1 {
            app.advanced.line_states = preview.line_states();
        }
        if app.advanced.line_filter == 2 && (frame.is_multiple_of(12) || seek) {
            match preview.visible_note_lines() {
                Ok(lines) => app.advanced.visible_lines = lines,
                Err(e) => app.error = Some(format!("Could not inspect visible notes: {e:#}")),
            }
        }
        app.preview.exit_bounds = preview.hud_pause_bounds();
        if let Some(report) = preview.playtest_report() {
            app.playtest.report = report;
        }
        app.update_movie(Some(&preview)).await;
        gui.draw();
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        if let Some(smoke) = &mut playback_smoke
            && smoke.update(&mut app, &mut preview)?
        {
            break;
        }
        if let Some(directory) = &options.smoke
            && smoke::step(
                &mut app,
                &mut preview,
                directory,
                frame,
                &smoke::Baseline {
                    initial_lines,
                    initial_notes,
                    initial_line0,
                    bundled_demo,
                },
            )?
        {
            break;
        }
        frame += 1;
        next_frame().await;
    }
    files
        .checkpoint(&app, app.close_confirmed)
        .map_err(anyhow::Error::msg)?;
    Ok(())
}
