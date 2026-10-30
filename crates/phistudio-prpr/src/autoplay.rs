//! Restore an editor cursor without replaying historical sounds or particles.
//! During playback, all judgement goes through the original prpr Judge::update.
use prpr::{
    core::{Chart, NoteKind},
    judge::{Judge, JudgeStatus, Judgement},
};

pub(crate) fn seek(chart: &mut Chart, judge: &mut Judge, time: f64) {
    chart.reset();
    judge.reset();
    for (line_id, line) in chart.lines.iter_mut().enumerate() {
        for (note_id, note) in line.notes.iter_mut().enumerate() {
            if note.fake || note.time >= time {
                continue;
            }
            let finished_at = match note.kind {
                NoteKind::Hold { end_time, .. } if end_time > time => {
                    note.judge = JudgeStatus::Hold(true, time, 0., false, f64::INFINITY);
                    continue;
                }
                NoteKind::Hold { end_time, .. } => end_time,
                _ => note.time,
            };
            note.judge = JudgeStatus::Judged;
            judge.commit(
                finished_at,
                Judgement::Perfect,
                line_id as u32,
                note_id as u32,
                0.,
            );
        }
    }
    for (line, (indices, start)) in chart.lines.iter().zip(&mut judge.notes) {
        *start = indices
            .iter()
            .take_while(|&&i| matches!(line.notes[i as usize].judge, JudgeStatus::Judged))
            .count();
    }
    judge.last_time = time;
    judge.judgements.borrow_mut().clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use phistudio_core::ChartDocument;
    use serde_json::json;

    #[test]
    fn seeks_restore_combo_and_active_holds_without_judging_fake_notes() {
        let mut root: serde_json::Value = serde_json::from_str(include_str!(
            "../../../assets/charts/phistudio-demo.rpe.json"
        ))
        .unwrap();
        let template = root["judgeLineList"][0]["notes"][0].clone();
        let notes: Vec<_> = [
            (1, 1, 1, 0),
            (3, 2, 2, 0),
            (4, 3, 3, 0),
            (2, 4, 6, 0),
            (1, 5, 5, 1),
            (1, 7, 7, 0),
        ]
        .into_iter()
        .map(|(kind, start, end, fake)| {
            let mut note = template.clone();
            note["type"] = json!(kind);
            note["startTime"] = json!([start, 0, 1]);
            note["endTime"] = json!([end, 0, 1]);
            note["isFake"] = json!(fake);
            note
        })
        .collect();
        root["judgeLineList"][0]["notes"] = json!(notes);
        root["BPMList"] = json!([{"bpm": 120., "startTime": [0,0,1]}]);
        let document = ChartDocument::from_value(root.clone()).unwrap();
        let mut fs = crate::assets::ChartAssets::new(None).unwrap();
        let mut chart = pollster::block_on(prpr::parse::parse_rpe(
            &document.render_json().unwrap(),
            &mut fs,
            Default::default(),
            false,
        ))
        .unwrap();
        let mut judge = Judge::new(&chart);
        for (time, count, holding) in [
            (0.25, 0, 0),
            (0.5, 0, 0),
            (0.50001, 1, 0),
            (1.6, 3, 0),
            (2.2, 3, 1),
            (3.7, 5, 0),
            (1.6, 3, 0),
            (2.2, 3, 1),
            (0., 0, 0),
        ] {
            seek(&mut chart, &mut judge, time);
            assert_eq!(judge.combo(), count);
            assert_eq!(judge.counts(), [count, 0, 0, 0]);
            assert_eq!(
                chart.lines[0]
                    .notes
                    .iter()
                    .filter(|n| matches!(n.judge, JudgeStatus::Hold(..)))
                    .count(),
                holding
            );
            assert!(
                chart.lines[0]
                    .notes
                    .iter()
                    .filter(|n| n.fake)
                    .all(|n| matches!(n.judge, JudgeStatus::NotJudged))
            );
            assert!(judge.judgements.borrow().is_empty());
        }
        assert_eq!(document.as_json(), &root);
    }
}
