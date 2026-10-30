use super::*;

#[test]
fn mixed_delete_is_one_undo_and_restores_counts_source_order_and_extensions() {
    let mut editor = fixture();
    let before = editor.document().as_json().clone();
    let mut selection = notes(&[0, 2, 0]);
    selection.events = events(&[0, 2]).events;
    let edit = tools::delete(editor.document(), &selection).unwrap();
    apply(&mut editor, edit);
    assert_eq!(editor.document().notes(0).len(), 1);
    assert_eq!(
        editor
            .document()
            .event_values(0, 1, EventType::Rotate)
            .unwrap()[0]["startTime"],
        json!([2, 0, 1])
    );
    assert_eq!(
        editor.document().as_json()["judgeLineList"][0]["numOfNotes"],
        1
    );
    assert!(editor.is_dirty());
    assert_eq!(editor.undo_label(), Some("Delete selection"));
    assert!(editor.undo());
    assert_eq!(editor.document().as_json(), &before);
    assert!(!editor.is_dirty());
    assert!(!editor.can_undo());
    assert!(editor.redo());
    assert!(editor.undo());
    assert_eq!(editor.document().as_json(), &before);
}

#[test]
fn failed_batch_rolls_back_earlier_tracks_and_preserves_redo_revision_and_saved_state() {
    let mut editor = fixture();
    let edit = tools::transform(editor.document(), &notes(&[1]), Transform::MirrorX(0.)).unwrap();
    apply(&mut editor, edit);
    editor.undo();
    let before = editor.document().as_json().clone();
    let revision = editor.revision();
    let mut selection = notes(&[0]);
    selection.events = events(&[0]).events;
    let edit = tools::transform(
        editor.document(),
        &selection,
        Transform::Shift {
            beats: beat(-2),
            x: 10.,
        },
    )
    .err()
    .unwrap();
    assert!(edit.to_string().contains("before beat zero"));
    let mut selection = notes(&[1]);
    selection.events = events(&[0]).events;
    let edit = tools::transform(
        editor.document(),
        &selection,
        Transform::Shift {
            beats: beat(-2),
            x: 10.,
        },
    )
    .unwrap();
    assert!(edit.apply(&mut editor).is_err()); // event 6..7 would overlap the untouched 4..5
    assert_eq!(editor.document().as_json(), &before);
    assert_eq!(editor.revision(), revision);
    assert!(!editor.is_dirty());
    assert!(!editor.can_undo());
    assert!(editor.can_redo());
}

#[test]
fn generic_command_transaction_also_rolls_back_atomically() {
    let mut editor = fixture();
    let before = editor.document().as_json().clone();
    let result = editor.execute_batch(
        "Test transaction",
        vec![
            Command::AddNote {
                line: 0,
                note: Note::default(),
            },
            Command::DeleteNote(NoteId {
                line: 0,
                index: 999,
            }),
        ],
    );
    assert!(result.is_err());
    assert_eq!(editor.document().as_json(), &before);
    assert_eq!(editor.revision(), 0);
    assert!(!editor.can_undo());
}

#[test]
fn clipboard_paste_keeps_raw_data_timing_and_relative_lines_and_layers() {
    let editor = fixture();
    let mut selection = notes(&[0, 1]);
    selection.events = events(&[0]).events;
    let source = editor.document().as_json().clone();
    let clipboard = ChartClipboard::copy(editor.document(), &selection).unwrap();
    let mut target = Editor::new(ChartDocument::empty("Target", 150.).unwrap());
    target
        .execute(Command::AddLine {
            name: "Target line".into(),
        })
        .unwrap();
    let before = target.document().as_json().clone();
    let edit = clipboard
        .paste(target.document(), 1, 0, beat(10), true)
        .unwrap();
    let pasted = apply(&mut target, edit);
    assert_eq!(pasted.len(), 3);
    let note = target.document().note(1, 0).unwrap();
    assert_eq!(note.start_time, beat(10));
    assert_eq!(note.x, 135.);
    assert_eq!(note.alpha, 500);
    assert_eq!(note.above, 2);
    assert_eq!(note.is_fake, 7);
    let raw = &target.document().as_json()["judgeLineList"][1]["notes"][0];
    assert_eq!(raw["hitsound"], "custom.wav");
    assert_eq!(
        raw["controls"],
        source["judgeLineList"][0]["notes"][0]["controls"]
    );
    let hold = target.document().note(1, 1).unwrap();
    assert_eq!(hold.start_time, beat(12));
    assert_eq!(hold.end_time, beat(13));
    let event = target.document().event(pasted.events[0]).unwrap();
    assert_eq!(event.start_time, beat(15));
    assert_eq!(pasted.events[0].layer, 0);
    assert_eq!(editor.document().as_json(), &source);
    target.undo();
    assert_eq!(target.document().as_json(), &before);
}

