use phistudio_core::{
    Beat, BpmChange, ChartDocument, Command, Editor, Note, NoteId, NoteKind, TempoMap, load_chart,
    save_chart,
};
use serde_json::{Value, json};

const DEMO: &str = include_str!("../../../assets/charts/phistudio-demo.rpe.json");
fn demo() -> ChartDocument {
    ChartDocument::parse(DEMO).unwrap()
}
fn beat(text: &str) -> Beat {
    text.parse().unwrap()
}

#[test]
fn java_demo_round_trip_is_semantically_identical() {
    let source: Value = serde_json::from_str(DEMO).unwrap();
    let chart = demo();
    assert_eq!(chart.as_json(), &source);
    assert_eq!(
        serde_json::from_str::<Value>(&chart.to_json().unwrap()).unwrap(),
        source
    );
    assert_eq!(chart.note_count(), 4);
    assert_eq!(chart.note(0, 1).unwrap().kind, NoteKind::Drag);
    assert_eq!(chart.note(0, 2).unwrap().kind, NoteKind::Flick);
    assert_eq!(chart.note(0, 3).unwrap().kind, NoteKind::Hold);
}

#[test]
fn edits_and_undo_preserve_unknown_fields_at_every_level() {
    let mut source = demo().as_json().clone();
    source["futureRoot"] = json!({"raw":[1,null,{"unknown":true}]});
    source["META"]["futureMeta"] = json!("keep");
    source["judgeLineList"][0]["futureLine"] = json!({"a":42});
    source["judgeLineList"][0]["notes"][0]["hitsound"] = json!("custom.wav");
    source["judgeLineList"][0]["notes"][0]["futureNote"] = json!([1, 2, 3]);
    source["blockAreaList"] = json!([{"phiStudioColor":[20,50,255],"move":[{"unknown":true}]}]);
    let mut editor = Editor::new(ChartDocument::from_value(source.clone()).unwrap());
    let mut note = editor.document().note(0, 0).unwrap();
    note.x = 97.;
    editor
        .execute(Command::UpdateNote {
            id: NoteId { line: 0, index: 0 },
            note,
        })
        .unwrap();
    let saved = editor.document().as_json();
    assert_eq!(
        saved["judgeLineList"][0]["notes"][0]["hitsound"],
        "custom.wav"
    );
    assert_eq!(
        saved["judgeLineList"][0]["notes"][0]["futureNote"],
        json!([1, 2, 3])
    );
    assert_eq!(saved["blockAreaList"], source["blockAreaList"]);
    editor.undo();
    assert_eq!(editor.document().as_json(), &source);
    assert!(!editor.is_dirty());
    editor.redo();
    assert_eq!(editor.document().note(0, 0).unwrap().x, 97.);
}

#[test]
fn hold_creation_deletion_and_history_restore_exact_json() {
    let original = demo().as_json().clone();
    let mut editor = Editor::new(demo());
    let note = Note {
        kind: NoteKind::Hold,
        start_time: beat("3:1/3"),
        end_time: beat("5:2/3"),
        ..Note::default()
    };
    editor
        .execute(Command::AddNote {
            line: 0,
            note: note.clone(),
        })
        .unwrap();
    assert_eq!(editor.document().note(0, 4).unwrap(), note);
    assert_eq!(
        editor.document().as_json()["judgeLineList"][0]["numOfNotes"],
        5
    );
    editor
        .execute(Command::DeleteNote(NoteId { line: 0, index: 1 }))
        .unwrap();
    editor.undo();
    assert_eq!(editor.document().note_count(), 5);
    editor.undo();
    assert_eq!(editor.document().as_json(), &original);
    editor.redo();
    editor.redo();
    assert_eq!(editor.document().note_count(), 4);
    assert_eq!(editor.document().note(0, 3).unwrap(), note);
}

