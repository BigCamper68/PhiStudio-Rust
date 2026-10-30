use phistudio_core::tools::{self, CloneSpec, Selection, ValueField, ValueMode, ValueProfile};
use phistudio_core::*;
use serde_json::json;
fn beat(n: i32) -> Beat {
    Beat::new(n, 0, 1).unwrap()
}
#[test]
fn metadata_overrides_manifest_display_and_is_losslessly_reversible() {
    let mut raw = ChartDocument::empty("Source", 120.)
        .unwrap()
        .as_json()
        .clone();
    raw["META"]["unknown"] = json!({"keep":[1,2]});
    let mut e = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
    let mut o = ProjectOptions::default();
    o.authored.insert("name".into(), "Source".into());
    o.display.insert("name".into(), "Manifest".into());
    let mut meta = ChartMetadata::read(e.document(), &o);
    assert_eq!(meta.name, "Manifest");
    meta.name = "Source".into();
    meta.offset_ms = -125.5;
    e.execute(Command::EditMetadata(meta)).unwrap();
    assert_eq!(o.display_field(e.document(), "name"), "Source");
    assert_eq!(e.document().offset_seconds(), -0.1255);
    assert_eq!(
        e.document().as_json()["META"]["unknown"],
        raw["META"]["unknown"]
    );
    e.undo();
    assert_eq!(e.document().as_json(), &raw);
}

#[test]
fn whole_metadata_offsets_are_integer_json_and_read_without_a_seconds_roundtrip() {
    for ms in [150, -150, 0, 1001, i32::MIN, i32::MAX] {
        let mut raw = ChartDocument::empty("Offset", 120.)
            .unwrap()
            .as_json()
            .clone();
        raw["META"]["offset"] = json!(ms);
        raw["META"]["unknown"] = json!({"keep":[1,2]});
        let mut editor = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
        let mut meta = ChartMetadata::read(editor.document(), &Default::default());
        assert_eq!(meta.offset_ms, f64::from(ms));
        meta.name = "Edited".into();
        editor.execute(Command::EditMetadata(meta)).unwrap();
        assert_eq!(
            editor.document().as_json()["META"]["offset"].as_i64(),
            Some(i64::from(ms))
        );
        let after = editor.document().as_json().clone();
        editor.undo();
        assert_eq!(editor.document().as_json(), &raw);
        editor.redo();
        assert_eq!(editor.document().as_json(), &after);
    }
}