#[test]
fn sparse_multi_track_creation_undo_restores_shared_missing_and_null_parents() {
    for shape in [None, Some(json!(null)), Some(json!([null, null]))] {
        let mut root = ChartDocument::empty("Sparse", 120.)
            .unwrap()
            .as_json()
            .clone();
        if let Some(shape) = shape {
            root["judgeLineList"][0]["eventLayers"] = shape;
        } else {
            root["judgeLineList"][0]
                .as_object_mut()
                .unwrap()
                .shift_remove("eventLayers");
        }
        let mut editor = Editor::new(ChartDocument::from_value(root).unwrap());
        let before = editor.document().as_json().clone();
        let edit = tools::generate_motion(
            editor.document(),
            0,
            3,
            beat(2),
            beat(4),
            Beat::new(0, 1, 3).unwrap(),
            |t| [t * 100., t * 200.],
        )
        .unwrap();
        let selected = apply(&mut editor, edit);
        assert_eq!(selected.events.len(), 12);
        assert_eq!(selected.events[0].layer, 3);
        let after = editor.document().as_json().clone();
        editor.undo();
        assert_eq!(editor.document().as_json(), &before);
        editor.redo();
        assert_eq!(editor.document().as_json(), &after);
    }
}

#[test]
fn batch_values_follow_time_order_and_preserve_custom_note_fields_and_native_ranges() {
    let mut editor = fixture();
    let before = editor.document().as_json().clone();
    let edit = tools::batch_values(
        editor.document(),
        &notes(&[2, 0, 1]),
        ValueField::X,
        ValueMode::Set,
        &[10., 20., 30.],
    )
    .unwrap();
    let selection = apply(&mut editor, edit);
    assert_eq!(selection.notes.len(), 3);
    assert_eq!(
        editor
            .document()
            .notes(0)
            .iter()
            .map(|(_, n)| n.x)
            .collect::<Vec<_>>(),
        vec![10., 20., 30.]
    );
    assert_eq!(
        editor.document().as_json()["judgeLineList"][0]["notes"][0]["hitsound"],
        "custom.wav"
    );
    let edit = tools::batch_values(
        editor.document(),
        &selection,
        ValueField::Alpha,
        ValueMode::Set,
        &[65535.],
    )
    .unwrap();
    apply(&mut editor, edit);
    assert_eq!(editor.document().note(0, 0).unwrap().alpha, u16::MAX);
    assert!(
        tools::batch_values(
            editor.document(),
            &selection,
            ValueField::Alpha,
            ValueMode::Set,
            &[-1.]
        )
        .is_err()
    );
    editor.undo();
    editor.undo();
    assert_eq!(editor.document().as_json(), &before);
}

#[test]
fn transforms_keep_hold_duration_and_quantize_short_holds_to_one_tick() {
    let mut editor = fixture();
    let original = editor.document().as_json().clone();
    let edit = tools::transform(
        editor.document(),
        &notes(&[0, 1]),
        Transform::ScaleTime(Beat::new(0, 1, 2).unwrap()),
    )
    .unwrap();
    let selection = apply(&mut editor, edit);
    let hold = editor.document().note(0, selection.notes[1].index).unwrap();
    assert_eq!(hold.start_time, beat(2));
    assert_eq!(hold.end_time, Beat::new(2, 1, 2).unwrap());
    editor.undo();
    assert_eq!(editor.document().as_json(), &original);
    let edit = tools::transform(
        editor.document(),
        &notes(&[1]),
        Transform::NoteType {
            kind: NoteKind::Hold,
            hold_length: Beat::new(0, 1, 100).unwrap(),
        },
    )
    .unwrap();
    let selection = apply(&mut editor, edit);
    let edit = tools::transform(editor.document(), &selection, Transform::Quantize(4)).unwrap();
    let selection = apply(&mut editor, edit);
    let hold = editor.document().note(0, selection.notes[0].index).unwrap();
    assert_eq!(
        hold.end_time.checked_sub(hold.start_time).unwrap(),
        Beat::new(0, 1, 4).unwrap()
    );
}

