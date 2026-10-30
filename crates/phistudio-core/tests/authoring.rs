use phistudio_core::{
    AttachedUi, Beat, BpmChange, ChartDocument, Command, EditTarget, Editor, EventId, EventType,
    LineEvent,
};
use serde_json::{Value, json};

fn source() -> Value {
    serde_json::from_str(include_str!(
        "../../../assets/charts/phistudio-demo.rpe.json"
    ))
    .unwrap()
}
fn editor(value: &Value) -> Editor {
    Editor::new(ChartDocument::from_value(value.clone()).unwrap())
}
fn beat(s: &str) -> Beat {
    s.parse().unwrap()
}
fn event(start: &str, end: &str) -> LineEvent {
    LineEvent {
        start_time: beat(start),
        end_time: beat(end),
        ..Default::default()
    }
}
fn id(kind: EventType, index: usize) -> EventId {
    EventId {
        line: 0,
        layer: 0,
        kind,
        index,
    }
}

#[test]
fn all_event_tracks_restore_missing_null_and_sparse_layers_exactly() {
    for layers in [
        None,
        Some(Value::Null),
        Some(json!([])),
        Some(json!([null])),
        Some(json!([{"speedEvents":null}])),
    ] {
        for kind in EventType::ALL {
            for layer in [0, 3] {
                let mut raw = source();
                let line = raw["judgeLineList"][0].as_object_mut().unwrap();
                line.shift_remove("eventLayers");
                if let Some(v) = &layers {
                    line.insert("eventLayers".into(), v.clone());
                }
                let mut ed = editor(&raw);
                let value = event("0:1/3", "1:2/3");
                ed.execute(Command::AddEvent {
                    line: 0,
                    layer,
                    kind,
                    event: value.clone(),
                })
                .unwrap();
                let target = EventId {
                    layer,
                    ..id(kind, 0)
                };
                assert_eq!(ed.document().event(target).unwrap(), value);
                ed.undo();
                assert_eq!(
                    ed.document().as_json(),
                    &raw,
                    "{layers:?} {kind:?} layer {layer}"
                );
                ed.redo();
                assert_eq!(ed.document().event(target).unwrap(), value);
            }
        }
    }
}

#[test]
fn event_value_edits_preserve_source_order_defaults_and_speed_metadata() {
    let mut raw = source();
    let events = json!([
        {"startTime":[4,0,1],"endTime":[5,0,1],"start":3,"end":4,"easingType":9,"easingLeft":0.2,"bezier":1,"bezierPoints":[0.2,0.1,0.8,0.9],"future":{"raw":true}},
        {"startTime":[0,0,7],"endTime":[1,0,1],"start":2,"end":3}
    ]);
    raw["judgeLineList"][0]["eventLayers"] = json!([{"speedEvents":events}]);
    let mut ed = editor(&raw);
    let target = id(EventType::Speed, 0);
    let mut value = ed.document().event(target).unwrap();
    assert_eq!(value.easing_type, 1);
    value.end = 6.;
    ed.execute(Command::UpdateEvent {
        id: target,
        event: value,
    })
    .unwrap();
    let mut expected = raw.clone();
    expected["judgeLineList"][0]["eventLayers"][0]["speedEvents"][0]["end"] = json!(6.);
    assert_eq!(ed.document().as_json(), &expected);
    ed.undo();
    assert_eq!(ed.document().as_json(), &raw);
    let mut value = ed.document().event(target).unwrap();
    value.start_time = beat("2:1/3");
    value.end_time = beat("3:1/3");
    ed.execute(Command::UpdateEvent {
        id: target,
        event: value.clone(),
    })
    .unwrap();
    assert_eq!(
        ed.last_target(),
        Some(EditTarget::Event(id(EventType::Speed, 1)))
    );
    assert_eq!(ed.document().event(id(EventType::Speed, 1)).unwrap(), value);
    ed.undo();
    assert_eq!(ed.document().as_json(), &raw);
    ed.redo();
    ed.execute(Command::DeleteEvent(id(EventType::Speed, 0)))
        .unwrap();
    ed.undo();
    ed.undo();
    assert_eq!(ed.document().as_json(), &raw);
}

#[test]
fn deleting_last_event_omits_track_and_undo_restores_authored_object() {
    let mut raw = source();
    raw["judgeLineList"][0]["eventLayers"] = json!([{"alphaEvents":[{"startTime":[0,0,1],"endTime":[1,0,1],"start":255,"end":255,"extra":[null,1]}],"futureTrack":[1]}]);
    let mut ed = editor(&raw);
    ed.execute(Command::DeleteEvent(id(EventType::Alpha, 0)))
        .unwrap();
    assert!(
        ed.document().as_json()["judgeLineList"][0]["eventLayers"][0]
            .get("alphaEvents")
            .is_none()
    );
    ed.undo();
    assert_eq!(ed.document().as_json(), &raw);
    ed.redo();
    ed.undo();
    assert_eq!(ed.document().as_json(), &raw);
}