#[test]
fn invalid_command_leaves_chart_history_and_saved_state_untouched() {
    let mut editor = Editor::new(demo());
    let original = editor.document().as_json().clone();
    let note = Note {
        kind: NoteKind::Hold,
        start_time: Beat::ONE,
        end_time: Beat::ZERO,
        ..Note::default()
    };
    assert!(editor.execute(Command::AddNote { line: 0, note }).is_err());
    assert!(
        editor
            .execute(Command::DeleteNote(NoteId { line: 3, index: 0 }))
            .is_err()
    );
    assert_eq!(editor.document().as_json(), &original);
    assert!(!editor.can_undo());
    assert!(!editor.is_dirty());
    assert_eq!(editor.revision(), 0);
}

#[test]
fn saved_state_is_correct_after_undo_redo_and_branching() {
    let mut editor = Editor::new(demo());
    editor
        .execute(Command::RenameChart("Saved".into()))
        .unwrap();
    editor.mark_saved();
    editor
        .execute(Command::RenameChart("Draft".into()))
        .unwrap();
    assert!(editor.is_dirty());
    editor.undo();
    assert!(!editor.is_dirty());
    editor.redo();
    assert!(editor.is_dirty());
    editor.undo();
    editor.undo();
    editor
        .execute(Command::RenameChart("New branch at same cursor".into()))
        .unwrap();
    assert!(editor.is_dirty());
    assert!(!editor.can_redo());
    assert_eq!(editor.revision(), 7);
}

#[test]
fn no_op_does_not_discard_redo_or_increment_revision() {
    let mut editor = Editor::new(demo());
    editor
        .execute(Command::RenameChart("Other".into()))
        .unwrap();
    editor.undo();
    let revision = editor.revision();
    assert!(
        !editor
            .execute(Command::UpdateNote {
                id: NoteId { line: 0, index: 0 },
                note: editor.document().note(0, 0).unwrap()
            })
            .unwrap()
    );
    assert_eq!(editor.revision(), revision);
    assert!(editor.can_redo());
}

#[test]
fn undo_restores_missing_or_null_containers_and_authored_counts() {
    for notes in [None, Some(Value::Null), Some(json!([]))] {
        let mut source = demo().as_json().clone();
        let line = source["judgeLineList"][0].as_object_mut().unwrap();
        line.shift_remove("notes");
        if let Some(notes) = notes {
            line.insert("notes".into(), notes);
        }
        line.insert("numOfNotes".into(), json!(197));
        let mut editor = Editor::new(ChartDocument::from_value(source.clone()).unwrap());
        editor
            .execute(Command::AddNote {
                line: 0,
                note: Note::default(),
            })
            .unwrap();
        editor.undo();
        assert_eq!(editor.document().as_json(), &source);
        editor.redo();
        assert_eq!(editor.document().note_count(), 1);
    }
}

#[test]
fn line_creation_and_rename_are_undoable() {
    let mut editor = Editor::new(demo());
    let original = editor.document().as_json().clone();
    editor
        .execute(Command::AddLine {
            name: "Second line".into(),
        })
        .unwrap();
    editor
        .execute(Command::RenameLine {
            line: 1,
            name: "Melody".into(),
        })
        .unwrap();
    assert_eq!(editor.document().lines()[1].name, "Melody");
    editor.undo();
    editor.undo();
    assert_eq!(editor.document().as_json(), &original);
    editor.redo();
    editor.redo();
    assert_eq!(editor.document().line_count(), 2);
}

