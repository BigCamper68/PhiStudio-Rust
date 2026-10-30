//! Interactive practice uses the unchanged native input pump, Judge and HUD.
use crate::{Preview, autoplay};
use anyhow::{Result, ensure};
use macroquad::prelude::*;
use prpr::{
    config::Mods,
    core::BadNote,
    judge::Judge,
    scene::{Main, Scene},
    time::TimeManager,
    ui::Ui,
};

struct InputScene;
impl Scene for InputScene {
    fn update(&mut self, _: &mut TimeManager) -> Result<()> {
        Ok(())
    }
    fn render(&mut self, _: &mut TimeManager, _: &mut Ui) -> Result<()> {
        Ok(())
    }
}
pub(crate) struct PlaySession {
    restored_fake: Vec<(usize, usize)>,
    pub bad_notes: Vec<BadNote>,
    end_time: f64,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlaytestReport {
    pub score: u32,
    pub accuracy: f64,
    pub combo: u32,
    pub max_combo: u32,
    pub total: u32,
    pub counts: [u32; 4],
    pub early: u32,
    pub late: u32,
    pub finished: bool,
}

impl Preview {
    /// Match the AP/FC indicator in the pinned Phira GameScene::update.
    pub(crate) fn update_playtest_line_color(&mut self) {
        let counts = self.judge.counts();
        self.resource.judge_line_color =
            if counts[2] + counts[3] == 0 && self.resource.config.ap_fc_indicator {
                if counts[1] == 0 {
                    self.resource.res_pack.info.color_perfect()
                } else {
                    self.resource.res_pack.info.color_good()
                }
            } else {
                WHITE
            };
        self.resource.judge_line_color.a *= self.resource.alpha;
    }
    pub async fn begin_playtest(&mut self, start: f64) -> Result<()> {
        ensure!(start.is_finite(), "Playtest start must be finite");
        self.end_playtest();
        ensure!(
            self.chart.lines.iter().any(|line| line
                .notes
                .iter()
                .any(|note| !note.fake && note.time >= start)),
            "No playable notes at or after this position"
        );
        if self.input_pump.is_none() {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            let icons = tempfile::tempdir()?;
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                for (name, data) in [
                    (
                        "info.png",
                        include_bytes!("../../../assets/runtime/info.png").as_slice(),
                    ),
                    (
                        "warn.png",
                        include_bytes!("../../../assets/runtime/warn.png").as_slice(),
                    ),
                    (
                        "ok.png",
                        include_bytes!("../../../assets/runtime/ok.png").as_slice(),
                    ),
                    (
                        "error.png",
                        include_bytes!("../../../assets/runtime/error.png").as_slice(),
                    ),
                ] {
                    std::fs::write(icons.path().join(name), data)?;
                }
                macroquad::file::set_pc_assets_folder(icons.path().to_str().unwrap());
            }
            #[cfg(any(target_os = "android", target_os = "ios"))]
            macroquad::file::set_pc_assets_folder("");
            let old_directory = std::env::current_dir()?;
            let pump = Main::new(Box::new(InputScene), TimeManager::new(1., false), None).await;
            // The pinned Macroquad iOS asset loader switches to the app bundle.
            let restore = std::env::set_current_dir(old_directory);
            macroquad::file::set_pc_assets_folder("");
            restore?;
            self.input_pump = Some(pump?);
        }
        self.chart.reset();
        let mut restored_fake = vec![];
        for (line_index, line) in self.chart.lines.iter_mut().enumerate() {
            for (index, note) in line.notes.iter_mut().enumerate() {
                if !note.fake && note.time < start {
                    restored_fake.push((line_index, index));
                    note.fake = true;
                    note.judge = prpr::judge::JudgeStatus::Judged;
                }
            }
        }
        self.judge = Judge::new(&self.chart);
        self.judge.advance_to(&mut self.chart, start);
        self.playtest = Some(PlaySession {
            restored_fake,
            bad_notes: vec![],
            end_time: self.chart_duration(),
        });
        self.resource.config.mods.remove(Mods::AUTOPLAY);
        self.resource.config.interactive = true;
        self.resource.config.use_keyboard = false;
        self.update_playtest_line_color();
        self.needs_seek = false;
        self.clear_particles = true;
        self.last_time = start;
        self.last_render_wall = get_time();
        Ok(())
    }
    pub fn end_playtest(&mut self) {
        if let Some(session) = self.playtest.take() {
            for (line, index) in session.restored_fake {
                if let Some(note) = self
                    .chart
                    .lines
                    .get_mut(line)
                    .and_then(|l| l.notes.get_mut(index))
                {
                    note.fake = false;
                }
            }
            self.resource.config.mods.insert(Mods::AUTOPLAY);
            self.resource.config.interactive = false;
            self.resource.judge_line_color = self.resource.res_pack.info.color_perfect();
            self.resource.judge_line_color.a *= self.resource.alpha;
            self.judge = Judge::new(&self.chart);
            autoplay::seek(&mut self.chart, &mut self.judge, self.last_time);
            self.needs_seek = true;
            self.clear_particles = true;
        }
    }
    /// Drain the original subscriber on every editor frame after registration,
    /// including pauses and Projects, so stale input never accumulates.
    pub fn pump_playtest_input(&mut self) -> Result<()> {
        if let Some(pump) = &mut self.input_pump {
            pump.update()?;
        }
        Ok(())
    }
    pub fn set_playtest_viewport(&mut self, viewport: (i32, i32, i32, i32)) {
        self.playtest_viewport = Some(viewport);
    }
    pub fn playtest_report(&self) -> Option<PlaytestReport> {
        let session = self.playtest.as_ref()?;
        let r = self.judge.result();
        Some(PlaytestReport {
            score: r.score,
            accuracy: r.accuracy,
            combo: self.judge.combo(),
            max_combo: r.max_combo,
            total: r.num_of_notes,
            counts: r.counts,
            early: r.early,
            late: r.late,
            // GameScene ends at the music boundary and its original outro delay.
            // Judging the final note is independent of reaching the chart end.
            finished: self.last_time > session.end_time + prpr::scene::GameScene::FADEOUT_TIME,
        })
    }
}
