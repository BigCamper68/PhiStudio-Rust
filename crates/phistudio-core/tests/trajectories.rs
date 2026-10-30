use phistudio_core::{
    ChartDocument, Command, Editor, EventType,
    trajectories::{self, Shape, Spec},
};
fn linear(_: i32, t: f64) -> f64 {
    t
}
#[test]
fn all_shapes_are_finite_and_transform_consistently() {
    for shape in Shape::ALL {
        let mut spec = Spec {
            shape,
            ..Default::default()
        };
        if shape == Shape::Polar {
            spec.expressions = ["200".into(), "2*pi*t".into()];
        }
        let compiled = spec.compile().unwrap();
        for i in 0..=200 {
            assert!(
                compiled
                    .sample(i as f64 / 200., &linear)
                    .unwrap()
                    .iter()
                    .all(|v| v.is_finite()),
                "{shape:?}"
            );
        }
        let p = compiled.point(0.35, &linear).unwrap();
        spec.center = [50., -30.];
        let q = spec.compile().unwrap().point(0.35, &linear).unwrap();
        assert!((q[0] - p[0] - 50.).abs() < 1e-8);
        assert!((q[1] - p[1] + 30.).abs() < 1e-8);
    }
}
#[test]
fn recipe_rebuild_undo_and_stale_edits_preserve_raw_data() {
    let doc = ChartDocument::empty("Raw name", 120.).unwrap();
    let original = doc.as_json().clone();
    let mut editor = Editor::new(doc);
    let spec = Spec {
        tangent: true,
        ..Default::default()
    };
    let (edit, count) = trajectories::prepare(
        editor.document(),
        0,
        1,
        None,
        "Spiral copy",
        spec.clone(),
        linear,
    )
    .unwrap();
    assert!(count > 0);
    edit.apply(&mut editor).unwrap();
    let first = editor.document().as_json().clone();
    let saved = trajectories::recipes(editor.document(), 0).unwrap();
    assert_eq!(saved[0].name, "Spiral copy");
    let mut changed = spec;
    changed.radius = [450., 300.];
    trajectories::prepare(
        editor.document(),
        0,
        1,
        Some(&saved[0].id),
        "Revised",
        changed,
        linear,
    )
    .unwrap()
    .0
    .apply(&mut editor)
    .unwrap();
    assert_ne!(editor.document().as_json(), &first);
    editor.undo();
    assert_eq!(editor.document().as_json(), &first);
    editor.undo();
    assert_eq!(editor.document().as_json(), &original);
    editor.redo();
    assert_eq!(editor.document().as_json(), &first);
    let id = phistudio_core::EventId {
        line: 0,
        layer: 1,
        kind: EventType::MoveX,
        index: 0,
    };
    let mut event = editor.document().event(id).unwrap();
    event.start += 10.;
    editor.execute(Command::UpdateEvent { id, event }).unwrap();
    let before = editor.document().as_json().clone();
    let revision = editor.revision();
    assert!(
        trajectories::prepare(
            editor.document(),
            0,
            1,
            Some(&saved[0].id),
            "Unsafe",
            Spec::default(),
            linear
        )
        .is_err()
    );
    assert_eq!(editor.document().as_json(), &before);
    assert_eq!(editor.revision(), revision);
    trajectories::detach(editor.document(), 0, &saved[0].id, false)
        .unwrap()
        .apply(&mut editor)
        .unwrap();
    assert!(
        trajectories::recipes(editor.document(), 0)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        editor.document().as_json()["judgeLineList"][0]["eventLayers"],
        before["judgeLineList"][0]["eventLayers"]
    );
}

#[test]
fn approximation_respects_seconds_and_tolerance_across_tempo_changes() {
    let mut root = ChartDocument::empty("Tempo", 120.)
        .unwrap()
        .as_json()
        .clone();
    root["BPMList"] =
        serde_json::json!([{"startTime":[0,0,1],"bpm":120},{"startTime":[2,0,1],"bpm":37}]);
    let doc = ChartDocument::from_value(root).unwrap();
    let spec = Spec {
        shape: Shape::Lissajous,
        tangent: true,
        ..Default::default()
    };
    let exact = Spec {
        simplify: false,
        ..spec.clone()
    };
    let mut editor = Editor::new(doc.clone());
    let (edit, simple_count) =
        trajectories::prepare(&doc, 0, 1, None, "Simple", spec.clone(), linear).unwrap();
    let points = edit.points().to_vec();
    edit.apply(&mut editor).unwrap();
    let mut reference = Editor::new(doc.clone());
    let (edit, raw_count) =
        trajectories::prepare(&doc, 0, 1, None, "Exact", exact, linear).unwrap();
    let raw_points = edit.points().to_vec();
    edit.apply(&mut reference).unwrap();
    assert!(simple_count <= raw_count);
    for (a, b) in points.iter().zip(&raw_points) {
        for axis in 0..2 {
            assert!((a[axis] - b[axis]).abs() <= spec.tolerance[axis] + 1e-8);
        }
    }
    for axis in 0..3 {
        let kind = [EventType::MoveX, EventType::MoveY, EventType::Rotate][axis];
        let events = editor.document().event_values(0, 1, kind).unwrap();
        for value in reference.document().event_values(0, 1, kind).unwrap() {
            let beat: phistudio_core::Beat =
                serde_json::from_value(value["startTime"].clone()).unwrap();
            let candidate = events
                .iter()
                .find(|v| {
                    let start: phistudio_core::Beat =
                        serde_json::from_value(v["startTime"].clone()).unwrap();
                    let end: phistudio_core::Beat =
                        serde_json::from_value(v["endTime"].clone()).unwrap();
                    start <= beat && end >= beat
                })
                .unwrap();
            let a: phistudio_core::Beat =
                serde_json::from_value(candidate["startTime"].clone()).unwrap();
            let b: phistudio_core::Beat =
                serde_json::from_value(candidate["endTime"].clone()).unwrap();
            let tempo = doc.tempo();
            let t = (tempo.seconds_at(beat.as_f64()) - tempo.seconds_at(a.as_f64()))
                / (tempo.seconds_at(b.as_f64()) - tempo.seconds_at(a.as_f64()));
            let actual = candidate["start"].as_f64().unwrap()
                + t * (candidate["end"].as_f64().unwrap() - candidate["start"].as_f64().unwrap());
            assert!(
                (actual - value["start"].as_f64().unwrap()).abs() <= spec.tolerance[axis] + 1e-8
            );
        }
    }
}

