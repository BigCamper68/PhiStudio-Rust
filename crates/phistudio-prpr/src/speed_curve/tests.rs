use super::*;
use phistudio_core::{Command, Editor, Note, NoteId};
fn document() -> ChartDocument {
    let mut raw = ChartDocument::empty("Speed curve", 120.)
        .unwrap()
        .as_json()
        .clone();
    raw["judgeLineList"][0]["eventLayers"][0]["speedEvents"] = json!([{"startTime":[0,0,1],"endTime":[8,0,1],"start":10,"end":10,"easingType":1,"custom":{"preserve":true}}]);
    raw["judgeLineList"][0]["notes"] = json!([Note {
        start_time: Beat::new(4, 0, 1).unwrap(),
        end_time: Beat::new(4, 0, 1).unwrap(),
        ..Default::default()
    }]);
    raw["judgeLineList"][0]["notes"][0]["customNote"] = json!("untouched");
    ChartDocument::from_value(raw).unwrap()
}
fn range() -> [Beat; 2] {
    [Beat::ZERO, Beat::new(8, 0, 1).unwrap()]
}
fn source(doc: &ChartDocument) -> SpeedCurveSource {
    SpeedCurveSource::capture(doc, NoteId { line: 0, index: 0 }).unwrap()
}
#[test]
fn the_graph_samples_upstream_height_in_both_project_speed_modes() {
    for modern in [false, true] {
        let source = source(&document());
        let graph = pollster::block_on(speed_curve_profile(&source, 0, range(), modern)).unwrap();
        let ratio = 100. / prpr::core::HEIGHT_RATIO;
        for sample in &graph.samples {
            assert!(
                (sample[1] - (4. - sample[0]) * 0.5 * 10. * ratio).abs() < 0.01,
                "{sample:?}"
            );
        }
        assert_eq!(graph.judgment_distance, 0.);
    }
}
#[test]
fn moving_a_distance_point_changes_only_one_speed_layer_and_has_exact_undo_redo() {
    for modern in [false, true] {
        let original = document();
        let before = original.as_json().clone();
        let source = source(&original);
        let graph = pollster::block_on(speed_curve_profile(&source, 0, range(), modern)).unwrap();
        let beat = Beat::new(2, 0, 1).unwrap();
        let value = graph.samples.iter().find(|p| p[0] == 2.).unwrap()[1] + 120.;
        let mut points = graph.points.clone();
        points.push(SpeedCurvePoint {
            beat,
            source_beat: beat,
            distance: value,
            easing: None,
            fixed: false,
        });
        let edit = pollster::block_on(fit_speed_curve(&source, 0, &graph, points, modern)).unwrap();
        let mut editor = Editor::new(original);
        assert!(editor.execute(Command::EditSpeedCurve(edit)).unwrap());
        assert_eq!(
            editor.document().as_json()["judgeLineList"][0]["notes"],
            before["judgeLineList"][0]["notes"]
        );
        let after = editor.document().as_json().clone();
        let graph = pollster::block_on(speed_curve_profile(
            &source_of(editor.document()),
            0,
            range(),
            modern,
        ))
        .unwrap();
        assert!((graph.samples.iter().find(|p| p[0] == 2.).unwrap()[1] - value).abs() < 1.);
        assert!(editor.undo());
        assert_eq!(editor.document().as_json(), &before);
        assert!(editor.redo());
        assert_eq!(editor.document().as_json(), &after);
    }
}
fn source_of(doc: &ChartDocument) -> SpeedCurveSource {
    source(doc)
}
#[test]
fn adding_a_point_on_the_curve_retains_original_events_and_recipe_reopens() {
    let original = document();
    let source = source(&original);
    let graph = pollster::block_on(speed_curve_profile(&source, 0, range(), true)).unwrap();
    let beat = Beat::new(2, 0, 1).unwrap();
    let mut points = graph.points.clone();
    points.push(SpeedCurvePoint {
        beat,
        source_beat: beat,
        distance: graph.samples[128][1],
        easing: None,
        fixed: false,
    });
    let edit = pollster::block_on(fit_speed_curve(&source, 0, &graph, points, true)).unwrap();
    assert_eq!(edit.events, source.layers[0]);
    let mut editor = Editor::new(original);
    editor.execute(Command::EditSpeedCurve(edit)).unwrap();
    let saved =
        ChartDocument::parse(&serde_json::to_string(editor.document().as_json()).unwrap()).unwrap();
    let reopened =
        pollster::block_on(speed_curve_profile(&source_of(&saved), 0, range(), true)).unwrap();
    assert!(reopened.points.iter().any(|p| p.beat == beat));
}
#[test]
fn stale_sources_and_noninvertible_distances_cannot_modify_a_chart() {
    let original = document();
    let source = source(&original);
    let graph = pollster::block_on(speed_curve_profile(&source, 0, range(), true)).unwrap();
    let edit = pollster::block_on(fit_speed_curve(
        &source,
        0,
        &graph,
        graph.points.clone(),
        true,
    ))
    .unwrap();
    let mut editor = Editor::new(original);
    let mut note = source.note.clone();
    note.speed = 2.;
    editor
        .execute(Command::UpdateNote {
            id: source.id,
            note,
        })
        .unwrap();
    let before = editor.document().as_json().clone();
    assert!(editor.execute(Command::EditSpeedCurve(edit)).is_err());
    assert_eq!(editor.document().as_json(), &before);
    let mut zero = source;
    zero.note.speed = 0.;
    let graph = pollster::block_on(speed_curve_profile(&zero, 0, range(), true)).unwrap();
    let mut points = graph.points.clone();
    points.push(SpeedCurvePoint {
        beat: Beat::ONE,
        source_beat: Beat::ONE,
        distance: 10.,
        easing: None,
        fixed: false,
    });
    assert!(pollster::block_on(fit_speed_curve(&zero, 0, &graph, points, true)).is_err());
}
#[test]
fn every_distance_easing_is_baked_to_native_linear_speed_intervals() {
    let original = document();
    let source = source(&original);
    for modern in [false, true] {
        let graph = pollster::block_on(speed_curve_profile(&source, 0, range(), modern)).unwrap();
        for easing in 1..=29 {
            let mut points = graph.points.clone();
            points[0].easing = Some(easing);
            let edit = pollster::block_on(fit_speed_curve(&source, 0, &graph, points, modern))
                .unwrap_or_else(|e| panic!("Easing {easing}, modern={modern}: {e:#}"));
            assert!(edit.events.len() <= 1026);
            assert!(
                edit.events
                    .iter()
                    .all(|v| v["easingType"] == 1 && v["bezier"] == 0)
            );
            assert!(edit.events.iter().any(|v| v["custom"]["preserve"] == true));
        }
    }
}
#[test]
fn bpm_changes_other_layers_negative_speed_and_y_control_remain_native() {
    for negative in [false, true] {
        let mut raw = document().as_json().clone();
        raw["BPMList"] = json!([{"startTime":[0,0,1],"bpm":120.},{"startTime":[2,0,1],"bpm":180.}]);
        raw["judgeLineList"][0]["eventLayers"][0]["speedEvents"][0]["start"] =
            json!(if negative { -10. } else { 10. });
        raw["judgeLineList"][0]["eventLayers"][0]["speedEvents"][0]["end"] =
            json!(if negative { -10. } else { 10. });
        raw["judgeLineList"][0]["eventLayers"].as_array_mut().unwrap().push(json!({"speedEvents":[{"startTime":[0,0,1],"endTime":[8,0,1],"start":2.,"end":2.,"easingType":1}]}));
        raw["judgeLineList"][0]["notes"][0]["above"] = json!(0);
        raw["judgeLineList"][0]["notes"][0]["speed"] = json!(0.7);
        raw["judgeLineList"][0]["notes"][0]["yOffset"] = json!(40.);
        raw["judgeLineList"][0]["yControl"] =
            json!([{"x":-5000.,"y":1.,"easing":2},{"x":5000.,"y":1.4,"easing":1}]);
        let doc = ChartDocument::from_value(raw.clone()).unwrap();
        let source = source(&doc);
        let graph = pollster::block_on(speed_curve_profile(&source, 0, range(), true)).unwrap();
        let beat = Beat::new(3, 0, 1).unwrap();
        let mut native = pollster::block_on(NativeModel::new(&source, true, range()[1])).unwrap();
        let mut points = graph.points.clone();
        points.push(SpeedCurvePoint {
            beat,
            source_beat: beat,
            distance: native.distance(3.) + 80.,
            easing: None,
            fixed: false,
        });
        let edit = pollster::block_on(fit_speed_curve(&source, 0, &graph, points, true)).unwrap();
        let mut editor = Editor::new(doc);
        editor.execute(Command::EditSpeedCurve(edit)).unwrap();
        assert_eq!(
            editor.document().as_json()["judgeLineList"][0]["eventLayers"][1],
            raw["judgeLineList"][0]["eventLayers"][1]
        );
        assert_eq!(
            editor.document().as_json()["judgeLineList"][0]["yControl"],
            raw["judgeLineList"][0]["yControl"]
        );
    }
}
#[test]
fn empty_layer_creation_restores_its_original_shape_on_undo() {
    let doc = document();
    let before = doc.as_json().clone();
    let source = source(&doc);
    let graph = pollster::block_on(speed_curve_profile(&source, 3, range(), true)).unwrap();
    let beat = Beat::new(2, 0, 1).unwrap();
    let mut points = graph.points.clone();
    points.push(SpeedCurvePoint {
        beat,
        source_beat: beat,
        distance: graph.samples[128][1] + 60.,
        easing: None,
        fixed: false,
    });
    let edit = pollster::block_on(fit_speed_curve(&source, 3, &graph, points, true)).unwrap();
    let mut editor = Editor::new(doc);
    editor.execute(Command::EditSpeedCurve(edit)).unwrap();
    assert!(editor.undo());
    assert_eq!(editor.document().as_json(), &before);
}
#[test]
fn global_native_clock_matches_the_real_parser_with_long_speed_sentinels() {
    for modern in [false, true] {
        let mut raw = document().as_json().clone();
        raw["judgeLineList"][0]["eventLayers"][0]["speedEvents"][0]["endTime"] =
            json!([999999, 0, 1]);
        let document = ChartDocument::from_value(raw.clone()).unwrap();
        let mut source = source(&document);
        source.native_horizon = Some(clock::native_horizon(&document));
        let mut fs = crate::assets::ChartAssets::new(None).unwrap();
        let mut actual = pollster::block_on(prpr::parse::parse_rpe(
            &serde_json::to_string(&raw).unwrap(),
            &mut fs,
            ChartExtra::default(),
            modern,
        ))
        .unwrap();
        actual.lines[0].height.set_time(2.);
        let hit = f64::from(actual.lines[0].height.now()) * 450.;
        let profile = pollster::block_on(speed_curve_profile(
            &source,
            0,
            [Beat::ZERO, Beat::new(5, 0, 1).unwrap()],
            modern,
        ))
        .unwrap();
        for point in &profile.samples {
            actual.lines[0].height.set_time(point[0] * 0.5);
            let distance = hit - f64::from(actual.lines[0].height.now()) * 450.;
            assert!(
                (point[1] - distance).abs() < 1.,
                "{modern}: {point:?}, {distance}"
            );
        }
    }
}
#[test]
fn disjoint_curve_regions_keep_their_logical_points_and_easing_hints() {
    let doc = document();
    let source = source(&doc);
    let graph = pollster::block_on(speed_curve_profile(&source, 0, range(), true)).unwrap();
    let mut points = graph.points.clone();
    points[0].easing = Some(8);
    let edit = pollster::block_on(fit_speed_curve(&source, 0, &graph, points, true)).unwrap();
    let mut editor = Editor::new(doc);
    editor.execute(Command::EditSpeedCurve(edit)).unwrap();
    let next = source_of(editor.document());
    let other_range = [Beat::new(10, 0, 1).unwrap(), Beat::new(14, 0, 1).unwrap()];
    let graph = pollster::block_on(speed_curve_profile(&next, 0, other_range, true)).unwrap();
    let beat = Beat::new(12, 0, 1).unwrap();
    let mut points = graph.points.clone();
    points.push(SpeedCurvePoint {
        beat,
        source_beat: beat,
        distance: graph.samples[256][1] + 60.,
        easing: None,
        fixed: false,
    });
    let edit = pollster::block_on(fit_speed_curve(&next, 0, &graph, points, true)).unwrap();
    editor.execute(Command::EditSpeedCurve(edit)).unwrap();
    let source = source_of(editor.document());
    assert_eq!(source.saved_recipes(0).len(), 2);
    let graph = pollster::block_on(speed_curve_profile(&source, 0, range(), true)).unwrap();
    assert_eq!(graph.points[0].easing, Some(8));
    assert_eq!(
        graph.points.len(),
        3,
        "Baked fragments must stay hidden after editing a different region"
    );
}

