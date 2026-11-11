use phistudio_core::{
    Beat, ChartDocument, Command, Editor, EventId, EventType, LineEvent,
    custom_easing::{Curve, CurveLibrary, Mode, Node, RECIPE_KEY, Snapshot, Source},
};
use serde_json::json;

fn sample(curve: &Curve, x: f64) -> f64 {
    if x >= 1. {
        return 1.;
    }
    let segments = curve.segments().unwrap();
    let segment = segments.iter().find(|s| x < s.points[3][0]).unwrap();
    segment.point(segment.parameter_at(x))[1]
}
fn curve() -> Curve {
    Curve {
        version: 1,
        name: "Motion".into(),
        nodes: vec![
            Node {
                x: 0.,
                y: 0.,
                mode: Mode::Corner,
                incoming: None,
                outgoing: Some([0.1, 1.5]),
            },
            Node {
                x: 0.55,
                y: 0.55,
                mode: Mode::Corner,
                incoming: Some([0.4, -0.25]),
                outgoing: Some([0.7, 1.4]),
            },
            Node {
                x: 1.,
                y: 1.,
                mode: Mode::Corner,
                incoming: Some([0.9, 0.6]),
                outgoing: None,
            },
        ],
    }
}
fn document(kind: EventType) -> (ChartDocument, EventId) {
    let mut raw = ChartDocument::empty("Easing", 120.)
        .unwrap()
        .as_json()
        .clone();
    raw["BPMList"] = json!([{"bpm":120,"startTime":[0,0,1]},{"bpm":75,"startTime":[3,0,1]}]);
    let event = LineEvent {
        end_time: Beat::new(8, 0, 1).unwrap(),
        start: 10.,
        end: 100.,
        link_group: 4,
        ..Default::default()
    };
    raw["judgeLineList"][0]["eventLayers"][0][kind.key()] = json!([event]);
    raw["judgeLineList"][0]["eventLayers"][0][kind.key()][0]["unknownVendorData"] =
        json!({"keep":[1,2,3]});
    (
        ChartDocument::from_value(raw).unwrap(),
        EventId {
            line: 0,
            layer: 0,
            kind,
            index: 0,
        },
    )
}

#[test]
fn inserting_nodes_preserves_full_cubic_and_automatic_shapes() {
    for original in [
        curve(),
        Curve {
            nodes: vec![
                Node::new(0., 0., Mode::Auto),
                Node::new(0.4, 0.7, Mode::Auto),
                Node::new(1., 1., Mode::Auto),
            ],
            ..Default::default()
        },
    ] {
        let mut edited = original.clone();
        for x in [0.2, 0.5, 0.9] {
            edited = edited.insert(x).unwrap().0;
        }
        for i in 0..=1000 {
            let x = i as f64 / 1000.;
            assert!(
                (sample(&edited, x) - sample(&original, x)).abs() < 1e-10,
                "x={x}"
            );
        }
        assert_eq!(edited.nodes.len(), original.nodes.len() + 3);
        assert!(edited.insert(0.).is_err());
        assert!(edited.insert(0.5).is_err());
    }
    let hold = Curve {
        nodes: vec![
            Node::new(0., 0., Mode::Hold),
            Node::new(1., 1., Mode::Linear),
        ],
        ..Default::default()
    };
    let inserted = hold.insert(0.5).unwrap().0;
    assert_eq!(sample(&inserted, 0.75), 0.);
    assert_eq!(sample(&inserted, 1.), 1.);
}

#[test]
fn node_and_handle_edits_keep_endpoints_ordering_and_smooth_tangents() {
    let initial = curve();
    let moved = initial.move_nodes(&[0, 1, 2], 10., -0.2).unwrap();
    assert_eq!(moved.nodes[0].point(), [0., 0.]);
    assert_eq!(moved.nodes[2].point(), [1., 1.]);
    assert!(moved.nodes[1].x < 1. && moved.nodes[1].x > 0.55);
    assert!((moved.nodes[1].y - 0.35).abs() < 1e-12);
    let smooth = initial
        .set_mode(&[1], Mode::Smooth)
        .unwrap()
        .move_handle(1, true, [0.75, 1.2], false)
        .unwrap();
    let n = &smooth.nodes[1];
    let a = n.incoming.unwrap();
    let b = n.outgoing.unwrap();
    assert!(((a[0] - n.x) * (b[1] - n.y) - (a[1] - n.y) * (b[0] - n.x)).abs() < 1e-10);
    let broken = smooth.move_handle(1, false, [0.2, -2.], true).unwrap();
    assert_eq!(broken.nodes[1].mode, Mode::Corner);
    assert_eq!(broken.nodes[1].outgoing, n.outgoing);
    let deleted = broken.delete_nodes(&[0, 1, 2]).unwrap();
    assert_eq!(deleted.nodes.len(), 2);
    assert!(broken.move_handle(0, false, [0., 0.], false).is_err());
    assert!(initial.move_nodes(&[1], f64::NAN, 0.).is_err());
}

