use phistudio_core::{tools::*, *};
use serde_json::json;
fn spec() -> AudioNoteSpec {
    AudioNoteSpec {
        options: AudioNoteOptions::default(),
        range: [Beat::ZERO, Beat::new(20, 0, 1).unwrap()],
        lines: vec![0],
        audio_offset: 0.,
    }
}
fn markers(times: &[f64]) -> Vec<AudioMarker> {
    times
        .iter()
        .enumerate()
        .map(|(i, &seconds)| AudioMarker {
            seconds,
            strength: 0.7 + (i % 3) as f32 * 0.1,
            band: 1 + i % 3,
        })
        .collect()
}
fn preview(doc: &ChartDocument, m: &[AudioMarker], s: &AudioNoteSpec) -> AudioNotePreview {
    preview_audio_notes(doc, m, s, || false).unwrap()
}
#[test]
fn signed_offsets_bpm_changes_raw_precision_and_quantization_limits() {
    let mut raw = ChartDocument::empty("Timing", 120.)
        .unwrap()
        .as_json()
        .clone();
    raw["BPMList"] = json!([{"bpm":120,"startTime":[0,0,1]},{"bpm":60,"startTime":[2,0,1]}]);
    let doc = ChartDocument::from_value(raw).unwrap();
    let mut s = spec();
    s.options.quantize = false;
    s.options.collision = AudioCollision::Allow;
    s.options.shift_ms = 25.;
    for offset in [0.125, -0.25] {
        s.audio_offset = offset;
        for item in preview(&doc, &markers(&[0.25, 0.75, 1.35, 2.2]), &s).notes {
            // Match through native chart seconds rather than assuming one BPM.
            let source = doc.tempo().seconds_at(item.note.start_time.as_f64()) + offset - 0.025;
            assert!(
                [0.25, 0.75, 1.35, 2.2]
                    .iter()
                    .any(|v| (source - v).abs() < 1e-6),
                "{source}"
            );
        }
    }
    let doc = ChartDocument::empty("Snap", 120.).unwrap();
    s = spec();
    s.options.max_snap_ms = 30.;
    let m = [
        AudioMarker {
            seconds: 0.12,
            strength: 0.7,
            band: 1,
        },
        AudioMarker {
            seconds: 0.13,
            strength: 0.9,
            band: 3,
        },
        AudioMarker {
            seconds: 0.061,
            strength: 0.8,
            band: 2,
        },
    ];
    s.options.per_band = true;
    let p = preview(&doc, &m, &s);
    assert_eq!(p.notes.len(), 1);
    assert_eq!(p.notes[0].note.start_time, Beat::new(0, 1, 4).unwrap());
    assert_eq!(p.notes[0].note.kind, NoteKind::Flick);
    assert_eq!(p.stats.off_grid, 1);
    assert_eq!(p.stats.merged, 1);
    s.options.division = 3;
    s.options.max_snap_ms = 0.;
    let p = preview(&doc, &markers(&[1. / 6.]), &s);
    assert_eq!(p.notes[0].note.start_time, Beat::new(0, 1, 3).unwrap());
}
#[test]
fn raw_shapes_multi_line_generation_and_one_exact_sparse_undo() {
    let mut e = Editor::new(ChartDocument::empty("Sparse", 120.).unwrap());
    e.execute(Command::AddLines { count: 1 }).unwrap();
    let mut raw = e.document().as_json().clone();
    raw["judgeLineList"][0]["notes"] = json!(null);
    raw["judgeLineList"][0]["numOfNotes"] = json!(999);
    raw["judgeLineList"][1]
        .as_object_mut()
        .unwrap()
        .shift_remove("notes");
    raw["judgeLineList"][1]
        .as_object_mut()
        .unwrap()
        .shift_remove("numOfNotes");
    raw["extension"] = json!({"keep":[null,1]});
    let mut e = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
    e.mark_saved();
    let mut s = spec();
    s.lines = vec![1, 0];
    s.options.position = AudioPosition::PingPong;
    s.options.lanes = 3;
    s.options.side = AudioSide::Alternating;
    let p = preview(e.document(), &markers(&[0., 0.5, 1., 1.5, 2.]), &s);
    assert_eq!(
        p.notes.iter().map(|n| n.line).collect::<Vec<_>>(),
        vec![1, 0, 1, 0, 1]
    );
    assert_eq!(
        p.notes.iter().map(|n| n.note.x).collect::<Vec<_>>(),
        vec![-450., 0., 450., 0., -450.]
    );
    assert!(!e.is_dirty());
    assert_eq!(e.document().as_json(), &raw);
    let selected = insert_audio_notes(e.document(), &p.notes)
        .unwrap()
        .apply(&mut e)
        .unwrap()
        .unwrap();
    assert_eq!(selected.notes.len(), 5);
    assert_eq!(e.document().note_count(), 5);
    assert_eq!(e.undo_label(), Some("Generate notes from audio markers"));
    let after = e.document().as_json().clone();
    assert!(e.undo());
    assert_eq!(e.document().as_json(), &raw);
    assert!(!e.is_dirty());
    assert!(e.redo());
    assert_eq!(e.document().as_json(), &after);
    assert_eq!(
        preview(e.document(), &markers(&[0., 0.5, 1., 1.5, 2.]), &s)
            .notes
            .len(),
        0
    );
}
#[test]
fn hold_intervals_raw_source_order_positions_sides_and_touching_tails() {
    let mut e = Editor::new(ChartDocument::empty("Overlap", 120.).unwrap());
    for (start, end, x, above) in [
        (8., 10., 0., 1),
        (0., 4., 0., 1),
        (2., 2., 400., 1),
        (1., 1., 0., 0),
    ] {
        e.execute(Command::AddNote {
            line: 0,
            note: Note {
                kind: if end > start {
                    NoteKind::Hold
                } else {
                    NoteKind::Tap
                },
                start_time: Beat::from_grid(start, 4).unwrap(),
                end_time: Beat::from_grid(end, 4).unwrap(),
                x,
                above,
                ..Default::default()
            },
        })
        .unwrap();
    }
    let mut s = spec();
    let m = markers(&[0.5, 1., 2., 2.5, 4.5, 5.]);
    let p = preview(e.document(), &m, &s);
    assert_eq!(
        p.notes
            .iter()
            .map(|n| n.note.start_time.as_f64())
            .collect::<Vec<_>>(),
        vec![4., 5., 10.]
    );
    s.options.x = 400.;
    assert!(preview(e.document(), &m, &s).notes.len() > p.notes.len());
    s.options.collision = AudioCollision::Line;
    assert_eq!(preview(e.document(), &m, &s).notes.len(), p.notes.len());
    s = spec();
    s.options.kind = NoteKind::Hold;
    s.options.hold_beats = 4.;
    let empty = ChartDocument::empty("Holds", 120.).unwrap();
    let p = preview(&empty, &markers(&[0., 0.5, 1.]), &s);
    assert_eq!(p.notes.len(), 3);
    assert_eq!(p.notes[0].note.end_time, Beat::ONE);
    assert_eq!(p.notes[1].note.end_time, Beat::new(2, 0, 1).unwrap());
    s.options.trim_holds = false;
    assert_eq!(preview(&empty, &markers(&[0., 0.5, 1.]), &s).notes.len(), 1);
}
#[test]
fn salience_thinning_limits_random_seed_and_input_rejection_are_deterministic() {
    let doc = ChartDocument::empty("Density", 120.).unwrap();
    let mut s = spec();
    s.options.position = AudioPosition::Random;
    s.options.seed = 87;
    s.options.max_notes = 2;
    s.options.minimum_gap_beats = 0.5;
    let m = [
        AudioMarker {
            seconds: 0.,
            strength: 0.1,
            band: 1,
        },
        AudioMarker {
            seconds: 0.125,
            strength: 0.95,
            band: 2,
        },
        AudioMarker {
            seconds: 0.25,
            strength: 0.5,
            band: 3,
        },
        AudioMarker {
            seconds: 0.5,
            strength: 0.6,
            band: 1,
        },
    ];
    let p = preview(&doc, &m, &s);
    assert_eq!(
        p.notes
            .iter()
            .map(|n| n.note.start_time.as_f64())
            .collect::<Vec<_>>(),
        vec![0.25, 1.]
    );
    let again = preview(&doc, &m, &s);
    assert!(
        p.notes
            .iter()
            .zip(&again.notes)
            .all(|(a, b)| a.note == b.note)
    );
    assert!(preview_audio_notes(&doc, &m, &s, || true).is_err());
    for seconds in [f64::NAN, -1., f64::INFINITY] {
        assert!(
            preview_audio_notes(
                &doc,
                &[AudioMarker {
                    seconds,
                    strength: 0.5,
                    band: 1
                }],
                &s,
                || false
            )
            .is_err()
        );
    }
    let mut invalid = s.clone();
    invalid.options.speed = 0.;
    assert!(preview_audio_notes(&doc, &m, &invalid, || false).is_err());
    invalid = s.clone();
    invalid.lines = vec![0, 0];
    assert!(preview_audio_notes(&doc, &m, &invalid, || false).is_err());
    assert_eq!(preview(&doc, &[], &s).notes.len(), 0);
    let options: EditorOptions = serde_json::from_str("{}").unwrap();
    assert_eq!(options.audio_notes, AudioNoteOptions::default());
}