#[test]
fn hold_heads_stay_pinned_while_the_approach_curve_can_change() {
    for modern in [false, true] {
        let mut raw = document().as_json().clone();
        raw["judgeLineList"][0]["notes"][0]["type"] = json!(2);
        raw["judgeLineList"][0]["notes"][0]["endTime"] = json!([7, 0, 1]);
        raw["judgeLineList"][0]["notes"][0]["yOffset"] = json!(35.);
        let doc = ChartDocument::from_value(raw).unwrap();
        let source = source(&doc);
        let graph = pollster::block_on(speed_curve_profile(&source, 0, range(), modern)).unwrap();
        assert!(
            graph
                .samples
                .iter()
                .filter(|p| p[0] >= 4.)
                .all(|p| (p[1] - 35.).abs() < 0.001)
        );
        assert!(
            graph
                .points
                .iter()
                .filter(|p| p.beat >= source.note.start_time)
                .all(|p| p.fixed)
        );
        let mut points = graph.points.clone();
        points.push(SpeedCurvePoint {
            beat: Beat::new(2, 0, 1).unwrap(),
            source_beat: Beat::new(2, 0, 1).unwrap(),
            distance: graph.samples[128][1] + 100.,
            easing: Some(7),
            fixed: false,
        });
        let edit = pollster::block_on(fit_speed_curve(&source, 0, &graph, points, modern)).unwrap();
        let mut candidate = source.clone();
        candidate.layers[0] = edit.events;
        let updated =
            pollster::block_on(speed_curve_profile(&candidate, 0, range(), modern)).unwrap();
        assert!((updated.samples[128][1] - graph.samples[128][1] - 100.).abs() < 1.);
        assert!(
            updated
                .samples
                .iter()
                .filter(|p| p[0] >= 4.)
                .all(|p| (p[1] - 35.).abs() < 0.001)
        );
    }
}

