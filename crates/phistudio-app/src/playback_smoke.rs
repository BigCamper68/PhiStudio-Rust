//! Real-time integration probe used by tools/playback-smoke.py, never on Android.
use crate::workbench::Workbench;
use anyhow::{Result, ensure};
use macroquad::prelude::*;
use phistudio_prpr::Preview;
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub struct Smoke {
    directory: PathBuf,
    stage: u8,
    frames: usize,
    checkpoint: usize,
    since: f64,
    paused_at: f64,
    events: Vec<serde_json::Value>,
    images: Vec<(String, Image)>,
    router: crate::touch::TouchRouter,
    pending_touch: Option<(egui::TouchPhase, egui::Pos2)>,
    scrub_to: egui::Pos2,
    scrub_resumed: bool,
}
impl Smoke {
    pub fn new(directory: &Path) -> Result<Self> {
        std::fs::create_dir_all(directory)?;
        let result = directory.join("result.txt");
        if result.exists() {
            std::fs::remove_file(result)?;
        }
        Ok(Self {
            directory: directory.to_owned(),
            stage: 0,
            frames: 0,
            checkpoint: 0,
            since: get_time(),
            paused_at: 0.,
            events: Vec::new(),
            images: Vec::new(),
            router: Default::default(),
            pending_touch: None,
            scrub_to: egui::Pos2::ZERO,
            scrub_resumed: false,
        })
    }
    pub fn take_input(&mut self, app: &Workbench) -> (Vec<egui::Event>, bool) {
        if let Some((phase, position)) = self.pending_touch.take() {
            if phase == egui::TouchPhase::Start {
                let target = app.editor.document().tempo().beat_at(2.25);
                self.scrub_to = position + egui::vec2(0., (target - app.beat) as f32 * app.zoom);
            }
            self.router.event(phase, 1, position)
        } else {
            (Vec::new(), false)
        }
    }
    fn event(&mut self, name: &str, preview: &Preview) -> Result<()> {
        let report = preview.playback_report();
        self.events.push(serde_json::json!({"event":name, "wall_time":get_time(),
            "unix_seconds":SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64(), "playback":report}));
        std::fs::write(
            self.directory.join("events.json"),
            serde_json::to_vec_pretty(&self.events)?,
        )?;
        self.images.push((name.into(), preview.snapshot()));
        Ok(())
    }
    pub fn update(&mut self, app: &mut Workbench, preview: &mut Preview) -> Result<bool> {
        ensure!(
            app.render_error.is_none() && app.error.is_none(),
            "Playback setup failed: {:?} {:?}",
            app.error,
            app.render_error
        );
        let report = preview.playback_report();
        ensure!(
            report["audio_available"] == true && report["transport"]["music"].is_object(),
            "No real music output: {report}"
        );
        let seconds = app.editor.document().tempo().seconds_at(app.beat);
        let now = get_time();
        match self.stage {
            0 => {
                self.frames += 1;
                if self.frames == 1 {
                    app.advanced.playback_rate = std::env::var("PHISTUDIO_PROBE_RATE")
                        .ok()
                        .map(|v| v.parse())
                        .transpose()?
                        .unwrap_or(1.);
                }
                if self.frames == 8 {
                    self.event("start", preview)?;
                    app.beat = 0.;
                    app.playing = true;
                    preview.set_playback(0., true, true)?;
                    self.stage = 1;
                    self.since = now;
                }
            }
            1 => {
                let checkpoints = [
                    (0.3, 0, 0),
                    (0.6, 1, 0),
                    (1.1, 2, 0),
                    (1.6, 3, 0),
                    (2.2, 3, 1),
                    (3.2, 4, 0),
                    (3.8, 5, 0),
                ];
                let (at, combo, holding) = checkpoints[self.checkpoint];
                if seconds >= at {
                    ensure!(
                        seconds < at + 0.35,
                        "Playback skipped a verification interval: {seconds}"
                    );
                    ensure!(
                        report["combo"] == combo
                            && report["holding"] == holding
                            && report["counts"] == serde_json::json!([combo, 0, 0, 0]),
                        "Unexpected original Judge state: {report}"
                    );
                    self.event(&format!("play-{}", self.checkpoint), preview)?;
                    self.checkpoint += 1;
                    if self.checkpoint == checkpoints.len() {
                        let image = preview.snapshot();
                        let cyan = image
                            .bytes
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .filter(|p| p[2] > 160 && p[1] > 130 && p[2] > p[0].saturating_add(20))
                            .count();
                        ensure!(
                            cyan == 0,
                            "Judged notes still render: {cyan} cyan texture pixels"
                        );
                        ensure!(
                            report["score"] == 1_000_000,
                            "Autoplay did not complete the chart: {report}"
                        );
                        self.event("suspend-callback", preview)?;
                        phistudio_prpr::suspend_audio();
                        // Prove the callback stops music while rendering is not
                        // running; Android can stop the Surface thread first.
                        std::thread::sleep(Duration::from_millis(350));
                        app.playing = false;
                        preview.set_playback(seconds, false, false)?;
                        self.paused_at = seconds;
                        self.since = get_time();
                        self.stage = 2;
                        self.event("paused", preview)?;
                    }
                }
            }
            2 => {
                ensure!(seconds == self.paused_at, "Paused chart cursor moved");
                if now - self.since > 0.8 {
                    self.event("pause-end", preview)?;
                    app.beat = app.editor.document().tempo().beat_at(2.25);
                    preview.set_playback(2.25, false, true)?;
                    self.since = now;
                    self.stage = 3;
                }
            }
            3 => {
                ensure!(
                    report["combo"] == 3 && report["holding"] == 1,
                    "Mid-Hold seek lost state: {report}"
                );
                if self.frames == 8 {
                    self.event("seek-paused", preview)?;
                    self.frames += 1;
                }
                if now - self.since > 0.6 {
                    self.event("resume", preview)?;
                    app.playing = true;
                    preview.set_playback(2.25, true, false)?;
                    self.stage = 4;
                }
            }
            4 => {
                ensure!(
                    report["combo"] == 3 && report["holding"] == 1,
                    "Resume or fake note changed judgement: {report}"
                );
                if seconds >= 2.7 {
                    self.event("fake-passed", preview)?;
                    app.beat = app.editor.document().tempo().beat_at(3.9);
                    preview.set_playback(3.9, true, true)?;
                    self.stage = 5;
                    self.since = now;
                }
            }
            5 => {
                ensure!(
                    report["combo"] == 5 && report["holding"] == 0 && report["score"] == 1_000_000,
                    "Forward seek lost score: {report}"
                );
                if now - self.since > 0.4 {
                    self.event("forward-seek-end", preview)?;
                    app.tool = crate::workbench::Tool::Select;
                    let grid = crate::grid_input::geometry(app.grid_area, app);
                    self.pending_touch = Some((
                        egui::TouchPhase::Start,
                        egui::pos2(grid.rect.center().x, grid.rect.bottom() - 30.),
                    ));
                    self.stage = 6;
                    self.since = now;
                }
            }
            6 => {
                ensure!(
                    app.playing && app.grid_input.is_scrubbing() && report["playing"] == false,
                    "A playback scrub did not pause the transport: {report}"
                );
                if now - self.since > 0.2 {
                    self.event("scrub-held", preview)?;
                    self.pending_touch = Some((egui::TouchPhase::Move, self.scrub_to));
                    self.stage = 7;
                    self.since = now;
                }
            }
            7 => {
                ensure!(
                    app.playing
                        && app.grid_input.is_scrubbing()
                        && report["playing"] == false
                        && (seconds - 2.25).abs() < 1e-6,
                    "Finger scrubbing lost the selected time: {report}"
                );
                if now - self.since > 0.3 {
                    self.event("scrub-position", preview)?;
                    // A newer release coordinate must override the last move.
                    let delta = app.editor.document().tempo().beat_at(2.3)
                        - app.editor.document().tempo().beat_at(2.25);
                    self.pending_touch = Some((
                        egui::TouchPhase::End,
                        self.scrub_to + egui::vec2(0., delta as f32 * app.zoom),
                    ));
                    self.stage = 8;
                    self.since = now;
                }
            }
            8 => {
                ensure!(
                    app.playing
                        && !app.grid_input.is_scrubbing()
                        && report["playing"] == true
                        && seconds >= 2.3 - 1e-6,
                    "Finger release did not resume at the selected time: {report}"
                );
                if !self.scrub_resumed {
                    self.event("scrub-resumed", preview)?;
                    self.scrub_resumed = true;
                }
                if now - self.since > 0.3 {
                    ensure!(
                        seconds > 2.4
                            && seconds < 2.85
                            && report["combo"] == 3
                            && report["holding"] == 1,
                        "Playback did not advance from the released cursor: {report}"
                    );
                    self.event("scrub-resumed-end", preview)?;
                    app.playing = false;
                    preview.set_playback(seconds, false, false)?;
                    for (name, image) in &self.images {
                        image.export_png(
                            self.directory.join(format!("{name}.png")).to_str().unwrap(),
                        );
                    }
                    std::fs::write(
                        self.directory.join("result.txt"),
                        "PASS\nReal audio output, shared transport and original prpr autoplay\nTap/Flick/Drag/Hold, combo/score, fake notes and removal of judged textures\nPause, direct suspend callback, rewind into a Hold, resume and forward seek\nTouchRouter and egui finger scrub pauses while held and resumes at the final release coordinate\n",
                    )?;
                    return Ok(true);
                }
            }
            _ => unreachable!(),
        }
        ensure!(
            now - self.since < 15.,
            "Playback verification timed out: {report}"
        );
        Ok(false)
    }
}