#[test]
fn invalid_or_overlapping_generation_is_atomic_and_unchanged_rebuild_is_noop() {
    let mut editor = Editor::new(ChartDocument::empty("Atomic", 120.).unwrap());
    let before = editor.document().as_json().clone();
    let revision = editor.revision();
    assert!(
        trajectories::prepare(
            editor.document(),
            0,
            0,
            None,
            "Overlap",
            Spec::default(),
            linear
        )
        .is_err()
    );
    let bad = Spec {
        expressions: ["1/0".into(), "0".into()],
        shape: Shape::Formula,
        ..Default::default()
    };
    assert!(trajectories::prepare(editor.document(), 0, 1, None, "Invalid", bad, linear).is_err());
    assert_eq!(editor.document().as_json(), &before);
    assert_eq!(revision, editor.revision());
    trajectories::prepare(
        editor.document(),
        0,
        1,
        None,
        "Path",
        Spec::default(),
        linear,
    )
    .unwrap()
    .0
    .apply(&mut editor)
    .unwrap();
    let saved = trajectories::recipes(editor.document(), 0)
        .unwrap()
        .pop()
        .unwrap();
    let revision = editor.revision();
    trajectories::prepare(
        editor.document(),
        0,
        1,
        Some(&saved.id),
        &saved.name,
        saved.spec,
        linear,
    )
    .unwrap()
    .0
    .apply(&mut editor)
    .unwrap();
    assert_eq!(revision, editor.revision());
    trajectories::detach(editor.document(), 0, &saved.id, true)
        .unwrap()
        .apply(&mut editor)
        .unwrap();
    assert!(
        editor
            .document()
            .event_values(0, 1, EventType::MoveX)
            .unwrap()
            .is_empty()
    );
    editor.undo();
    editor.undo();
    assert_eq!(*editor.document().as_json(), before);
}

#[test]
fn recipe_editing_preserves_unknown_recipe_parameters_and_unrelated_future_recipes() {
    let mut editor = Editor::new(ChartDocument::empty("Extensions", 120.).unwrap());
    trajectories::prepare(
        editor.document(),
        0,
        1,
        None,
        "Known",
        Spec::default(),
        linear,
    )
    .unwrap()
    .0
    .apply(&mut editor)
    .unwrap();
    let mut root = editor.document().as_json().clone();
    let list = root["judgeLineList"][0][trajectories::RECIPE_FIELD]
        .as_array_mut()
        .unwrap();
    list[0]["futureRecipe"] = serde_json::json!({"keep":null});
    list[0]["spec"]["futureParameter"] = serde_json::json!([8, 2]);
    let foreign = serde_json::json!({"id":"foreign-v2","name":"Future recipe","version":2,"spec":{"shape":"UnrecognizedFutureShape"},"future":[3,1]});
    list.push(foreign.clone());
    let id = list[0]["id"].as_str().unwrap().to_owned();
    editor = Editor::new(ChartDocument::from_value(root.clone()).unwrap());
    let spec = Spec {
        radius: [400., 200.],
        ..Default::default()
    };
    trajectories::prepare(editor.document(), 0, 1, Some(&id), "Changed", spec, linear)
        .unwrap()
        .0
        .apply(&mut editor)
        .unwrap();
    let list = &editor.document().as_json()["judgeLineList"][0][trajectories::RECIPE_FIELD];
    assert_eq!(list[0]["futureRecipe"], serde_json::json!({"keep":null}));
    assert_eq!(
        list[0]["spec"]["futureParameter"],
        serde_json::json!([8, 2])
    );
    assert_eq!(list[1], foreign);
    trajectories::detach(editor.document(), 0, &id, false)
        .unwrap()
        .apply(&mut editor)
        .unwrap();
    assert_eq!(
        editor.document().as_json()["judgeLineList"][0][trajectories::RECIPE_FIELD],
        serde_json::json!([foreign])
    );
    editor.undo();
    editor.undo();
    assert_eq!(*editor.document().as_json(), root);
}
#[test]
fn cartesian_and_polar_formulas_keep_independent_values_and_legacy_polar_recipes_work() {
    let spec = Spec {
        shape: Shape::Polar,
        expressions: ["50".into(), "pi/2".into()],
        ..Default::default()
    };
    let p = spec.compile().unwrap().point(0.5, &linear).unwrap();
    assert!(p[0].abs() < 1e-8);
    assert!((p[1] - 50.).abs() < 1e-8);
    let spec = Spec {
        polar_expressions: Some(["20".into(), "0".into()]),
        ..spec
    };
    assert_eq!(
        spec.compile().unwrap().point(0.5, &linear).unwrap(),
        [20., 0.]
    );
    let spec = Spec {
        shape: Shape::Formula,
        ..spec
    };
    let p = spec.compile().unwrap().point(0.5, &linear).unwrap();
    assert_eq!(p, [50., std::f64::consts::FRAC_PI_2]);
}