#[test]
fn splitting_a_linear_speed_event_preserves_native_motion_outside_the_work_range() {
    for modern in [false, true] {
        let mut raw = document().as_json().clone();
        let line = &mut raw["judgeLineList"][0];
        line["eventLayers"][0]["speedEvents"][0]["start"] = json!(5.);
        line["eventLayers"][0]["speedEvents"][0]["end"] = json!(15.);
        line["eventLayers"][0]["speedEvents"][0]["endTime"] = json!([12, 0, 1]);
        line["eventLayers"][0]["alphaEvents"][0]["endTime"] = json!([16, 0, 1]);
        let doc = ChartDocument::from_value(raw).unwrap();
        let mut source = source(&doc);
        source.native_horizon = Some(clock::native_horizon(&doc));
        let range = [Beat::new(2, 0, 1).unwrap(), Beat::new(6, 0, 1).unwrap()];
        let graph = pollster::block_on(speed_curve_profile(&source, 0, range, modern)).unwrap();
        let mut points = graph.points.clone();
        points.push(SpeedCurvePoint {
            beat: Beat::new(3, 0, 1).unwrap(),
            source_beat: Beat::new(3, 0, 1).unwrap(),
            distance: graph.samples[128][1] + 75.,
            easing: None,
            fixed: false,
        });
        let edit = pollster::block_on(fit_speed_curve(&source, 0, &graph, points, modern)).unwrap();
        let mut candidate = source.clone();
        candidate.layers[0] = edit.events;
        let mut before = pollster::block_on(NativeModel::new(&source, modern, range[1])).unwrap();
        let mut after = pollster::block_on(NativeModel::new(&candidate, modern, range[1])).unwrap();
        for beat in [0., 1., 2., 6., 7., 10., 12., 15.] {
            assert!(
                (before.distance(beat) - after.distance(beat)).abs() < 1.,
                "{modern}: {beat}"
            );
        }
    }
}

