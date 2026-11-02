use phistudio_core::{
    Beat, ChartDocument, Command, Editor, ProjectOptions,
    noise_fields::{self as noise, Action, Format, Track},
};
use serde_json::{Value, json};

#[test]
fn next_geometry_matches_upstream_samples_across_aspects_and_reverse_seeking() {
    let chart: Value =
        serde_json::from_str(include_str!("fixtures/noise-fields/chart.json")).unwrap();
    let samples: Value =
        serde_json::from_str(include_str!("fixtures/noise-fields/samples.json")).unwrap();
    let document = ChartDocument::from_value(chart).unwrap();
    let areas: Vec<_> = noise::fields(&document)
        .unwrap()
        .iter()
        .map(|v| noise::NextArea::new(v, document.tempo()).unwrap())
        .collect();
    for aspect in [1., 1.5, 16. / 9., 9. / 16., 4. / 3., 21. / 9.] {
        for expected in samples["samples"].as_array().unwrap().iter().rev() {
            let id = expected["id"].as_u64().unwrap() as usize;
            let time = expected["time"].as_f64().unwrap();
            let sample = areas[id].sample(time, aspect);
            let state = expected["state"].as_str().unwrap();
            if state.starts_with("hidden") {
                assert!(sample.is_none(), "id={id} t={time} {state}");
                continue;
            }
            let actual = sample.unwrap_or_else(|| panic!("id={id} t={time} {state}"));
            assert_eq!(
                actual.phase,
                match state {
                    "active" => 2,
                    "ready" => 1,
                    _ => 0,
                },
                "id={id} t={time}"
            );
            let convert = |x: f64, y: f64| [x * 10. * aspect as f64 / 1350., y * 10. / 900.];
            let target = convert(
                expected["center"]["x"].as_f64().unwrap(),
                expected["center"]["y"].as_f64().unwrap(),
            );
            let error = (actual.pose.center.x as f64 - target[0])
                .hypot(actual.pose.center.y as f64 - target[1]);
            assert!(
                error < 0.0002,
                "center id={id} t={time} aspect={aspect} error={error} actual={:?} target={target:?}",
                actual.pose.center
            );
            for expected in expected["points"].as_array().unwrap() {
                let target = convert(
                    expected["x"].as_f64().unwrap(),
                    expected["y"].as_f64().unwrap(),
                );
                let error = actual
                    .corners()
                    .iter()
                    .map(|point| (point.x as f64 - target[0]).hypot(point.y as f64 - target[1]))
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    error < 0.0002,
                    "corner id={id} t={time} aspect={aspect} error={error}"
                );
            }
        }
    }
}
fn beat(v: i32) -> Beat {
    Beat::new(v, 0, 1).unwrap()
}
fn editor() -> Editor {
    Editor::new(ChartDocument::empty("Noise authoring", 120.).unwrap())
}
fn create(doc: &ChartDocument, format: Format) -> Value {
    noise::create(beat(2), beat(10), format, false, doc.tempo())
}
fn insert(editor: &mut Editor, value: Value) {
    editor
        .execute(Command::EditNoise(Action::Insert { index: 0, value }))
        .unwrap();
}
#[test]
fn missing_and_null_lists_restore_exactly_with_shared_dirty_state() {
    for shape in [None, Some(Value::Null), Some(json!([]))] {
        let mut raw = editor().document().as_json().clone();
        if let Some(v) = shape.clone() {
            raw[noise::LIST] = v;
        }
        let mut editor = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
        let field = create(editor.document(), Format::Next);
        insert(&mut editor, field);
        assert!(editor.is_dirty());
        assert_eq!(editor.document().as_json()["META"]["RPEVersion"], 220);
        editor.undo();
        assert_eq!(editor.document().as_json(), &raw);
        assert!(!editor.is_dirty());
        editor.redo();
        assert!(editor.is_dirty());
    }
}
#[test]
fn editing_a_key_keeps_unknown_properties_original_order_and_only_one_undo_step() {
    let mut e = editor();
    let mut raw = create(e.document(), Format::Native);
    raw["future"] = json!({"kind":9});
    raw["bottomLeftPercentage"]["future"] = json!([1, 2]);
    raw["moveEvents"] = json!([{"time":4.,"endPosition":{"x":0.6,"y":0.5,"future":"v"},"future":1},{"time":1.,"endPosition":{"x":0.1,"y":0.2},"future":2}]);
    insert(&mut e, raw.clone());
    e.mark_saved();
    let mut key = raw["moveEvents"][1].clone();
    key["endPosition"]["y"] = json!(0.3);
    e.execute(Command::EditNoise(Action::Key {
        field: 0,
        track: Track::Move,
        index: Some(1),
        value: Some(key.clone()),
    }))
    .unwrap();
    let row = &e.document().as_json()[noise::LIST][0];
    assert_eq!(row["future"], raw["future"]);
    assert_eq!(row["moveEvents"][0], raw["moveEvents"][0]);
    assert_eq!(row["moveEvents"][1], key);
    e.undo();
    assert_eq!(noise::fields(e.document()).unwrap(), &[raw]);
    assert!(!e.is_dirty());
}
#[test]
fn native_seconds_and_next_beats_shift_in_their_own_clocks_across_bpm_changes() {
    let mut raw = editor().document().as_json().clone();
    raw["BPMList"] = json!([{"startTime":[0,0,1],"bpm":120.},{"startTime":[4,0,1],"bpm":60.}]);
    let doc = ChartDocument::from_value(raw).unwrap();
    for format in [Format::Native, Format::Next] {
        let original = create(&doc, format);
        let shifted = noise::shifted(&original, beat(2), beat(6), doc.tempo()).unwrap();
        assert_eq!(
            noise::beat(&shifted, "appearTime", format, doc.tempo()).unwrap(),
            beat(6)
        );
        if format == Format::Native {
            let delta = shifted["disappearTime"].as_f64().unwrap()
                - original["disappearTime"].as_f64().unwrap();
            assert_eq!(delta, 3.);
        } else {
            assert_eq!(
                noise::beat(&shifted, "disappearTime", format, doc.tempo()).unwrap(),
                beat(14)
            );
        }
    }
}
#[test]
fn invalid_batch_is_atomic_and_a_deleted_key_can_be_restored_with_its_container() {
    let mut e = editor();
    let field = create(e.document(), Format::Next);
    insert(&mut e, field);
    let before = e.document().as_json().clone();
    let revision = e.revision();
    let valid = noise::new_key(
        &before[noise::LIST][0],
        Track::MoveX,
        beat(2),
        beat(4),
        e.document().tempo(),
    )
    .unwrap();
    let mut invalid = valid.clone();
    invalid["endTime"] = json!([1, 0, 1]);
    assert!(
        e.execute_batch(
            "Noise key batch",
            vec![
                Command::EditNoise(Action::Key {
                    field: 0,
                    track: Track::MoveX,
                    index: None,
                    value: Some(valid)
                }),
                Command::EditNoise(Action::Key {
                    field: 0,
                    track: Track::MoveX,
                    index: None,
                    value: Some(invalid)
                })
            ]
        )
        .is_err()
    );
    assert_eq!(e.document().as_json(), &before);
    assert_eq!(e.revision(), revision);
}
#[test]
fn native_point_keys_and_rpe_intervals_validate_their_actual_format() {
    let e = editor();
    let native = create(e.document(), Format::Native);
    let next = create(e.document(), Format::Next);
    assert!(noise::validate(&native).is_ok());
    assert!(noise::validate(&next).is_ok());
    let mut value = native.clone();
    value["bottomLeft"] = json!({"x":0.,"y":0.});
    assert!(noise::validate(&value).is_err());
    let mut value = native;
    value["phiStudioColor"] = json!([256, 0, 0]);
    assert!(noise::validate(&value).is_err());
    let mut value = next;
    value["moveXEvents"] = json!([{"startTime":[1,0,1],"endTime":[3,0,1],"start":0.,"end":1.},{"startTime":[2,0,1],"endTime":[4,0,1],"start":1.,"end":2.}]);
    assert!(noise::validate(&value).is_err());
}
#[test]
fn phira_pro_export_packages_guide_lines_and_reimports_exact_authored_fields() {
    let mut e = editor();
    let mut field = create(e.document(), Format::Native);
    field[noise::COLOR] = json!([10, 220, 80]);
    field["unknown"] = json!("preserved");
    insert(&mut e, field);
    let original = e.document().as_json().clone();
    let export = phistudio_core::prepare_project_export(
        e.document(),
        &ProjectOptions::default(),
        None,
        None,
    )
    .unwrap();
    let file = std::fs::File::open(export.path()).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let chart: Value = serde_json::from_reader(archive.by_name("chart.rpe.json").unwrap()).unwrap();
    assert!(chart[noise::LIST].as_array().unwrap().is_empty());
    assert_eq!(chart["judgeLineList"].as_array().unwrap().len(), 2);
    let line = &chart["judgeLineList"][1];
    assert_eq!(line["Texture"], "phistudio-noise-fields/isSubtract0.png");
    assert_eq!(
        line["extended"]["colorEvents"][0]["start"],
        json!([10, 220, 80])
    );
    assert!(
        archive
            .by_name("phistudio-noise-fields/isSubtract0.png")
            .is_ok()
    );
    assert!(
        archive
            .by_name("phistudio-noise-fields/isSubtract1.png")
            .is_ok()
    );
    assert_eq!(
        ChartDocument::from_value(chart).unwrap().as_json(),
        &original
    );
    assert_eq!(e.document().as_json(), &original);
}
#[test]
fn uncolored_exports_keep_native_fields_and_external_marker_edits_are_not_overwritten() {
    let mut e = editor();
    let field = create(e.document(), Format::Native);
    insert(&mut e, field);
    let (plain, assets) = noise::export::prepare(e.document(), &[]).unwrap();
    assert_eq!(&plain, e.document().as_json());
    assert!(assets.is_empty());
    let mut field = noise::fields(e.document()).unwrap()[0].clone();
    field[noise::COLOR] = json!([255, 255, 255]);
    e.execute(Command::EditNoise(Action::Update {
        index: 0,
        value: field,
    }))
    .unwrap();
    let (mut exported, _) = noise::export::prepare(e.document(), &[]).unwrap();
    exported["judgeLineList"][1]["extended"]["colorEvents"][0]["start"] = json!([0, 255, 0]);
    let imported = ChartDocument::from_value(exported.clone()).unwrap();
    assert_eq!(imported.as_json(), &exported);
}
#[test]
fn color_animations_do_not_change_geometry_or_blocking_phases() {
    let e = editor();
    let mut raw = create(e.document(), Format::Next);
    let base = noise::Field::new(&raw, e.document().tempo()).unwrap();
    let mut key =
        noise::new_key(&raw, Track::Color, beat(2), beat(4), e.document().tempo()).unwrap();
    key["start"] = json!([255, 0, 0]);
    key["end"] = json!([0, 0, 255]);
    raw[noise::COLOR_EVENTS] = json!([key]);
    let colored = noise::Field::new(&raw, e.document().tempo()).unwrap();
    let a = base.sample(1.5, 1.5).unwrap();
    let b = colored.sample(1.5, 1.5).unwrap();
    assert_eq!(b.color, [128, 0, 128]);
    assert_eq!(a.phase, b.phase);
    assert_eq!(a.corners(), b.corners());
}

