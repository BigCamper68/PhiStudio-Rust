//! Audition replaces only the native sound cue; render/judgment fields are retained.
use crate::Preview;
use prpr::{core::Chart, judge::HitSound};

#[derive(Default)]
pub(crate) struct HitsoundMask {
    selected: Option<Vec<usize>>,
    muted: Vec<(usize, usize, HitSound)>,
}
impl HitsoundMask {
    fn apply(&mut self, chart: &mut Chart, lines: Option<&[usize]>) {
        if self.selected.as_deref() == lines {
            return;
        }
        let mut next = lines.map(<[usize]>::to_vec);
        if let Some(lines) = &mut next {
            lines.sort_unstable();
            lines.dedup();
        }
        if self.selected == next {
            return;
        }
        for (line, index, sound) in self.muted.drain(..) {
            if let Some(note) = chart
                .lines
                .get_mut(line)
                .and_then(|l| l.notes.get_mut(index))
            {
                note.hitsound = sound;
            }
        }
        if let Some(selected) = &next {
            for (line_index, line) in chart.lines.iter_mut().enumerate() {
                if selected.binary_search(&line_index).is_ok() {
                    continue;
                }
                for (note_index, note) in line.notes.iter_mut().enumerate() {
                    let sound = std::mem::replace(&mut note.hitsound, HitSound::None);
                    self.muted.push((line_index, note_index, sound));
                }
            }
        }
        self.selected = next;
    }
}
impl Preview {
    /// Preview/playtest audition only; source charts and movie scenes stay intact.
    pub fn set_hitsound_lines(&mut self, lines: Option<&[usize]>) {
        self.hitsound_audition.apply(&mut self.chart, lines);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phistudio_core::{ChartDocument, Command, Editor, Note};

    #[test]
    fn native_sound_mask_preserves_visual_notes_and_restores_custom_and_builtin_cues() {
        let mut editor = Editor::new(ChartDocument::empty("Audition", 120.).unwrap());
        editor.execute(Command::AddLines { count: 2 }).unwrap();
        for line in 0..3 {
            editor
                .execute(Command::AddNote {
                    line,
                    note: Note::default(),
                })
                .unwrap();
        }
        let source = editor.document().as_json().clone();
        let mut fs = crate::assets::ChartAssets::new(None).unwrap();
        let mut chart = pollster::block_on(prpr::parse::parse_rpe(
            &source.to_string(),
            &mut fs,
            prpr::core::ChartExtra::default(),
            true,
        ))
        .unwrap();
        chart.lines[1].notes[0].hitsound = HitSound::Flick;
        chart.lines[2].notes[0].hitsound = HitSound::Custom("original.wav".into());
        let visuals: Vec<_> = chart
            .lines
            .iter()
            .map(|l| {
                let n = &l.notes[0];
                (
                    n.time,
                    n.height,
                    n.speed,
                    n.above,
                    n.fake,
                    n.object.translation.0.now(),
                )
            })
            .collect();
        let mut mask = HitsoundMask::default();
        mask.apply(&mut chart, Some(&[1]));
        assert!(matches!(chart.lines[0].notes[0].hitsound, HitSound::None));
        assert!(matches!(chart.lines[1].notes[0].hitsound, HitSound::Flick));
        assert!(matches!(chart.lines[2].notes[0].hitsound, HitSound::None));
        mask.apply(&mut chart, Some(&[2, 0, 2]));
        assert!(matches!(chart.lines[0].notes[0].hitsound, HitSound::Click));
        assert!(matches!(chart.lines[1].notes[0].hitsound, HitSound::None));
        assert!(
            matches!(&chart.lines[2].notes[0].hitsound,HitSound::Custom(s) if s=="original.wav")
        );
        mask.apply(&mut chart, Some(&[]));
        assert!(
            chart
                .lines
                .iter()
                .all(|l| matches!(l.notes[0].hitsound, HitSound::None))
        );
        mask.apply(&mut chart, None);
        assert!(matches!(chart.lines[0].notes[0].hitsound, HitSound::Click));
        assert!(matches!(chart.lines[1].notes[0].hitsound, HitSound::Flick));
        assert!(
            matches!(&chart.lines[2].notes[0].hitsound,HitSound::Custom(s) if s=="original.wav")
        );
        for (line, expected) in chart.lines.iter().zip(visuals) {
            let n = &line.notes[0];
            assert_eq!(
                (
                    n.time,
                    n.height,
                    n.speed,
                    n.above,
                    n.fake,
                    n.object.translation.0.now()
                ),
                expected
            );
        }
        assert_eq!(editor.document().as_json(), &source);
    }
}