#[test]
fn speed_metadata_and_unselected_order_survive_group_operations_and_exact_undo() {
    let mut root = fixture().document().as_json().clone();
    root["judgeLineList"][0]["eventLayers"][1]["speedEvents"] = json!([{"startTime":[0,0,1],"endTime":[1,0,1],"start":2,"end":4,"easingType":27,"easingLeft":0.2,"bezier":1,"bezierPoints":[0.2,0.3,0.6,0.8],"future":"speed"}]);
    let mut editor = Editor::new(ChartDocument::from_value(root).unwrap());
    let before = editor.document().as_json().clone();
    let selected = Selection {
        notes: vec![],
        events: vec![EventId {
            line: 0,
            layer: 1,
            kind: EventType::Speed,
            index: 0,
        }],
    };
    let edit = tools::transform(
        editor.document(),
        &selected,
        Transform::Shift {
            beats: beat(1),
            x: 0.,
        },
    )
    .unwrap();
    apply(&mut editor, edit);
    let raw = &editor
        .document()
        .event_values(0, 1, EventType::Speed)
        .unwrap()[0];
    assert_eq!(raw["easingType"], 27);
    assert_eq!(raw["bezier"], 1);
    assert_eq!(raw["future"], "speed");
    assert_eq!(
        editor.document().as_json()["judgeLineList"][0]["eventLayers"][1]["rotateEvents"],
        before["judgeLineList"][0]["eventLayers"][1]["rotateEvents"]
    );
    editor.undo();
    assert_eq!(editor.document().as_json(), &before);
}

#[test]
fn exact_duplicates_keep_distinct_extensions_and_restore_source_order() {
    let mut root = fixture().document().as_json().clone();
    let original = root["judgeLineList"][0]["notes"][0].clone();
    root["judgeLineList"][0]["notes"]
        .as_array_mut()
        .unwrap()
        .push(original.clone());
    let mut distinct = original;
    distinct["hitsound"] = json!("different.wav");
    root["judgeLineList"][0]["notes"]
        .as_array_mut()
        .unwrap()
        .push(distinct);
    let mut editor = Editor::new(ChartDocument::from_value(root).unwrap());
    let before = editor.document().as_json().clone();
    let selection = Selection::all(editor.document(), 0, 1, true, false).unwrap();
    let edit = tools::remove_duplicates(editor.document(), &selection).unwrap();
    apply(&mut editor, edit);
    assert_eq!(editor.document().notes(0).len(), 4);
    editor.undo();
    assert_eq!(editor.document().as_json(), &before);
}

#[test]
fn large_generator_is_one_history_entry_and_does_not_copy_full_chart_per_note() {
    let mut editor = fixture();
    let before = editor.document().as_json().clone();
    let mut spec = pattern();
    spec.start = beat(8);
    spec.end = beat(2007);
    spec.step = Beat::ONE;
    let generated = spec.notes(|t| t).unwrap();
    assert_eq!(generated.len(), 2000);
    let edit = tools::generate_notes(editor.document(), 0, generated).unwrap();
    let selected = apply(&mut editor, edit);
    assert_eq!(selected.len(), 2000);
    assert_eq!(editor.document().notes(0).len(), 2003);
    editor.undo();
    assert_eq!(editor.document().as_json(), &before);
    assert!(!editor.can_undo());
}

#[test]
fn horizontal_scaling_preserves_raw_extensions_timing_and_sparse_containers() {
    let mut editor = fixture();
    let before = editor.document().as_json().clone();
    let selected = notes(&[0, 1, 2]);
    let edit = tools::transform(
        editor.document(),
        &selected,
        Transform::ScaleX {
            anchor: 135.,
            factor: -2.,
        },
    )
    .unwrap();
    apply(&mut editor, edit);
    let rows = editor.document().notes(0);
    assert_eq!(
        rows.iter().map(|(_, n)| n.x).collect::<Vec<_>>(),
        vec![675., 135., -135.]
    );
    let mut changed = editor.document().as_json().clone();
    assert_eq!(changed["judgeLineList"][0]["numOfNotes"], 3);
    changed["judgeLineList"][0]["numOfNotes"] = before["judgeLineList"][0]["numOfNotes"].clone();
    for (a, b) in changed["judgeLineList"][0]["notes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip(before["judgeLineList"][0]["notes"].as_array().unwrap())
    {
        a["positionX"] = b["positionX"].clone();
    }
    assert_eq!(changed, before);
    editor.undo();
    assert_eq!(*editor.document().as_json(), before);
    assert!(
        tools::transform(
            editor.document(),
            &selected,
            Transform::ScaleX {
                anchor: 0.,
                factor: f64::NAN
            }
        )
        .is_err()
    );
    assert!(editor.can_redo());
}