#[test]
fn inserted_keys_sample_existing_values_and_group_shifts_keep_native_seconds() {
    let doc = editor().document().clone();
    let mut field = create(&doc, Format::Native);
    field["moveEvents"] = json!([
        {"time":1.,"endPosition":{"x":0.2,"y":0.3},"easeTypeX":1,"easeTypeY":2},
        {"time":3.,"endPosition":{"x":0.8,"y":0.7}}
    ]);
    let key = noise::new_key(&field, Track::Move, beat(4), beat(5), doc.tempo()).unwrap();
    assert!((key["endPosition"]["x"].as_f64().unwrap() - 0.35).abs() < 1e-6);
    assert!((key["endPosition"]["y"].as_f64().unwrap() - 0.6).abs() < 1e-6);
    let mut with_key = field.clone();
    with_key["moveEvents"].as_array_mut().unwrap().push(key);
    let a = noise::Field::new(&field, doc.tempo())
        .unwrap()
        .sample(2., 1.5)
        .unwrap();
    let b = noise::Field::new(&with_key, doc.tempo())
        .unwrap()
        .sample(2., 1.5)
        .unwrap();
    assert!((a.pose.center - b.pose.center).length() < 1e-5);
    let mut next = create(&doc, Format::Next);
    next["Name"] = json!("Next name");
    assert_eq!(noise::name(&next), "Next name");
    next["moveXEvents"] =
        json!([{"startTime":[2,0,1],"endTime":[6,0,1],"start":100.,"end":200.,"easingType":1}]);
    let key = noise::new_key(&next, Track::MoveX, beat(8), beat(9), doc.tempo()).unwrap();
    assert_eq!(key["start"], 200.);
    assert_eq!(key["end"], 200.);
    assert_eq!(
        noise::shifted(&field, beat(3), beat(3), doc.tempo()).unwrap(),
        field
    );
}

