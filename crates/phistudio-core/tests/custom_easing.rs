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
fn authoring_keeps_one_logical_event_and_has_exact_one_step_undo_redo() {
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
        assert_eq!(editor.document().event_values(0, 0, kind).unwrap().len(), 1);
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
    let mut original = editor.document().as_json().clone();
    phistudio_core::custom_easing::projection::expand(
        &mut original,
        editor.document().tempo(),
        true,
        || false,
    )
    .unwrap();
    for change_tempo in [false, true] {
        let mut raw = original.clone();
        if change_tempo {
            raw["BPMList"][0]["bpm"] = json!(123);
        } else {
            raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][1]["end"] = json!(17.);
        }
        let doc = ChartDocument::from_value(raw).unwrap();
        assert!(
            Source::capture(&doc, selection.events[0])
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
            selection.events[0]
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
fn final_hold_uses_a_projected_instant_without_extending_the_authoring_range() {
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
    assert_eq!(
        editor
            .document()
            .event_values(0, 0, EventType::MoveX)
            .unwrap()
            .len(),
        2
    );
    let projected = editor.document().render_value().unwrap();
    let track = projected["judgeLineList"][0]["eventLayers"][0]["moveXEvents"]
        .as_array()
        .unwrap();
    assert_eq!(track.last().unwrap(), &next);
    assert_eq!(track[1]["startTime"], json!([8, 0, 1]));
    assert_eq!(track[1]["endTime"], json!([8, 0, 1]));
    assert_eq!(track[1]["start"], 100.);
    assert_eq!(track[1]["end"], 100.);
    let source = Source::capture(editor.document(), selected.events[0]).unwrap();
    assert_eq!(source.members.len(), 1);
    assert_eq!(source.curve, Some(hold));
}

#[test]
fn logical_curves_roundtrip_portable_exports_and_preserve_xy_timing_links() {
    let (doc, id) = document(EventType::MoveX);
    let mut raw = doc.as_json().clone();
    raw["judgeLineList"][0]["eventLayers"][0]["moveYEvents"] =
        raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"].clone();
    let mut editor = Editor::new(ChartDocument::from_value(raw).unwrap());
    Snapshot::capture(editor.document(), &[id], editor.revision())
        .unwrap()
        .prepare(&curve(), || false)
        .unwrap()
        .apply(&mut editor, true)
        .unwrap();
    editor.document().validate_xy_layer(0, 0).unwrap();
    let original = editor.document().as_json().clone();
    let package =
        phistudio_core::prepare_project_export(editor.document(), &Default::default(), None, None)
            .unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(package.path()).unwrap()).unwrap();
    let mut bytes = String::new();
    use std::io::Read as _;
    zip.by_name("chart.rpe.json")
        .unwrap()
        .read_to_string(&mut bytes)
        .unwrap();
    let portable: serde_json::Value = serde_json::from_str(&bytes).unwrap();
    let track = portable["judgeLineList"][0]["eventLayers"][0]["moveXEvents"]
        .as_array()
        .unwrap();
    assert!(track.len() > 5);
    assert!(
        track
            .iter()
            .all(|v| v.get(phistudio_core::custom_easing::CURVE_KEY).is_none())
    );
    let restored = ChartDocument::parse(&bytes).unwrap();
    assert_eq!(restored.as_json(), &original);
    assert_eq!(editor.document().as_json(), &original);
    let root = tempfile::tempdir().unwrap();
    let imported =
        phistudio_core::import_project(package.path(), root.path(), "Curves.pez", true).unwrap();
    assert_eq!(
        phistudio_core::load_chart(&imported.charts[imported.suggested.unwrap()].path)
            .unwrap()
            .as_json(),
        &original
    );
}

#[test]
fn cropped_bindings_rebase_visible_endpoints_and_keep_equal_value_excursions() {
    use phistudio_core::custom_easing::Binding;
    let original = Binding::new(curve());
    let mut twice = original.clone();
    twice.reverse([100., 10.]);
    twice.reverse([10., 100.]);
    assert_eq!(twice, original);
    let binding = original.crop([10., 100.], 0.2, 0.8).unwrap();
    let before = [
        binding.scalar_value([10., 100.], 0.).unwrap(),
        binding.scalar_value([10., 100.], 1.).unwrap(),
    ];
    let rebased = binding.rebase(before, [25., 125.]).unwrap();
    for (x, expected) in [(0., 25.), (1., 125.)] {
        assert!((rebased.scalar_value([25., 125.], x).unwrap() - expected).abs() < 1e-8);
    }
    let mut reversed = binding.clone();
    reversed.reverse([before[1], before[0]]);
    for i in 0..=100 {
        let t = i as f64 / 100.;
        assert!(
            (binding.scalar_value(before, 1. - t).unwrap()
                - reversed.scalar_value([before[1], before[0]], t).unwrap())
            .abs()
                < 1e-9
        );
    }
    let c = Curve {
        nodes: vec![
            Node::new(0., 0., Mode::Linear),
            Node {
                x: 0.25,
                y: 0.5,
                mode: Mode::Corner,
                incoming: None,
                outgoing: Some([0.4, 1.5]),
            },
            Node {
                x: 0.75,
                y: 0.5,
                mode: Mode::Linear,
                incoming: Some([0.6, 1.5]),
                outgoing: None,
            },
            Node::new(1., 1., Mode::Linear),
        ],
        ..Default::default()
    };
    let equal = Binding::new(c).crop([0., 100.], 0.25, 0.75).unwrap();
    assert!((equal.scalar_value([50., 50.], 0.5).unwrap() - 50.).abs() > 20.);
    assert!(equal.rebase([50., 50.], [25., 26.]).is_err());
    let shifted = equal.rebase([50., 50.], [25., 25.]).unwrap();
    assert!(
        (shifted.scalar_value([25., 25.], 0.5).unwrap()
            - (equal.scalar_value([50., 50.], 0.5).unwrap() - 25.))
            .abs()
            < 1e-9
    );
}

#[test]
fn ordinary_property_edits_moves_and_bpm_changes_keep_authoritative_curve_data() {
    use phistudio_core::tools::{self, Selection, Transform};
    let (doc, id) = document(EventType::MoveX);
    let mut editor = Editor::new(doc);
    Snapshot::capture(editor.document(), &[id], editor.revision())
        .unwrap()
        .prepare(&curve(), || false)
        .unwrap()
        .apply(&mut editor, false)
        .unwrap();
    let before = editor.document().as_json().clone();
    let mut event = editor.document().event(id).unwrap();
    event.start = 20.;
    event.end = 120.;
    editor.execute(Command::UpdateEvent { id, event }).unwrap();
    assert_eq!(
        editor
            .document()
            .event(id)
            .unwrap()
            .custom_easing
            .as_ref()
            .unwrap()
            .curve,
        curve()
    );
    let selected = Selection {
        notes: vec![],
        events: vec![id],
    };
    tools::transform(editor.document(), &selected, Transform::ReverseTime)
        .unwrap()
        .apply(&mut editor)
        .unwrap();
    assert!(
        editor
            .document()
            .event(id)
            .unwrap()
            .custom_easing
            .as_ref()
            .unwrap()
            .domain[0]
            > 0.9
    );
    tools::transform(editor.document(), &selected, Transform::Easing(6))
        .unwrap()
        .apply(&mut editor)
        .unwrap();
    assert!(editor.document().event(id).unwrap().custom_easing.is_none());
    editor.undo();
    editor.undo();
    editor.undo();
    assert_eq!(editor.document().as_json(), &before);
    let mut raw = before;
    raw["BPMList"][0]["bpm"] = json!(90.);
    let changed = ChartDocument::from_value(raw).unwrap();
    assert_eq!(
        changed
            .event(id)
            .unwrap()
            .custom_easing
            .as_ref()
            .unwrap()
            .curve,
        curve()
    );
    assert!(
        changed.render_value().unwrap()["judgeLineList"][0]["eventLayers"][0]["moveXEvents"]
            .as_array()
            .unwrap()
            .len()
            > 5
    );
}

#[test]
fn package_secondary_charts_are_portable_and_opaque_resource_bytes_stay_unchanged() {
    use std::io::Read as _;
    let (doc, id) = document(EventType::MoveX);
    let mut editor = Editor::new(doc);
    Snapshot::capture(editor.document(), &[id], editor.revision())
        .unwrap()
        .prepare(&curve(), || false)
        .unwrap()
        .apply(&mut editor, false)
        .unwrap();
    let assets = tempfile::tempdir().unwrap();
    let first = assets.path().join("first.rpe.json");
    let second = assets.path().join("second.rpe.json");
    phistudio_core::save_chart(editor.document(), &first).unwrap();
    phistudio_core::save_chart(editor.document(), &second).unwrap();
    let original_second = std::fs::read(&second).unwrap();
    let opaque = b"{\n  \"arbitrary\": {\"phiStudioCustomEasing\": [1, 2, 3]}\n}\n";
    std::fs::write(assets.path().join("opaque.json"), opaque).unwrap();
    let export = phistudio_core::prepare_project_export(
        editor.document(),
        &Default::default(),
        Some(assets.path()),
        Some(&first),
    )
    .unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(export.path()).unwrap()).unwrap();
    for name in ["first.rpe.json", "second.rpe.json"] {
        let mut text = String::new();
        zip.by_name(name)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        let raw: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(
            raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"]
                .as_array()
                .unwrap()
                .len()
                > 5
        );
        assert_eq!(
            ChartDocument::parse(&text).unwrap().as_json(),
            editor.document().as_json()
        );
    }
    let mut bytes = vec![];
    zip.by_name("opaque.json")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, opaque);
    assert_eq!(std::fs::read(second).unwrap(), original_second);
}