#[test]
fn an_imported_nonlinear_event_cannot_be_cut_by_the_work_range() {
    let mut raw = document().as_json().clone();
    raw["judgeLineList"][0]["eventLayers"][0]["speedEvents"][0]["end"] = json!(15.);
    raw["judgeLineList"][0]["eventLayers"][0]["speedEvents"][0]["easingType"] = json!(8);
    let doc = ChartDocument::from_value(raw).unwrap();
    let source = source(&doc);
    let range = [Beat::ONE, Beat::new(7, 0, 1).unwrap()];
    let graph = pollster::block_on(speed_curve_profile(&source, 0, range, true)).unwrap();
    let mut points = graph.points.clone();
    points[0].easing = Some(7);
    let error = pollster::block_on(fit_speed_curve(&source, 0, &graph, points, true)).unwrap_err();
    assert!(error.to_string().contains("nonlinear event"), "{error:#}");
    assert_eq!(
        doc.as_json()["judgeLineList"][0]["eventLayers"][0]["speedEvents"][0]["easingType"],
        8
    );
}

#[test]
fn distance_projection_matches_native_notes_with_y_control_and_y_offset() {
    for modern in [false, true] {
        for hold in [false, true] {
            let mut raw = document().as_json().clone();
            let line = &mut raw["judgeLineList"][0];
            line["notes"][0]["type"] = json!(if hold { 2 } else { 1 });
            line["notes"][0]["endTime"] = json!([7, 0, 1]);
            line["notes"][0]["above"] = json!(0);
            line["notes"][0]["speed"] = json!(0.7);
            line["notes"][0]["yOffset"] = json!(60.);
            line["yControl"] = json!([
                {"x":-5000.,"y":1.,"easing":2},
                {"x":5000.,"y":1.4,"easing":1}
            ]);
            let doc = ChartDocument::from_value(raw.clone()).unwrap();
            let mut source = source(&doc);
            source.native_horizon = Some(clock::native_horizon(&doc));
            let mut fs = crate::assets::ChartAssets::new(None).unwrap();
            let mut actual = pollster::block_on(prpr::parse::parse_rpe(
                &serde_json::to_string(&raw).unwrap(),
                &mut fs,
                ChartExtra::default(),
                modern,
            ))
            .unwrap();
            let profile =
                pollster::block_on(speed_curve_profile(&source, 0, range(), modern)).unwrap();
            let line = &mut actual.lines[0];
            let note = &line.notes[0];
            let offset = f64::from(note.object.translation.1.now()) * 450.;
            for [beat, distance] in &profile.samples {
                line.height.set_time(beat * 0.5);
                let floor = (note.height - f64::from(line.height.now())) * 450.;
                let mut control = line.ctrl_obj.borrow_mut();
                control.set_height(floor + offset / note.speed);
                let moving = if hold && *beat >= 4. { 0. } else { floor };
                let expected =
                    -(moving * note.speed * f64::from(control.y.now_opt().unwrap_or(1.)) + offset);
                assert!(
                    (distance - expected).abs() < 0.01,
                    "hold={hold}, modern={modern}, beat={beat}: {distance} != {expected}"
                );
            }
        }
    }
}
