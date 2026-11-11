use phistudio_core::{
    Beat, ChartDocument, ChartError, Command, Editor, EventType, LineEvent, Note, NoteId,
    note_tracing::{Snapshot, Spec, TracePoint, sampling_times},
};
use serde_json::json;

fn fixture() -> Editor {
    let mut raw = ChartDocument::empty("Trace", 120.)
        .unwrap()
        .as_json()
        .clone();
    raw["judgeLineList"][0]["notes"] = json!([Note {
        start_time: beat(4),
        end_time: beat(4),
        ..Default::default()
    }]);
    raw["judgeLineList"][0]["notes"][0]["unknown"] = json!({"preserve": [1, 2, 3]});
    Editor::new(ChartDocument::from_value(raw).unwrap())
}
fn beat(n: i64) -> Beat {
    n.to_string().parse().unwrap()
}
fn spec() -> Spec {
    Spec {
        source: NoteId { line: 0, index: 0 },
        destination: None,
        layer: 0,
        range: [Beat::ZERO, beat(4)],
        sample_rate: 120,
        rotation: true,
        replace: false,
        position_tolerance: 0.15,
        rotation_tolerance: 0.015,
    }
}
fn points() -> Vec<TracePoint> {
    (0..=240)
        .map(|i| {
            let seconds = f64::from(i) / 120.;
            TracePoint {
                seconds,
                values: [seconds * 100., seconds * 20., seconds * 720.],
                break_before: false,
            }
        })
        .collect()
}
fn slice(_: EventType, event: &LineEvent, a: f64, b: f64) -> Result<LineEvent, ChartError> {
    assert_eq!(event.easing_type, 1);
    Ok(LineEvent {
        start: event.start + (event.end - event.start) * a,
        end: event.start + (event.end - event.start) * b,
        ..event.clone()
    })
}
#[test]
fn new_line_is_one_transaction_and_preserves_multiturn_rotation_and_source() {
    for layer in [0, 3] {
        let mut editor = fixture();
        let before = editor.document().as_json().clone();
        let spec = Spec { layer, ..spec() };
        let edit = Snapshot::capture(editor.document(), spec)
            .unwrap()
            .prepare(editor.revision(), &points(), slice, || false)
            .unwrap();
        assert_eq!(edit.event_count(), 3);
        let selection = edit.apply(&mut editor).unwrap();
        assert_eq!(selection.events.len(), 3);
        assert_eq!(editor.document().line_count(), 2);
        assert_eq!(
            editor.document().as_json()["judgeLineList"][0],
            before["judgeLineList"][0]
        );
        let rotation = editor
            .document()
            .event_values(1, layer, EventType::Rotate)
            .unwrap();
        assert_eq!(rotation[0]["end"], 1440.);
        let after = editor.document().as_json().clone();
        editor.undo();
        assert_eq!(editor.document().as_json(), &before);
        editor.redo();
        assert_eq!(editor.document().as_json(), &after);
    }
}
#[test]
fn replacement_crops_both_sides_preserves_extensions_and_filters_selection() {
    let editor = fixture();
    let mut raw = editor.document().as_json().clone();
    for kind in [EventType::MoveX, EventType::MoveY, EventType::Rotate] {
        let event = &mut raw["judgeLineList"][0]["eventLayers"][0][kind.key()][0];
        event["endTime"] = json!([8, 0, 1]);
        event["end"] = json!(800.);
        event["unknown"] = json!({"curve": "metadata"});
    }
    let mut editor = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
    let spec = Spec {
        destination: Some(0),
        range: [beat(1), beat(3)],
        replace: true,
        ..spec()
    };
    let points: Vec<_> = points()
        .into_iter()
        .filter(|p| p.seconds >= 0.5 && p.seconds <= 1.5)
        .collect();
    let selection = Snapshot::capture(editor.document(), spec)
        .unwrap()
        .prepare(editor.revision(), &points, slice, || false)
        .unwrap()
        .apply(&mut editor)
        .unwrap();
    assert_eq!(selection.events.len(), 3);
    for kind in [EventType::MoveX, EventType::MoveY, EventType::Rotate] {
        let events = editor.document().event_values(0, 0, kind).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["end"], 100.);
        assert_eq!(events[2]["start"], 300.);
        for index in [0, 2] {
            assert_eq!(events[index]["unknown"], json!({"curve": "metadata"}));
        }
    }
    editor.undo();
    assert_eq!(editor.document().as_json(), &raw);
}
#[test]
fn discontinuities_are_not_interpolated_and_terminal_values_survive() {
    let mut editor = fixture();
    let values = [
        (0., 0., false),
        (1., 10., false),
        (1., 100., true),
        (2., 110., false),
        (2., 200., true),
    ];
    let points: Vec<_> = values
        .into_iter()
        .map(|(seconds, value, break_before)| TracePoint {
            seconds,
            values: [value; 3],
            break_before,
        })
        .collect();
    Snapshot::capture(editor.document(), spec())
        .unwrap()
        .prepare(editor.revision(), &points, slice, || false)
        .unwrap()
        .apply(&mut editor)
        .unwrap();
    let events = editor
        .document()
        .event_values(1, 0, EventType::MoveX)
        .unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0]["end"], 10.);
    assert_eq!(events[1]["start"], 100.);
    assert_eq!(events[2]["start"], 200.);
    assert_eq!(events[2]["startTime"], json!([4, 0, 1]));
    assert_eq!(events[2]["endTime"], json!([5, 0, 1]));
}
#[test]
fn retained_right_event_owns_final_boundary() {
    let mut raw = fixture().document().as_json().clone();
    let event = json!({"startTime":[4,0,1], "endTime":[8,0,1], "start":77., "end":99.});
    raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"]
        .as_array_mut()
        .unwrap()
        .push(event.clone());
    let mut editor = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
    let mut points = points();
    points.push(TracePoint {
        seconds: 2.,
        values: [999.; 3],
        break_before: true,
    });
    Snapshot::capture(
        editor.document(),
        Spec {
            destination: Some(0),
            replace: true,
            ..spec()
        },
    )
    .unwrap()
    .prepare(editor.revision(), &points, slice, || false)
    .unwrap()
    .apply(&mut editor)
    .unwrap();
    let events = editor
        .document()
        .event_values(0, 0, EventType::MoveX)
        .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1], event);
    editor.undo();
    assert_eq!(editor.document().as_json(), &raw);
}
#[test]
fn conflicts_and_stale_results_cannot_mutate_document_or_history() {
    let mut editor = fixture();
    let before = editor.document().as_json().clone();
    assert!(
        Snapshot::capture(
            editor.document(),
            Spec {
                destination: Some(0),
                ..spec()
            }
        )
        .unwrap()
        .prepare(editor.revision(), &points(), slice, || false)
        .is_err()
    );
    assert_eq!(editor.document().as_json(), &before);
    let edit = Snapshot::capture(editor.document(), spec())
        .unwrap()
        .prepare(editor.revision(), &points(), slice, || false)
        .unwrap();
    editor
        .execute(Command::RenameChart("Changed".into()))
        .unwrap();
    let changed = editor.document().as_json().clone();
    let revision = editor.revision();
    assert!(edit.apply(&mut editor).is_err());
    assert_eq!(editor.revision(), revision);
    assert_eq!(editor.document().as_json(), &changed);
    editor.undo();
    assert_eq!(editor.document().as_json(), &before);
}
#[test]
fn cancellation_nonfinite_unordered_and_incomplete_samples_are_rejected() {
    let editor = fixture();
    assert!(
        Snapshot::capture(editor.document(), spec())
            .unwrap()
            .prepare(0, &points(), slice, || true)
            .is_err()
    );
    for mutation in 0..4 {
        let mut points = points();
        match mutation {
            0 => points[5].values[0] = f64::NAN,
            1 => points.swap(5, 6),
            2 => {
                points.remove(0);
            }
            _ => points[4].seconds = points[3].seconds,
        }
        assert!(
            Snapshot::capture(editor.document(), spec())
                .unwrap()
                .prepare(0, &points, slice, || false)
                .is_err()
        );
    }
}
#[test]
fn sampling_preserves_bpm_changes_and_both_sides_of_native_boundaries() {
    let mut raw = fixture().document().as_json().clone();
    raw["BPMList"] = json!([{"startTime":[0,0,1],"bpm":120.}, {"startTime":[2,0,1],"bpm":240.}]);
    let document = ChartDocument::from_value(raw).unwrap();
    let times = sampling_times(&spec(), document.tempo(), &[0.7]).unwrap();
    assert_eq!(times.first().unwrap().seconds, 0.);
    assert_eq!(times.last().unwrap().seconds, 1.5);
    for cut in [0.7, 1., 1.5] {
        let at: Vec<_> = times.iter().filter(|t| t.seconds == cut).collect();
        assert_eq!(at.len(), 2);
        assert!(at[0].sample_seconds < cut && !at[0].break_before);
        assert!(at[1].break_before && at[1].sample_seconds == cut);
    }
    assert!(
        sampling_times(
            &Spec {
                range: [Beat::ZERO, beat(100_000)],
                ..spec()
            },
            document.tempo(),
            &[]
        )
        .is_err()
    );
}
#[test]
fn native_components_follow_sparse_layers_instead_of_raw_indices() {
    let mut raw = fixture().document().as_json().clone();
    raw["judgeLineList"][0]["eventLayers"] =
        json!([{"moveXEvents": []}, null, {"moveYEvents":[], "rotateEvents":[]}]);
    let document = ChartDocument::from_value(raw).unwrap();
    let snapshot = Snapshot::capture(
        &document,
        Spec {
            destination: Some(0),
            layer: 2,
            ..spec()
        },
    )
    .unwrap();
    assert_eq!(snapshot.native_components, [None, Some(0), Some(0)]);
}
#[test]
fn positive_first_bpm_keeps_negative_native_seconds_valid() {
    let mut raw = fixture().document().as_json().clone();
    raw["BPMList"] = json!([{"startTime":[4,0,1],"bpm":120.}]);
    let document = ChartDocument::from_value(raw).unwrap();
    let times = sampling_times(&spec(), document.tempo(), &[]).unwrap();
    assert_eq!(times[0].seconds, -2.);
    let samples: Vec<_> = times
        .iter()
        .map(|t| TracePoint {
            seconds: t.seconds,
            values: [t.seconds; 3],
            break_before: t.break_before,
        })
        .collect();
    let edit = Snapshot::capture(&document, spec())
        .unwrap()
        .prepare(0, &samples, slice, || false)
        .unwrap();
    let mut editor = Editor::new(document);
    edit.apply(&mut editor).unwrap();
    assert_eq!(
        editor
            .document()
            .event_values(1, 0, EventType::MoveX)
            .unwrap()[0]["startTime"],
        json!([0, 0, 1])
    );
}
#[test]
fn position_only_trace_preserves_rotation_and_detaches_only_affected_recipes() {
    let mut raw = fixture().document().as_json().clone();
    let generated = raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0].clone();
    let rotation = raw["judgeLineList"][0]["eventLayers"][0]["rotateEvents"].clone();
    raw["judgeLineList"][0]["phiStudioTrajectories"] = json!([
        {"id":"affected","layer":0,"generated":[{"kind":EventType::MoveX,"value":generated}]},
        {"id":"preserve","layer":3,"generated":[],"unknown":{"keep":true}}
    ]);
    let mut editor = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
    let spec = Spec {
        destination: Some(0),
        rotation: false,
        replace: true,
        ..spec()
    };
    Snapshot::capture(editor.document(), spec)
        .unwrap()
        .prepare(editor.revision(), &points(), slice, || false)
        .unwrap()
        .apply(&mut editor)
        .unwrap();
    assert_eq!(
        editor.document().as_json()["judgeLineList"][0]["eventLayers"][0]["rotateEvents"],
        rotation
    );
    assert_eq!(
        editor.document().as_json()["judgeLineList"][0]["phiStudioTrajectories"],
        json!([raw["judgeLineList"][0]["phiStudioTrajectories"][1]])
    );
    let after = editor.document().as_json().clone();
    editor.undo();
    assert_eq!(editor.document().as_json(), &raw);
    editor.redo();
    assert_eq!(editor.document().as_json(), &after);
}
