use super::*;

#[test]
fn split_uses_seconds_across_bpm_changes_and_join_preserves_linear_motion() {
    let mut root = ChartDocument::empty("Split", 120.)
        .unwrap()
        .as_json()
        .clone();
    root["BPMList"] = json!([{"bpm":120.,"startTime":[0,0,1]},{"bpm":60.,"startTime":[2,0,1]}]);
    root["judgeLineList"][0]["eventLayers"][0]["rotateEvents"] =
        json!([{"startTime":[0,0,1],"endTime":[4,0,1],"start":0.,"end":300.,"custom":"split"}]);
    let mut editor = Editor::new(ChartDocument::from_value(root).unwrap());
    let before = editor.document().as_json().clone();
    let selection = Selection {
        notes: vec![],
        events: vec![EventId {
            line: 0,
            layer: 0,
            kind: EventType::Rotate,
            index: 0,
        }],
    };
    let edit = tools::subdivide_events(
        editor.document(),
        &selection,
        2,
        None,
        |_, e, left, right| {
            Ok(LineEvent {
                start: e.start + (e.end - e.start) * left,
                end: e.start + (e.end - e.start) * right,
                ..e.clone()
            })
        },
    )
    .unwrap();
    let selected = apply(&mut editor, edit);
    let first = editor.document().event(selected.events[0]).unwrap();
    assert!((first.end - 100.).abs() < 1e-8);
    let edit = tools::join_events(editor.document(), &selected).unwrap();
    apply(&mut editor, edit);
    assert_eq!(
        editor
            .document()
            .event_values(0, 0, EventType::Rotate)
            .unwrap()
            .len(),
        1
    );
    editor.undo();
    editor.undo();
    assert_eq!(editor.document().as_json(), &before);
}

#[test]
fn generators_use_exact_rational_steps_endpoints_and_seeded_randomness() {
    let mut spec = pattern();
    let notes = spec.notes(|t| t).unwrap();
    assert_eq!(notes.len(), 7);
    assert_eq!(notes[0].start_time, beat(2));
    assert_eq!(notes[6].start_time, beat(4));
    assert_eq!(notes[6].x, 450.);
    for note in &notes {
        assert_eq!(
            note.end_time.checked_sub(note.start_time).unwrap(),
            spec.hold_length
        );
    }
    spec.include_endpoints = false;
    assert_eq!(spec.notes(|t| t).unwrap().len(), 5);
    spec.pattern = tools::Pattern::Random;
    assert_eq!(spec.notes(|t| t).unwrap(), spec.notes(|t| t).unwrap());
    spec.step = Beat::new(0, 1, 20001).unwrap();
    assert!(spec.notes(|t| t).is_err());
}