#[test]
fn control_hulls_cover_narrow_excursions_and_extrema_include_symmetric_roots() {
    let c = Curve {
        nodes: vec![
            Node {
                x: 0.,
                y: 0.,
                mode: Mode::Corner,
                outgoing: Some([0.000001, 2.]),
                incoming: None,
            },
            Node {
                x: 1.,
                y: 1.,
                mode: Mode::Corner,
                incoming: Some([0.000002, -1.]),
                outgoing: None,
            },
        ],
        ..Default::default()
    };
    let segment = c.segments().unwrap()[0];
    assert!(segment.chord_error() > 1.9);
    assert_eq!(segment.extrema().len(), 2);
    for t in segment.extrema() {
        let a = segment.point(t - 1e-7);
        let b = segment.point(t + 1e-7);
        assert!((a[1] - b[1]).abs() < 1e-10);
    }
    let symmetric = phistudio_core::custom_easing::Segment {
        points: [[0., 0.], [0.25, 1.], [0.75, 1.], [1., 0.]],
        linear: false,
        hold: false,
    };
    assert_eq!(symmetric.extrema(), vec![0.5]);
}

#[test]
fn portable_templates_roundtrip_and_import_merge_is_atomic() {
    let mut library = CurveLibrary::default();
    let id = library.save("Motion", curve(), None).unwrap();
    library.save("Motion", Curve::default(), None).unwrap();
    assert_eq!(library.templates[1].name, "Motion (2)");
    library.save("New motion", curve(), Some(&id)).unwrap();
    assert_eq!(library.templates.len(), 2);
    let exported = library.export().unwrap();
    assert_eq!(CurveLibrary::parse(&exported).unwrap(), library);
    let compatible = String::from_utf8(exported)
        .unwrap()
        .replace("phistudio-animation-curves", "rpen-animation-curves");
    let imported = CurveLibrary::parse(compatible.as_bytes()).unwrap();
    library.merge(&imported).unwrap();
    assert_eq!(library.templates.len(), 4);
    assert_eq!(library.templates[2].name, "New motion (2)");
    let before = library.clone();
    let mut invalid = imported;
    invalid.templates[0].curve.nodes[0].x = 0.1;
    assert!(library.merge(&invalid).is_err());
    assert_eq!(library, before);
    assert!(CurveLibrary::parse(&vec![b' '; 8 * 1024 * 1024 + 1]).is_err());
    let mut duplicate = library.clone();
    duplicate.templates[1].id = duplicate.templates[0].id.clone();
    assert!(duplicate.validate().is_err());
}

#[test]
fn authoring_reopens_whole_groups_and_has_exact_one_step_undo_redo() {
    for kind in EventType::ALL {
        let (doc, id) = document(kind);
        let before = doc.as_json().clone();
        let mut editor = Editor::new(doc);
        let c = curve();
        let pending = Snapshot::capture(editor.document(), &[id], editor.revision())
            .unwrap()
            .prepare(&c, || false)
            .unwrap();
        assert!(pending.event_count() > 5);
        assert_eq!(editor.document().as_json(), &before);
        let selection = pending.apply(&mut editor, false).unwrap().unwrap();
        let after = editor.document().as_json().clone();
        for id in [
            selection.events.first().unwrap(),
            &selection.events[selection.events.len() / 2],
            selection.events.last().unwrap(),
        ] {
            let source = Source::capture(editor.document(), *id).unwrap();
            assert_eq!(source.curve, Some(c.clone()));
            assert_eq!(source.members, selection.events);
            assert_eq!(source.raw["unknownVendorData"], json!({"keep":[1,2,3]}));
        }
        assert_eq!(editor.undo_label(), Some("Apply custom easing"));
        editor.undo();
        assert_eq!(editor.document().as_json(), &before);
        editor.redo();
        assert_eq!(editor.document().as_json(), &after);
        let changed = c.set_mode(&[0, 1], Mode::Linear).unwrap();
        let next = Snapshot::capture(editor.document(), &selection.events, editor.revision())
            .unwrap()
            .prepare(&changed, || false)
            .unwrap();
        next.apply(&mut editor, false).unwrap();
        assert_eq!(editor.document().event_values(0, 0, kind).unwrap().len(), 2);
        editor.undo();
        assert_eq!(editor.document().as_json(), &after);
    }
}