#[test]
fn curve_projection_never_inserts_absent_fields_into_sparse_imports() {
    let mut raw = ChartDocument::empty("Sparse", 120.)
        .unwrap()
        .as_json()
        .clone();
    raw["judgeLineList"][0]["eventLayers"] = json!([{}, null]);
    let imported = ChartDocument::from_value(raw.clone()).unwrap();
    assert_eq!(imported.as_json(), &raw);
    assert_eq!(imported.render_value().unwrap(), raw);
    raw["judgeLineList"][0]
        .as_object_mut()
        .unwrap()
        .shift_remove("eventLayers");
    assert_eq!(
        ChartDocument::from_value(raw.clone()).unwrap().as_json(),
        &raw
    );
}

#[test]
fn explicit_custom_speed_invalidates_only_the_previous_pec_height_override() {
    use phistudio_core::{
        NoteId, decode_chart, pec_height_signature, speed_curve::SpeedCurveSource,
    };
    let decoded = decode_chart(include_str!("fixtures/conversion/pec-minimum.pec")).unwrap();
    let mut editor = Editor::new(decoded.document);
    let before = editor.document().as_json().clone();
    assert!(pec_height_signature(&before).is_some());
    assert!(SpeedCurveSource::capture(editor.document(), NoteId { line: 0, index: 0 }).is_err());
    let id = EventId {
        line: 0,
        layer: 0,
        kind: EventType::Speed,
        index: 0,
    };
    Snapshot::capture(editor.document(), &[id], editor.revision())
        .unwrap()
        .prepare(&Curve::default(), || false)
        .unwrap()
        .apply(&mut editor, false)
        .unwrap();
    assert!(pec_height_signature(editor.document().as_json()).is_none());
    assert!(SpeedCurveSource::capture(editor.document(), NoteId { line: 0, index: 0 }).is_ok());
    editor.undo();
    assert_eq!(editor.document().as_json(), &before);
    assert!(pec_height_signature(editor.document().as_json()).is_some());
}

