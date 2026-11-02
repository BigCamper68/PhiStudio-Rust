//! Differential coverage for borrowed reads and the former owned JSON reader.
use phistudio_core::{
    Beat, ChartDocument, Command, Editor, EventType, LineEvent, Note, NoteId, NoteKind, StoryEvent,
    StoryKind,
};
use serde_json::{Value, json};

#[test]
fn lazy_notes_match_owned_deserialization_and_preserve_source_and_history() {
    let mut root = ChartDocument::empty("Read models", 120.)
        .unwrap()
        .as_json()
        .clone();
    let raw = (0..400)
        .map(|i| {
            let mut raw = json!(Note {
                kind: match i % 4 {
                    0 => NoteKind::Tap,
                    1 => NoteKind::Hold,
                    2 => NoteKind::Flick,
                    _ => NoteKind::Drag,
                },
                start_time: Beat::new(i % 17, 1, 3).unwrap(),
                end_time: Beat::new(i % 17 + 5, 1, 7).unwrap(),
                ..Default::default()
            });
            raw["judgeArea"] = Value::Null;
            raw["futureExtension"] =
                json!({"nested":[{"x":i,"unknown":"retained"}],"large":"x".repeat(1024)});
            raw
        })
        .collect::<Vec<_>>();
    root["judgeLineList"][0]["notes"] = json!(raw);
    let doc = ChartDocument::from_value(root).unwrap();
    let before = doc.as_json().clone();
    let legacy = raw
        .iter()
        .enumerate()
        .map(|(i, v)| (i, serde_json::from_value::<Note>(v.clone()).unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(doc.notes(0), legacy);
    assert_eq!(doc.iter_notes(0).collect::<Vec<_>>(), legacy);
    for (index, note) in &legacy {
        assert_eq!(&doc.note(0, *index).unwrap(), note);
    }
    assert_eq!(doc.iter_notes(99).count(), 0);
    assert!(doc.note(0, raw.len()).is_err());
    let mut editor = Editor::new(doc);
    let mut note = editor.document().note(0, 17).unwrap();
    note.size = 2.;
    editor
        .execute(Command::UpdateNote {
            id: NoteId { line: 0, index: 17 },
            note,
        })
        .unwrap();
    assert_eq!(
        editor.document().as_json()["judgeLineList"][0]["notes"][17]["futureExtension"],
        raw[17]["futureExtension"]
    );
    assert!(editor.undo());
    assert_eq!(editor.document().as_json(), &before);
    assert!(editor.redo());
    assert!(editor.undo());
    assert_eq!(editor.document().as_json(), &before);
    assert_eq!(
        ChartDocument::parse(&editor.document().to_json().unwrap())
            .unwrap()
            .as_json(),
        &before
    );
}
fn owned_event(kind: EventType, value: &Value) -> Result<LineEvent, String> {
    if !value.is_object() {
        return Err("Event must be an object".into());
    }
    let mut data = value.clone();
    if data.get("start").is_none() {
        data["start"] = json!(kind.default_value());
    }
    if data.get("end").is_none() {
        data["end"] = data["start"].clone();
    }
    if kind == EventType::Speed {
        for (key, value) in [
            ("easingType", json!(1)),
            ("easingLeft", json!(0.)),
            ("easingRight", json!(1.)),
            ("bezier", json!(0)),
            ("bezierPoints", json!([0., 0., 0., 0.])),
        ] {
            data[key] = value;
        }
    }
    serde_json::from_value(data).map_err(|e| e.to_string())
}
#[test]
fn event_reader_keeps_import_defaults_speed_metadata_and_error_behavior() {
    for kind in EventType::ALL {
        let base = json!(LineEvent::default());
        let mut cases = vec![base.clone()];
        for key in ["start", "end", "easingType", "bezierPoints"] {
            let mut missing = base.clone();
            missing.as_object_mut().unwrap().shift_remove(key);
            cases.push(missing);
            let mut null = base.clone();
            null[key] = Value::Null;
            cases.push(null);
            let mut malformed = base.clone();
            malformed[key] = json!("unknown");
            cases.push(malformed);
        }
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().shift_remove("start");
        missing.as_object_mut().unwrap().shift_remove("end");
        cases.push(missing);
        for mut raw in cases {
            raw["futureExtension"] = json!({"payload":"x".repeat(2048)});
            let before = raw.clone();
            match owned_event(kind, &raw) {
                Ok(expected) => assert_eq!(LineEvent::read(kind, &raw).unwrap(), expected),
                Err(expected) => assert!(
                    LineEvent::read(kind, &raw)
                        .unwrap_err()
                        .to_string()
                        .contains(&expected)
                ),
            }
            assert_eq!(raw, before);
        }
    }
}
#[test]
fn storyboard_reader_retains_owned_values_without_normalizing_unknown_extensions() {
    let doc = ChartDocument::empty("Story reads", 120.).unwrap();
    let mut editor = Editor::new(doc);
    for kind in StoryKind::EDITABLE {
        let event = StoryEvent::new(kind, Beat::ZERO, Beat::ONE);
        editor
            .execute(Command::EditStoryboard {
                line: 0,
                kind,
                index: None,
                event: Some(event),
            })
            .unwrap();
    }
    let mut root = editor.document().as_json().clone();
    for kind in StoryKind::EDITABLE {
        root["judgeLineList"][0]["extended"][kind.key()][0]["futureExtension"] =
            json!({"text":"preserved","values":[1,2,3]});
    }
    let doc = ChartDocument::from_value(root.clone()).unwrap();
    for kind in StoryKind::EDITABLE {
        let expected = serde_json::from_value::<StoryEvent>(
            root["judgeLineList"][0]["extended"][kind.key()][0].clone(),
        )
        .unwrap();
        assert_eq!(doc.story_events(0, kind).unwrap(), vec![expected]);
    }
    let mut editor = Editor::new(doc);
    let mut event = editor
        .document()
        .story_events(0, StoryKind::ScaleX)
        .unwrap()
        .remove(0);
    event.end = json!(2.);
    editor
        .execute(Command::EditStoryboard {
            line: 0,
            kind: StoryKind::ScaleX,
            index: Some(0),
            event: Some(event),
        })
        .unwrap();
    assert_eq!(
        editor.document().as_json()["judgeLineList"][0]["extended"]["scaleXEvents"][0]["futureExtension"],
        root["judgeLineList"][0]["extended"]["scaleXEvents"][0]["futureExtension"]
    );
    assert!(editor.undo());
    assert_eq!(editor.document().as_json(), &root);
    assert_eq!(
        ChartDocument::parse(&editor.document().to_json().unwrap())
            .unwrap()
            .as_json(),
        &root
    );
}