#[test]
fn distant_long_holds_and_neighbor_bucket_boundaries_are_handled_without_false_collisions() {
    let mut e = Editor::new(ChartDocument::empty("Spatial index", 120.).unwrap());
    for i in 0..2000 {
        e.execute(Command::AddNote {
            line: 0,
            note: Note {
                kind: NoteKind::Hold,
                start_time: Beat::from_grid((i % 30) as f64, 4).unwrap(),
                end_time: Beat::new(2000, 0, 1).unwrap(),
                x: if i % 2 == 0 { -450. } else { 450. },
                ..Default::default()
            },
        })
        .unwrap();
    }
    let mut s = spec();
    s.range[1] = Beat::new(1000, 0, 1).unwrap();
    let m = markers(&(0..200).map(|i| i as f64 * 0.5 + 0.25).collect::<Vec<_>>());
    assert_eq!(preview(e.document(), &m, &s).notes.len(), 200);
    e.execute(Command::AddNote {
        line: 0,
        note: Note {
            kind: NoteKind::Hold,
            end_time: Beat::new(2000, 0, 1).unwrap(),
            x: 40.,
            ..Default::default()
        },
    })
    .unwrap();
    assert_eq!(preview(e.document(), &m, &s).notes.len(), 0);
    let mut n = e.document().note(0, 2000).unwrap();
    n.x = 40.00001;
    e.execute(Command::UpdateNote {
        id: NoteId {
            line: 0,
            index: 2000,
        },
        note: n,
    })
    .unwrap();
    assert_eq!(preview(e.document(), &m, &s).notes.len(), 200);
}