#[test]
fn legacy_float_offsets_save_export_and_recover_without_changing_live_data() {
    use std::io::Read;
    for ms in [150., -150., 0., 1001.] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("chart.json");
        let mut raw = ChartDocument::empty("Legacy offset", 120.)
            .unwrap()
            .as_json()
            .clone();
        raw["META"]["offset"] = json!(ms);
        raw["META"]["unknown"] = json!(["keep", 1.25]);
        let document = ChartDocument::from_value(raw.clone()).unwrap();
        std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
        save_recovery(
            root.path(),
            &RecoverySnapshot {
                document: document.clone(),
                location: Some(ProjectLocation {
                    chart: path.clone(),
                    assets: None,
                }),
                beat: 0.,
                line: 0,
            },
        )
        .unwrap();
        save_chart(&document, &path).unwrap();
        let reopened = load_chart(&path).unwrap();
        assert_eq!(
            reopened.as_json()["META"]["offset"].as_i64(),
            Some(ms as i64)
        );
        assert_eq!(reopened.offset_seconds(), document.offset_seconds());
        assert_eq!(
            reopened.as_json()["META"]["unknown"],
            raw["META"]["unknown"]
        );
        assert!(
            load_recovery(root.path()).unwrap().is_none(),
            "Completed save must not create a false recovery prompt"
        );
        let export = prepare_project_export(&document, &Default::default(), None, None).unwrap();
        let mut zip = zip::ZipArchive::new(std::fs::File::open(export.path()).unwrap()).unwrap();
        let mut text = String::new();
        zip.by_name("chart.rpe.json")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        let exported: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(exported["META"]["offset"].as_i64(), Some(ms as i64));
        assert_eq!(exported["META"]["unknown"], raw["META"]["unknown"]);
        assert_eq!(document.as_json(), &raw);
    }
}
#[test]
fn storyboard_tracks_validate_overlap_and_preserve_extensions_null_shapes_and_undo() {
    for kind in StoryKind::ALL {
        let mut raw = ChartDocument::empty("Story", 120.)
            .unwrap()
            .as_json()
            .clone();
        raw["judgeLineList"][0]["extended"] = json!(null);
        let mut e = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
        let event = StoryEvent::new(kind, beat(1), beat(2));
        e.execute(Command::EditStoryboard {
            line: 0,
            kind,
            index: None,
            event: Some(event.clone()),
        })
        .unwrap();
        assert_eq!(
            e.document().story_events(0, kind).unwrap(),
            vec![event.clone()]
        );
        let saved = e.document().as_json().clone();
        assert!(
            e.execute(Command::EditStoryboard {
                line: 0,
                kind,
                index: None,
                event: Some(event)
            })
            .is_err()
        );
        assert_eq!(e.document().as_json(), &saved);
        e.undo();
        assert_eq!(e.document().as_json(), &raw);
        e.redo();
        e.execute(Command::EditStoryboard {
            line: 0,
            kind,
            index: Some(0),
            event: None,
        })
        .unwrap();
        assert!(e.document().story_events(0, kind).unwrap().is_empty());
        e.undo();
        assert_eq!(e.document().as_json(), &saved);
    }
}
#[test]
fn formulas_match_parametric_paths_and_reject_unsafe_or_nonfinite_expressions() {
    for (text, t, value) in [
        ("400*cos(2*$Pi$*$t$)", 0.5, -400.),
        ("min(3, max(1, t*10))", 0.2, 2.),
        ("-2^2 + pow(2,3)", 0., 4.),
        ("2^3^2", 0., 512.),
        ("clamp(t,0,1)", 2., 1.),
        ("1e-3 + .5", 0., 0.501),
    ] {
        assert!(
            (Formula::parse(text).unwrap().sample(t).unwrap() - value).abs() < 1e-8,
            "{text}"
        );
    }
    for text in ["", "foo(t)", "t; exit()", "sin(t,t)", "clamp(t,1)"] {
        assert!(Formula::parse(text).is_err(), "{text}");
    }
    assert!(Formula::parse("sqrt(-1)").unwrap().sample(0.).is_err());
    assert!(Formula::parse(&"(".repeat(100)).is_err());
}
#[test]
fn clone_keeps_ordered_destinations_profiles_source_metadata_and_one_undo() {
    let mut e = Editor::new(ChartDocument::empty("Clone", 120.).unwrap());
    for name in ["One", "Two"] {
        e.execute(Command::AddLine { name: name.into() }).unwrap();
    }
    e.execute(Command::AddEvent {
        line: 0,
        layer: 0,
        kind: EventType::MoveX,
        event: LineEvent {
            start_time: beat(2),
            end_time: beat(3),
            start: 1.,
            end: 2.,
            ..Default::default()
        },
    })
    .unwrap();
    let mut raw = e.document().as_json().clone();
    raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][1]["unknown"] =
        json!({"preserved":true});
    let mut e = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
    let s = Selection {
        notes: vec![],
        events: vec![EventId {
            line: 0,
            layer: 0,
            kind: EventType::MoveX,
            index: 1,
        }],
    };
    let spec = CloneSpec {
        targets: vec![2, 1],
        delay: beat(1),
        keep_source: false,
        offsets: [vec![10., 20.], vec![0.; 2], vec![0.; 2], vec![0.; 2]],
    };
    tools::clone_events(e.document(), &s, &spec)
        .unwrap()
        .apply(&mut e)
        .unwrap();
    for (line, start, value) in [(2, 2, 11.), (1, 3, 21.)] {
        let events = e.document().events(line, 0, EventType::MoveX).unwrap();
        let (i, event) = events.last().unwrap();
        assert_eq!(event.start_time, beat(start));
        assert_eq!(event.start, value);
        assert_eq!(
            e.document()
                .event_values(line, 0, EventType::MoveX)
                .unwrap()[*i]["unknown"],
            json!({"preserved":true})
        );
    }
    assert_eq!(
        e.document().events(0, 0, EventType::MoveX).unwrap().len(),
        1
    );
    assert!(e.undo());
    assert_eq!(e.document().as_json(), &raw);
    let bad = CloneSpec {
        targets: vec![0],
        keep_source: true,
        offsets: std::array::from_fn(|_| vec![0.]),
        ..spec
    };
    let plan = tools::clone_events(e.document(), &s, &bad).unwrap();
    assert!(plan.validate_preview(e.document()).is_err());
    assert!(plan.apply(&mut e).is_err());
    assert_eq!(e.document().as_json(), &raw);
    assert!(e.can_redo());
}
#[test]
fn profiles_use_java_random_order_and_combine_curve_variation_and_multipliers() {
    let p = ValueProfile {
        from: -400.,
        to: 400.,
        sequence: vec![1., -1.],
        ..Default::default()
    };
    assert_eq!(
        p.values(5, |_, t| t).unwrap(),
        vec![-400., 200., 0., -200., 400.]
    );
    let p = ValueProfile {
        disturbance: 1.,
        seed: 42,
        ..Default::default()
    };
    let expected = [
        0.4551273600657362,
        0.36644694351969087,
        -0.38256108933468047,
    ];
    for (a, b) in p.values(3, |_, t| t).unwrap().iter().zip(expected) {
        assert!((a - b).abs() < 1e-14);
    }
}
#[test]
fn constant_values_and_automatic_following_connection_are_single_transactions() {
    let mut e = Editor::new(ChartDocument::empty("Constant", 120.).unwrap());
    for (a, b) in [(1, 2), (2, 3)] {
        e.execute(Command::AddEvent {
            line: 0,
            layer: 0,
            kind: EventType::Rotate,
            event: LineEvent {
                start_time: beat(a),
                end_time: beat(b),
                start: 10.,
                end: 20.,
                ..Default::default()
            },
        })
        .unwrap();
    }
    let raw = e.document().as_json().clone();
    let id = EventId {
        line: 0,
        layer: 0,
        kind: EventType::Rotate,
        index: 1,
    };
    let mut event = e.document().event(id).unwrap();
    event.end = 90.;
    tools::update_event_auto(e.document(), id, event, false, true)
        .unwrap()
        .apply(&mut e)
        .unwrap();
    assert_eq!(
        e.document()
            .event(EventId { index: 2, ..id })
            .unwrap()
            .start,
        90.
    );
    e.undo();
    assert_eq!(e.document().as_json(), &raw);
    let mut event = e.document().event(id).unwrap();
    event.constant = true;
    event.end = event.start;
    e.execute(Command::UpdateEvent { id, event }).unwrap();
    tools::batch_values(
        e.document(),
        &Selection {
            notes: vec![],
            events: vec![id],
        },
        ValueField::EventEnd,
        ValueMode::Set,
        &[42.],
    )
    .unwrap()
    .apply(&mut e)
    .unwrap();
    let event = e.document().event(id).unwrap();
    assert_eq!((event.start, event.end), (42., 42.));
}

#[test]
fn clone_profiles_match_original_java_all_easings_and_seeded_variation() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/java-clone-profiles.json")).unwrap();
    for case in fixture["values"].as_array().unwrap() {
        let easing = case["easing"].as_i64().unwrap() as i32;
        let p = phistudio_core::tools::ValueProfile {
            from: -150.,
            to: 300.,
            sequence: vec![1., -1., 0.25],
            disturbance: 7.5,
            seed: 42,
            easing,
        };
        let actual = p.java_values(9).unwrap();
        for (a, b) in actual.iter().zip(case["values"].as_array().unwrap()) {
            assert!(
                (a - b.as_f64().unwrap()).abs() < 1e-9,
                "Java easing {easing}: {a} != {b}"
            );
        }
    }
}