#[test]
fn join_checks_actual_custom_geometry_and_batch_easing_explicitly_replaces_it() {
    use phistudio_core::tools::{self, Selection, ValueField, ValueMode};
    for curved in [false, true] {
        let (doc, id) = document(EventType::MoveX);
        let mut raw = doc.as_json().clone();
        raw["BPMList"] = json!([{"bpm":120,"startTime":[0,0,1]}]);
        let mut second = raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0].clone();
        second["startTime"] = json!([8, 0, 1]);
        second["endTime"] = json!([16, 0, 1]);
        second["start"] = json!(100.);
        second["end"] = json!(190.);
        raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"]
            .as_array_mut()
            .unwrap()
            .push(second);
        let mut editor = Editor::new(ChartDocument::from_value(raw).unwrap());
        let ids = [id, EventId { index: 1, ..id }];
        let selected = Selection {
            notes: vec![],
            events: ids.to_vec(),
        };
        let curve = if curved { curve() } else { Curve::default() };
        Snapshot::capture(editor.document(), &ids, editor.revision())
            .unwrap()
            .prepare(&curve, || false)
            .unwrap()
            .apply(&mut editor, false)
            .unwrap();
        let original = editor.document().as_json().clone();
        if curved {
            assert!(tools::join_events(editor.document(), &selected).is_err());
            tools::batch_values(
                editor.document(),
                &selected,
                ValueField::EventEasing,
                ValueMode::Set,
                &[1.],
            )
            .unwrap()
            .apply(&mut editor)
            .unwrap();
            assert!(
                editor
                    .document()
                    .events(0, 0, EventType::MoveX)
                    .unwrap()
                    .into_iter()
                    .all(|(_, e)| e.custom_easing.is_none())
            );
        }
        tools::join_events(editor.document(), &selected)
            .unwrap()
            .apply(&mut editor)
            .unwrap();
        assert_eq!(
            editor
                .document()
                .event_values(0, 0, EventType::MoveX)
                .unwrap()
                .len(),
            1
        );
        editor.undo();
        if curved {
            editor.undo();
        }
        assert_eq!(editor.document().as_json(), &original);
    }
}