#[test]
fn colored_export_retains_negative_lifetimes_and_bakes_rgb_curves_for_phira_pro() {
    let mut ed = editor();
    let mut field = noise::create(
        beat(-8),
        beat(-2),
        Format::Native,
        false,
        ed.document().tempo(),
    );
    field[noise::COLOR] = json!([0, 0, 255]);
    field[noise::COLOR_EVENTS] = json!([{"startTime":[-8,0,1],"endTime":[-2,0,1],"start":[0,0,255],"end":[255,0,0],"easingType":29}]);
    insert(&mut ed, field.clone());
    let (export, _) = noise::export::prepare(ed.document(), &[]).unwrap();
    let events = export["judgeLineList"][1]["extended"]["colorEvents"]
        .as_array()
        .unwrap();
    assert!(events.len() > 2);
    assert!(events.iter().all(|e| e["easingType"] == 1));
    // Phira Pro's RPEExtendedEvents deserializes RGBColor(u8, u8, u8),
    // including for linear color tracks. JSON floating values are invalid.
    #[derive(serde::Deserialize)]
    struct ProColorEvent {
        start: [u8; 3],
        end: [u8; 3],
    }
    for event in events {
        let parsed: ProColorEvent = serde_json::from_value(event.clone()).unwrap();
        assert_eq!(json!(parsed.start), event["start"]);
        assert_eq!(json!(parsed.end), event["end"]);
    }

    let sampler = noise::Field::new(&field, ed.document().tempo()).unwrap();
    for step in 0..290 {
        let time = -4. + step as f64 / 100.;
        let event = events
            .iter()
            .rev()
            .find(|e| {
                noise::time(e, "startTime", Format::Next, ed.document().tempo()).unwrap() <= time
            })
            .unwrap();
        let a = noise::time(event, "startTime", Format::Next, ed.document().tempo()).unwrap();
        let b = noise::time(event, "endTime", Format::Next, ed.document().tempo()).unwrap();
        let progress = ((time - a) / (b - a)).clamp(0., 1.);
        for (channel, rgb) in sampler.color_at(time).iter().enumerate() {
            let x = event["start"][channel].as_f64().unwrap();
            let y = event["end"][channel].as_f64().unwrap();
            assert!(
                (x + (y - x) * progress - *rgb as f64).abs() < 1.,
                "{time} channel={channel}"
            );
        }
    }
    assert_eq!(
        ChartDocument::from_value(export).unwrap().as_json(),
        ed.document().as_json()
    );
    let mut next = create(ed.document(), Format::Next);
    next[noise::COLOR] = json!([0, 255, 0]);
    next["activeIntervals"] = json!([{"startTime":[4,0,1],"endTime":[8,0,1],"readyFlash":0}]);
    let mut raw = ed.document().as_json().clone();
    raw[noise::LIST] = json!([next]);
    let bad = ChartDocument::from_value(raw).unwrap();
    assert!(
        noise::export::prepare(&bad, &[])
            .unwrap_err()
            .to_string()
            .contains("Ready duration")
    );
}