#[test]
fn changed_members_or_tempo_detach_a_saved_curve_without_trusting_stale_metadata() {
    let (doc, id) = document(EventType::MoveX);
    let mut editor = Editor::new(doc);
    let selection = Snapshot::capture(editor.document(), &[id], editor.revision())
        .unwrap()
        .prepare(&curve(), || false)
        .unwrap()
        .apply(&mut editor, false)
        .unwrap()
        .unwrap();
    let original = editor.document().as_json().clone();
    for change_tempo in [false, true] {
        let mut raw = original.clone();
        if change_tempo {
            raw["BPMList"][0]["bpm"] = json!(123);
        } else {
            raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][1]["end"] = json!(17.);
        }
        let doc = ChartDocument::from_value(raw).unwrap();
        assert!(
            Source::capture(&doc, selection.events[1])
                .unwrap()
                .curve
                .is_none()
        );
    }
    let mut raw = original.clone();
    raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0][RECIPE_KEY]["curve"]["name"] =
        json!("Tampered");
    let detached = Source::capture(
        &ChartDocument::from_value(raw).unwrap(),
        selection.events[0],
    )
    .unwrap();
    assert!(detached.curve.is_none());
    assert!(detached.raw.get(RECIPE_KEY).is_none());
    let mut raw = original;
    raw["META"]["name"] = json!("Outside edit");
    assert!(
        Source::capture(
            &ChartDocument::from_value(raw).unwrap(),
            selection.events[1]
        )
        .unwrap()
        .curve
        .is_some()
    );
}

#[test]
fn cancellation_stale_edits_and_native_timing_failures_are_atomic() {
    let (doc, id) = document(EventType::MoveX);
    let mut editor = Editor::new(doc);
    assert!(
        Snapshot::capture(editor.document(), &[id], editor.revision())
            .unwrap()
            .prepare(&curve(), || true)
            .is_err()
    );
    let pending = Snapshot::capture(editor.document(), &[id], editor.revision())
        .unwrap()
        .prepare(&curve(), || false)
        .unwrap();
    editor
        .execute(Command::RenameChart("Changed".into()))
        .unwrap();
    let before = editor.document().as_json().clone();
    assert!(pending.apply(&mut editor, false).is_err());
    assert_eq!(editor.document().as_json(), &before);
    let mut close = Curve::default();
    close.nodes.insert(1, Node::new(1e-20, 0.5, Mode::Linear));
    assert!(
        Snapshot::capture(editor.document(), &[id], editor.revision())
            .unwrap()
            .prepare(&close, || false)
            .is_err()
    );
    assert_eq!(editor.document().as_json(), &before);
}

#[test]
fn final_hold_preserves_untouched_following_events_and_reopens_continuations() {
    let (doc, id) = document(EventType::MoveX);
    let mut raw = doc.as_json().clone();
    let mut next = raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0].clone();
    next["startTime"] = json!([10, 0, 1]);
    next["endTime"] = json!([12, 0, 1]);
    next["start"] = json!(200.);
    next["end"] = json!(300.);
    // Imported array order may differ from timeline order.
    raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"]
        .as_array_mut()
        .unwrap()
        .insert(0, next.clone());
    let mut editor = Editor::new(ChartDocument::from_value(raw).unwrap());
    let hold = Curve {
        nodes: vec![
            Node::new(0., 0., Mode::Hold),
            Node::new(1., 1., Mode::Linear),
        ],
        ..Default::default()
    };
    let selected = Snapshot::capture(
        editor.document(),
        &[EventId { index: 1, ..id }],
        editor.revision(),
    )
    .unwrap()
    .prepare(&hold, || false)
    .unwrap()
    .apply(&mut editor, false)
    .unwrap()
    .unwrap();
    let track = editor
        .document()
        .event_values(0, 0, EventType::MoveX)
        .unwrap();
    assert_eq!(track.last().unwrap(), &next);
    assert_eq!(track[1]["startTime"], json!([8, 0, 1]));
    assert_eq!(track[1]["endTime"], json!([10, 0, 1]));
    assert_eq!(track[1]["start"], 100.);
    assert_eq!(track[1]["end"], 100.);
    let source = Source::capture(editor.document(), selected.events[1]).unwrap();
    assert_eq!(source.members.len(), 2);
    assert_eq!(source.curve, Some(hold));
}