#[test]
fn event_validation_uses_exact_intervals_and_keeps_redo_on_invalid_edits() {
    let mut raw = source();
    raw["judgeLineList"][0]["eventLayers"] = json!([]);
    let mut ed = editor(&raw);
    ed.execute(Command::AddEvent {
        line: 0,
        layer: 0,
        kind: EventType::Alpha,
        event: event("1:1/3", "2:2/3"),
    })
    .unwrap();
    ed.execute(Command::AddEvent {
        line: 0,
        layer: 0,
        kind: EventType::Alpha,
        event: event("2:2/3", "3"),
    })
    .unwrap();
    ed.undo();
    let before = ed.document().as_json().clone();
    let revision = ed.revision();
    for (layer, value) in [
        (0, event("2:1/3", "3")),
        (4, event("4", "5")),
        (0, event("-1", "0")),
        (0, event("4", "4")),
    ] {
        assert!(
            ed.execute(Command::AddEvent {
                line: 0,
                layer,
                kind: EventType::Alpha,
                event: value
            })
            .is_err()
        );
        assert_eq!(ed.document().as_json(), &before);
        assert_eq!(ed.revision(), revision);
        assert!(ed.can_redo());
    }
    let value = ed.document().event(id(EventType::Alpha, 0)).unwrap();
    assert!(
        !ed.execute(Command::UpdateEvent {
            id: id(EventType::Alpha, 0),
            event: value
        })
        .unwrap()
    );
    assert!(ed.can_redo());
    ed.redo();
    ed.undo();
    ed.undo();
    assert_eq!(ed.document().as_json(), &raw);
}

#[test]
fn bpm_edits_refresh_tempo_immediately_and_undo_unsorted_source_exactly() {
    let mut raw = source();
    raw["BPMList"] =
        json!([{"startTime":[8,0,1],"bpm":240,"future":"keep"},{"startTime":[0,0,7],"bpm":120}]);
    let mut ed = editor(&raw);
    assert_eq!(ed.document().tempo().seconds_at(12.), 5.);
    ed.execute(Command::UpdateBpm {
        index: 0,
        change: BpmChange {
            start: beat("4"),
            bpm: 60.,
        },
    })
    .unwrap();
    assert_eq!(ed.last_target(), Some(EditTarget::Bpm(1)));
    assert_eq!(ed.document().tempo().seconds_at(12.), 10.);
    assert_eq!(ed.document().as_json()["BPMList"][1]["future"], "keep");
    assert_eq!(
        ed.document().as_json()["BPMList"][0]["startTime"],
        json!([0, 0, 7])
    );
    ed.execute(Command::AddBpm(BpmChange {
        start: beat("6"),
        bpm: 180.,
    }))
    .unwrap();
    assert_eq!(ed.document().tempo().seconds_at(12.), 6.);
    ed.execute(Command::DeleteBpm(1)).unwrap();
    ed.undo();
    ed.undo();
    ed.undo();
    assert_eq!(ed.document().as_json(), &raw);
    assert_eq!(ed.document().tempo().seconds_at(12.), 5.);
    ed.redo();
    assert_eq!(ed.document().tempo().seconds_at(12.), 10.);
}

#[test]
fn bpm_first_beat_and_duplicates_are_protected_without_history_changes() {
    let mut ed = editor(&source());
    let original = ed.document().as_json().clone();
    for command in [
        Command::DeleteBpm(0),
        Command::UpdateBpm {
            index: 0,
            change: BpmChange {
                start: Beat::ONE,
                bpm: 120.,
            },
        },
        Command::AddBpm(BpmChange {
            start: Beat::ZERO,
            bpm: 90.,
        }),
        Command::AddBpm(BpmChange {
            start: Beat::ONE,
            bpm: f64::NAN,
        }),
    ] {
        assert!(ed.execute(command).is_err());
        assert_eq!(ed.document().as_json(), &original);
        assert_eq!(ed.revision(), 0);
    }
}

#[test]
fn line_properties_preserve_legacy_and_unknown_data_and_reject_parent_cycles() {
    let mut raw = source();
    raw["judgeLineList"][0]["rotateWithFather"] = json!(1);
    raw["judgeLineList"][0]["attachUI"] = json!(" Combo-Number ");
    raw["judgeLineList"][0]["future"] = json!({"textureControl":[0,null]});
    let copy = raw["judgeLineList"][0].clone();
    raw["judgeLineList"].as_array_mut().unwrap().push(copy);
    raw["judgeLineList"][1]["father"] = json!(0);
    let mut ed = editor(&raw);
    let mut props = ed.document().line_properties(0).unwrap();
    assert!(props.rotate_with_father);
    assert_eq!(props.attach_ui, Some(AttachedUi::ComboNumber));
    props.name = "Edited".into();
    props.bpm_factor = 2.;
    ed.execute(Command::EditLine {
        index: 0,
        properties: props.clone(),
    })
    .unwrap();
    let mut expected = raw.clone();
    expected["judgeLineList"][0]["Name"] = json!("Edited");
    expected["judgeLineList"][0]["bpmfactor"] = json!(2.);
    assert_eq!(ed.document().as_json(), &expected);
    props.father = 1;
    assert!(
        ed.execute(Command::EditLine {
            index: 0,
            properties: props
        })
        .is_err()
    );
    ed.undo();
    assert_eq!(ed.document().as_json(), &raw);
    assert!(ed.can_redo());
}

#[test]
fn deleting_line_reparents_children_and_undo_restores_all_indices() {
    let mut raw = source();
    let line = raw["judgeLineList"][0].clone();
    for _ in 0..3 {
        raw["judgeLineList"]
            .as_array_mut()
            .unwrap()
            .push(line.clone());
    }
    raw["judgeLineList"][1]["father"] = json!(3);
    raw["judgeLineList"][2]["father"] = json!(1);
    raw["judgeLineList"][3]["father"] = json!(0);
    let mut ed = editor(&raw);
    assert!(ed.execute(Command::DeleteLine(0)).is_err());
    ed.execute(Command::DeleteLine(1)).unwrap();
    assert_eq!(ed.document().as_json()["judgeLineList"][1]["father"], -1);
    assert_eq!(ed.document().as_json()["judgeLineList"][2]["father"], 0);
    ed.undo();
    assert_eq!(ed.document().as_json(), &raw);
    ed.execute(Command::DeleteLine(2)).unwrap();
    assert_eq!(ed.document().as_json()["judgeLineList"][1]["father"], 2);
    ed.undo();
    assert_eq!(ed.document().as_json(), &raw);
}