#[test]
fn rational_beats_normalize_negative_values_and_compare_exactly() {
    assert_eq!(Beat::new(-1, 3, 4).unwrap(), beat("-0.25"));
    assert_eq!(Beat::new(2, -1, 4).unwrap(), beat("1.75"));
    assert_eq!(Beat::new(0, 1, -4).unwrap(), beat("-0.25"));
    assert_eq!(Beat::new(0, 4, 8).unwrap().triple(), [0, 1, 2]);
    assert!(beat("0:1/2147483647") < beat("0:1/2147483646"));
    assert!(Beat::new(0, 1, 0).is_err());
    assert!(Beat::new(i32::MAX, 1, 1).is_err());
    assert!(Beat::from_grid(f64::NAN, 4).is_err());
    assert_eq!(Beat::from_grid(-0.24, 4).unwrap(), beat("-0.25"));
    assert_eq!(
        serde_json::from_str::<Beat>("[-1,3,4]").unwrap(),
        beat("-0.25")
    );
    assert!("0.000000000000000001".parse::<Beat>().is_err());
    let start = beat("2:1/7");
    let end = beat("5:2/9");
    let shift = beat("-0.25");
    assert_eq!(
        end.checked_sub(start).unwrap(),
        end.checked_add(shift)
            .unwrap()
            .checked_sub(start.checked_add(shift).unwrap())
            .unwrap()
    );
}

#[test]
fn bpm_boundaries_round_trip_and_match_prpr_clock_origin() {
    let map = TempoMap::new(vec![
        BpmChange {
            start: beat("8"),
            bpm: 240.,
        },
        BpmChange {
            start: beat("-2"),
            bpm: 120.,
        },
        BpmChange {
            start: beat("4"),
            bpm: 60.,
        },
    ])
    .unwrap();
    assert_eq!(map.seconds_at(-2.), 0.);
    assert_eq!(map.seconds_at(4.), 3.);
    assert_eq!(map.seconds_at(8.), 7.);
    for beat in [-9., -2., 0., 3.999, 4., 7.5, 8., 64.] {
        assert!((map.beat_at(map.seconds_at(beat)) - beat).abs() < 1e-10);
    }
    assert_eq!(map.bpm_at(8.), 240.);
    assert!(
        TempoMap::new(vec![BpmChange {
            start: Beat::ZERO,
            bpm: 0.
        }])
        .is_err()
    );
}

#[test]
fn render_sorting_never_rewrites_the_editable_source() {
    let mut source = demo().as_json().clone();
    source["BPMList"] = json!([{"bpm":180.,"startTime":[8,0,1],"extra":"last"},{"bpm":120.,"startTime":[0,0,1],"extra":"first"}]);
    let document = ChartDocument::from_value(source.clone()).unwrap();
    let render: Value = serde_json::from_str(&document.render_json().unwrap()).unwrap();
    assert_eq!(render["BPMList"][0]["extra"], "first");
    assert_eq!(document.as_json(), &source);
}

#[test]
fn malformed_charts_and_parent_cycles_are_rejected_before_rendering() {
    assert!(ChartDocument::parse("{}").is_err());
    for path in ["parent", "denominator", "note-type", "notes", "bpm"] {
        let mut source = demo().as_json().clone();
        match path {
            "parent" => source["judgeLineList"][0]["father"] = json!(0),
            "denominator" => source["judgeLineList"][0]["notes"][0]["startTime"] = json!([0, 1, 0]),
            "note-type" => source["judgeLineList"][0]["notes"][0]["type"] = json!(5),
            "notes" => source["judgeLineList"][0]["notes"][0] = json!([]),
            "bpm" => source["BPMList"][0]["bpm"] = json!(-2.),
            _ => unreachable!(),
        }
        assert!(ChartDocument::from_value(source).is_err(), "{path}");
    }
}

#[test]
fn atomic_save_reopens_edits_and_failure_does_not_mark_saved() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("chart.json");
    let mut editor = Editor::new(demo());
    save_chart(editor.document(), &path).unwrap();
    editor
        .execute(Command::RenameChart("Reopened".into()))
        .unwrap();
    assert!(
        save_chart(
            editor.document(),
            &directory.path().join("absent/chart.json")
        )
        .is_err()
    );
    assert!(editor.is_dirty());
    assert_eq!(load_chart(&path).unwrap().name(), "PhiStudio demo");
    save_chart(editor.document(), &path).unwrap();
    editor.mark_saved();
    assert!(!editor.is_dirty());
    assert_eq!(
        load_chart(&path).unwrap().as_json(),
        editor.document().as_json()
    );
}