#[test]
fn next_version_upgrade_undo_distinguishes_missing_and_explicit_null() {
    for before in [None, Some(Value::Null), Some(json!(123))] {
        let mut raw = editor().document().as_json().clone();
        if let Some(version) = before {
            raw["META"]["RPEVersion"] = version;
        } else {
            raw["META"]
                .as_object_mut()
                .unwrap()
                .shift_remove("RPEVersion");
        }
        let mut ed = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
        let value = create(ed.document(), Format::Next);
        insert(&mut ed, value);
        assert_eq!(ed.document().as_json()["META"]["RPEVersion"], 220);
        ed.undo();
        assert_eq!(ed.document().as_json(), &raw);
    }
}

#[test]
fn lifetime_resize_keeps_tied_phases_together_and_internal_phases_inside() {
    let document = ChartDocument::empty("Resize phases", 120.).unwrap();
    let tempo = document.tempo();
    let mut field = noise::create(
        Beat::ZERO,
        Beat::new(4, 0, 1).unwrap(),
        Format::Native,
        false,
        tempo,
    );
    field["future"] = json!({"keep":true});
    field["enableTime"] = json!(0.);
    let tick = Beat::new(0, 1, 4).unwrap();
    let bigger = noise::resize_field(&field, 3, Beat::new(6, 0, 1).unwrap(), tick, tempo).unwrap();
    assert_eq!(bigger["disappearTime"], json!(3.));
    assert_eq!(bigger["disableTime"], bigger["disappearTime"]);
    let shorter = noise::resize_field(&field, 0, Beat::ONE, tick, tempo).unwrap();
    assert_eq!(shorter["appearTime"], json!(0.5));
    assert_eq!(shorter["enableTime"], shorter["appearTime"]);
    let clamped =
        noise::resize_field(&field, 2, Beat::new(99, 0, 1).unwrap(), tick, tempo).unwrap();
    assert_eq!(clamped["disableTime"], clamped["disappearTime"]);
    assert_eq!(clamped["future"], field["future"]);
    assert_eq!(field["disappearTime"], json!(2.));
}
