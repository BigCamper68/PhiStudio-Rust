//! Optional sound timing keeps the original Judge/effects and original SFX player.
use crate::Preview;
#[derive(Default)]
pub(crate) struct SoundSchedule {
    pub due: Vec<(f64, usize, usize)>,
    pub cursor: usize,
    pub ready: bool,
}
impl Preview {
    pub(crate) fn rebuild_sound_schedule(&mut self) {
        if self.editor_options.hitsound_delay_ms == 0. && self.editor_options.drag_delay_ms == 0. {
            self.sound_schedule = SoundSchedule {
                ready: true,
                ..Default::default()
            };
            return;
        }
        let mut due = Vec::new();
        for (l, line) in self.chart.lines.iter().enumerate() {
            for (i, n) in line.notes.iter().enumerate() {
                if n.fake {
                    continue;
                }
                let delay = if matches!(n.kind, prpr::core::NoteKind::Drag) {
                    self.editor_options.drag_delay_ms
                } else {
                    self.editor_options.hitsound_delay_ms
                };
                due.push((n.time + f64::from(delay) / 1000., l, i));
            }
        }
        due.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.sound_schedule = SoundSchedule {
            cursor: due.partition_point(|(t, _, _)| *t <= self.last_time),
            due,
            ready: true,
        };
    }
    pub(crate) fn seek_sound_schedule(&mut self, time: f64) {
        self.sound_schedule.cursor = self
            .sound_schedule
            .due
            .partition_point(|(t, _, _)| *t <= time);
    }
}
pub(crate) fn play_due(
    schedule: &mut SoundSchedule,
    chart: &prpr::core::Chart,
    resource: &mut prpr::core::Resource,
    time: f64,
) {
    while let Some((at, line, note)) = schedule.due.get(schedule.cursor).copied() {
        if at > time {
            break;
        }
        schedule.cursor += 1;
        chart.lines[line].notes[note].hitsound.play(resource);
    }
}
