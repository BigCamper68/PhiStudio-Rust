use super::*;

#[test]
fn reverse_time_treats_holds_as_intervals_and_reverses_event_curve_metadata() {
    let mut editor = fixture();
    let before = editor.document().as_json().clone();
    let edit = tools::transform(
        editor.document(),
        &notes(&[0, 1, 2]),
        Transform::ReverseTime,
    )
    .unwrap();
    let selected = apply(&mut editor, edit);
    let rows = editor.document().notes(0);
    assert_eq!(rows[0].1.start_time, beat(5));
    assert_eq!(rows[1].1.start_time, beat(2));
    assert_eq!(rows[1].1.end_time, beat(3));
    assert_eq!(rows[2].1.start_time, beat(1));
    let edit = tools::transform(editor.document(), &selected, Transform::ReverseTime).unwrap();
    apply(&mut editor, edit);
    assert_eq!(editor.document().note(0, 0).unwrap().start_time, beat(1));
    editor.undo();
    editor.undo();
    assert_eq!(editor.document().as_json(), &before);
}

#[test]
fn no_op_does_not_dirty_chart_and_read_only_event_layers_reject_changes() {
    let mut editor = fixture();
    let edit = tools::transform(
        editor.document(),
        &notes(&[0]),
        Transform::Shift {
            beats: Beat::ZERO,
            x: 0.,
        },
    )
    .unwrap();
    assert!(edit.apply(&mut editor).unwrap().is_none());
    assert!(!editor.is_dirty());
    assert_eq!(editor.revision(), 0);
    assert!(tools::repeat(editor.document(), &notes(&[0]), 0, Beat::ONE).is_err());
    assert!(tools::relocate(editor.document(), &events(&[0]), 0, 4, false).is_err());
    assert!(
        Beat::new(i32::MAX, 0, 1)
            .unwrap()
            .checked_mul(beat(2))
            .is_err()
    );
}

#[test]
fn bulk_bezier_and_easing_windows_preserve_extensions_and_undo_exactly() {
    let mut editor = fixture();
    let before = editor.document().as_json().clone();
    let edit = tools::transform(
        editor.document(),
        &events(&[0, 2]),
        Transform::Curve {
            easing: 8,
            window: [0.2, 0.8],
            bezier: Some([0.2, 0.1, 0.8, 0.9]),
        },
    )
    .unwrap();
    let selected = apply(&mut editor, edit);
    assert_eq!(selected.events.len(), 2);
    for id in selected.events {
        let event = editor.document().event(id).unwrap();
        assert_eq!(event.bezier, 1);
        assert_eq!(event.easing_type, 8);
        assert_eq!(event.easing_left, 0.2);
        assert_eq!(
            editor
                .document()
                .event_values(id.line, id.layer, id.kind)
                .unwrap()[id.index]["custom"],
            "keep"
        );
    }
    editor.undo();
    assert_eq!(editor.document().as_json(), &before);
}

#[test]
fn nonlinear_imported_speed_metadata_cannot_be_silently_reinterpreted_by_curve_tools() {
    let mut root = fixture().document().as_json().clone();
    root["judgeLineList"][0]["eventLayers"][1]["speedEvents"] = json!([{"startTime":[0,0,1],"endTime":[2,0,1],"start":2,"end":8,"easingType":27,"bezier":1,"bezierPoints":[0.2,0.3,0.6,0.8]}]);
    let editor = Editor::new(ChartDocument::from_value(root).unwrap());
    let selected = Selection {
        notes: vec![],
        events: vec![EventId {
            line: 0,
            layer: 1,
            kind: EventType::Speed,
            index: 0,
        }],
    };
    assert!(tools::transform(editor.document(), &selected, Transform::ReverseTime).is_err());
    assert!(
        tools::subdivide_events(editor.document(), &selected, 2, None, |_, event, _, _| Ok(
            event.clone()
        ))
        .is_err()
    );
    assert!(!editor.is_dirty());
    assert_eq!(editor.revision(), 0);
}
