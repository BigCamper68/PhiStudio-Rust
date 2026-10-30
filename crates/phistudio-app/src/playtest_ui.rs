//! Practice controls; hit judgement stays in original prpr.
use crate::workbench::Workbench;
use egui::{self, Align2, FontId, Rect};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Countdown,
    Playing,
    Paused,
    Results,
}
pub(crate) enum PlayCommand {
    Start(f64),
    Exit,
}
struct Session {
    phase: Phase,
    start_beat: f64,
    return_beat: f64,
    deadline: Option<f64>,
}
#[derive(Default)]
pub(crate) struct PlaytestState {
    pub setup: bool,
    session: Option<Session>,
    pub command: Option<PlayCommand>,
    pub report: phistudio_prpr::PlaytestReport,
}
impl PlaytestState {
    pub fn active(&self) -> bool {
        self.session.is_some()
    }
}
impl Workbench {
    fn start_playtest(&mut self) {
        if !self.flush_shader_draft() {
            return;
        }
        let return_beat = self.beat;
        let start_beat = if self.options.playtest_from_cursor {
            self.beat
        } else {
            0.
        };
        self.playtest.session = Some(Session {
            phase: Phase::Countdown,
            start_beat,
            return_beat,
            deadline: None,
        });
        self.playtest.report = Default::default();
        self.playtest.setup = false;
        self.playtest.command = Some(PlayCommand::Start(
            self.editor.document().tempo().seconds_at(start_beat),
        ));
        self.preview.fullscreen = false;
        self.beat = start_beat;
        self.playing = false;
        self.drag = None;
        self.grid_input.cancel();
        self.timeline_scrub = false;
    }
    pub(crate) fn pause_playtest(&mut self) {
        if let Some(session) = &mut self.playtest.session
            && session.phase != Phase::Results
        {
            session.phase = Phase::Paused;
            session.deadline = None;
            self.playing = false;
        }
    }
    fn retry_playtest(&mut self) {
        if let Some(session) = &mut self.playtest.session {
            session.phase = Phase::Countdown;
            session.deadline = None;
            self.beat = session.start_beat;
            self.playtest.command = Some(PlayCommand::Start(
                self.editor
                    .document()
                    .tempo()
                    .seconds_at(session.start_beat),
            ));
            self.playtest.report = Default::default();
            self.playing = false;
        }
    }
    pub(crate) fn exit_playtest(&mut self) {
        if let Some(session) = self.playtest.session.take() {
            self.beat = session.return_beat;
        }
        self.playtest.command = Some(PlayCommand::Exit);
        self.playing = false;
        self.drag = None;
        self.grid_input.cancel();
        self.timeline_scrub = false;
    }
    pub(crate) fn playtest_back(&mut self) -> bool {
        if let Some(session) = &self.playtest.session {
            if matches!(session.phase, Phase::Paused | Phase::Results) {
                self.exit_playtest();
            } else {
                self.pause_playtest();
            }
            true
        } else {
            false
        }
    }
    pub(crate) fn playtest_setup(&mut self, ctx: &egui::Context) {
        if !self.playtest.setup || self.show_projects {
            return;
        }
        let mut open = true;
        crate::i18n::window("Playtest").open(&mut open).collapsible(false).resizable(false)
            .anchor(Align2::CENTER_CENTER, [0., 0.]).default_width(330.).show(ctx, |ui| {
                ui.label(crate::i18n::tr("Play the chart with Phira's original judgement and hit effects."));
                ui.checkbox(&mut self.options.playtest_from_cursor, crate::i18n::tr("Start from current cursor"));
                ui.add(egui::Slider::new(&mut self.options.playtest_countdown, 1..=3).text(crate::i18n::tr("Countdown (seconds)")));
                ui.small(crate::i18n::tr("Tap, hold, drag and flick on the fullscreen chart. Tap the HUD pause area to pause."));
                if ui.add_enabled(!self.file_busy && self.editor.document().note_count() > 0, egui::Button::new(crate::i18n::tr("Start Playtest"))).clicked() { self.start_playtest(); }
            });
        if !open {
            self.playtest.setup = false;
        }
    }
    pub(crate) fn playtest_ui(&mut self, ctx: &egui::Context) {
        self.grid_area = Rect::NOTHING;
        self.pixels_per_point = ctx.pixels_per_point();
        if self.close_requested {
            self.exit_playtest();
            return;
        }
        let now = ctx.input(|i| i.time);
        // The editor cancels its synthetic pointer when a second finger joins.
        // Gameplay keeps every raw finger; only an actual OS touch cancellation
        // pauses here. Android/iOS suspension also pauses explicitly in main.
        if ctx.input(|i| {
            i.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Touch {
                        phase: egui::TouchPhase::Cancel,
                        ..
                    }
                )
            })
        }) && self
            .playtest
            .session
            .as_ref()
            .is_some_and(|s| s.phase == Phase::Playing)
        {
            self.pause_playtest();
        }
        if let Some(session) = &mut self.playtest.session {
            if session.phase == Phase::Playing && self.playtest.report.finished {
                session.phase = Phase::Results;
                self.playing = false;
            }
            if session.phase == Phase::Countdown {
                let until = session
                    .deadline
                    .get_or_insert(now + self.options.playtest_countdown as f64);
                if now >= *until {
                    session.phase = Phase::Playing;
                    self.playing = true;
                }
            }
        }
        let Some(session) = &self.playtest.session else {
            return;
        };
        let phase = session.phase;
        let remaining = session
            .deadline
            .map_or(self.options.playtest_countdown as f64, |until| {
                (until - now).max(0.)
            });
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.playtest_back();
        }
        if phase == Phase::Playing
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space))
        {
            self.pause_playtest();
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::BLACK))
            .show(ctx, |ui| {
                let area = ui.max_rect();
                let aspect = self.preview_aspect();
                let size = if area.width() / area.height() > aspect {
                    egui::vec2(area.height() * aspect, area.height())
                } else {
                    egui::vec2(area.width(), area.width() / aspect)
                };
                let rect = Rect::from_center_size(area.center(), size);
                self.paint_preview(ui, rect);
                let pause = self.fullscreen_exit_rect(rect);
                if phase == Phase::Playing {
                    let requested = ctx.input(|i| {
                        i.events.iter().any(|event| match event {
                            egui::Event::Touch {
                                phase: egui::TouchPhase::Start,
                                pos,
                                ..
                            } => pause.contains(*pos),
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed: true,
                                ..
                            } => pause.contains(*pos),
                            _ => false,
                        })
                    });
                    if requested {
                        self.pause_playtest();
                    }
                }
                if phase == Phase::Countdown {
                    ui.painter()
                        .rect_filled(rect, 0., egui::Color32::from_black_alpha(60));
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        format!("{:.0}", remaining.ceil().max(1.)),
                        FontId::proportional(64.),
                        egui::Color32::WHITE,
                    );
                }
            });
        let phase = self.playtest.session.as_ref().map(|s| s.phase);
        if matches!(phase, Some(Phase::Paused | Phase::Results)) {
            let finished = phase == Some(Phase::Results);
            crate::i18n::window(if finished {
                "Playtest results"
            } else {
                "Playtest paused"
            })
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0., 0.])
            .default_width(310.)
            .show(ctx, |ui| {
                let r = &self.playtest.report;
                use crate::scroll::TouchScroll as _;
                egui::ScrollArea::vertical()
                    .id_source("playtest-report-scroll")
                    .max_height((ctx.screen_rect().height() - 150.).max(100.))
                    .touch_show(ui, |ui| {
                        egui::Grid::new("playtest-results")
                            .num_columns(2)
                            .show(ui, |ui| {
                                for (label, value) in [
                                    ("Score", format!("{:07}", r.score)),
                                    ("Accuracy", format!("{:.2}%", r.accuracy * 100.)),
                                    ("Max combo", r.max_combo.to_string()),
                                    ("Perfect", r.counts[0].to_string()),
                                    ("Good", r.counts[1].to_string()),
                                    ("Bad", r.counts[2].to_string()),
                                    ("Miss", r.counts[3].to_string()),
                                    ("Early / Late", format!("{} / {}", r.early, r.late)),
                                ] {
                                    ui.label(crate::i18n::tr(label));
                                    ui.label(value);
                                    ui.end_row();
                                }
                            });
                    });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if !finished
                        && ui.button(crate::i18n::tr("Resume")).clicked()
                        && let Some(session) = &mut self.playtest.session
                    {
                        session.phase = Phase::Countdown;
                        session.deadline = None;
                    }
                    if ui.button(crate::i18n::tr("Retry")).clicked() {
                        self.retry_playtest();
                    }
                    if ui.button(crate::i18n::tr("Exit to editor")).clicked() {
                        self.exit_playtest();
                    }
                });
            });
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn two_and_three_finger_chords_keep_playing_but_real_touch_cancel_pauses() {
        use egui::TouchPhase;
        for size in [egui::vec2(873., 393.), egui::vec2(1194., 834.)] {
            for fingers in [2, 3] {
                let mut app = Workbench::new(
                    phistudio_core::ChartDocument::empty("Multitouch", 120.).unwrap(),
                    None,
                );
                app.start_playtest();
                app.playtest.session.as_mut().unwrap().phase = Phase::Playing;
                app.playing = true;
                let original = app.editor.document().as_json().clone();
                let ctx = egui::Context::default();
                let mut router = crate::touch::TouchRouter::default();
                let raw = |events| egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                };
                let _ = ctx.run(raw(vec![]), |ctx| app.ui(ctx, false));
                let mut events = vec![];
                let mut pointer_canceled = false;
                for id in 1..=fingers {
                    let (e, canceled) = router.event_with_context(
                        &ctx,
                        TouchPhase::Start,
                        id,
                        egui::pos2(size.x * (0.3 + id as f32 * 0.12), size.y * 0.65),
                        id as f64 * 0.01,
                    );
                    events.extend(e);
                    pointer_canceled |= canceled;
                }
                assert!(
                    pointer_canceled,
                    "This is the editor's second-finger pointer cancellation"
                );
                let _ = ctx.run(raw(events), |ctx| app.ui(ctx, pointer_canceled));
                assert!(app.playing, "A {fingers}-finger chord paused Playtest");
                assert!(
                    app.playtest
                        .session
                        .as_ref()
                        .is_some_and(|s| s.phase == Phase::Playing)
                );
                // Lift the first finger while the others remain down, then start
                // another chord. Pointer ownership must not become a HUD click.
                for phase in [TouchPhase::Move, TouchPhase::End] {
                    let (e, c) = router.event_with_context(
                        &ctx,
                        phase,
                        1,
                        egui::pos2(size.x * 0.5, size.y * 0.65),
                        0.1,
                    );
                    let _ = ctx.run(raw(e), |ctx| app.ui(ctx, c));
                    assert!(app.playing);
                }
                let (e, c) = router.event_with_context(
                    &ctx,
                    TouchPhase::Cancel,
                    2,
                    egui::pos2(size.x * 0.54, size.y * 0.65),
                    0.2,
                );
                let _ = ctx.run(raw(e), |ctx| app.ui(ctx, c));
                assert!(!app.playing, "OS touch cancellation must still pause");
                assert_eq!(app.editor.document().as_json(), &original);
                assert!(!app.editor.can_undo());
            }
        }
    }
    #[test]
    fn pause_and_result_actions_remain_visible_on_compact_displays() {
        for size in [egui::vec2(873., 393.), egui::vec2(1194., 834.)] {
            for phase in [Phase::Paused, Phase::Results] {
                let mut app = Workbench::new(
                    phistudio_core::ChartDocument::empty("Practice", 120.).unwrap(),
                    None,
                );
                app.start_playtest();
                app.playtest.session.as_mut().unwrap().phase = phase;
                let ctx = egui::Context::default();
                for _ in 0..3 {
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ctx| app.ui(ctx, false),
                    );
                    for shape in output.shapes {
                        if let egui::Shape::Text(t) = shape.1
                            && ["Resume", "Retry", "Exit to editor"]
                                .contains(&t.galley.job.text.as_str())
                        {
                            let rect = Rect::from_min_size(t.pos, t.galley.size());
                            assert!(ctx.screen_rect().contains_rect(rect), "{size:?}: {rect:?}");
                            assert!(shape.0.contains_rect(rect));
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn practice_controls_preserve_chart_history_and_restore_editor_cursor() {
        let mut app = Workbench::new(
            phistudio_core::ChartDocument::empty("Practice", 120.).unwrap(),
            None,
        );
        app.beat = 12.;
        app.options.playtest_from_cursor = true;
        let original = app.editor.document().as_json().clone();
        app.start_playtest();
        assert!(app.playtest.active());
        assert!(!app.playing);
        assert!(matches!(app.playtest.command, Some(PlayCommand::Start(6.))));
        app.pause_playtest();
        assert!(!app.playing);
        app.retry_playtest();
        assert_eq!(app.beat, 12.);
        app.beat = 19.;
        app.exit_playtest();
        assert_eq!(app.beat, 12.);
        assert!(!app.playtest.active());
        assert_eq!(app.editor.document().as_json(), &original);
        assert!(!app.editor.can_undo());
    }
}
