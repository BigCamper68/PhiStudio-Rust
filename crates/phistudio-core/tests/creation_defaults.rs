use phistudio_core::{
    ChartDocument, Command, Editor, EditorPreferences, EventType, LineCreationDefaults, Note,
    NoteId,
    note_tracing::{Snapshot, Spec, TracePoint},
};
use serde_json::json;
fn defaults() -> LineCreationDefaults {
    LineCreationDefaults {
        x: 123.,
        y: -64.,
        rotation: 35.,
        alpha: 172.,
        speed: -12.,
    }
}
fn values(editor: &Editor, line: usize) -> Vec<f64> {
    [
        EventType::MoveX,
        EventType::MoveY,
        EventType::Rotate,
        EventType::Alpha,
        EventType::Speed,
    ]
    .into_iter()
    .map(|kind| {
        let events = editor.document().event_values(line, 0, kind).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["start"], events[0]["end"]);
        events[0]["start"].as_f64().unwrap()
    })
    .collect()
}
#[test]
fn single_and_batch_creation_use_preferences_and_undo_exactly() {
    for command in [
        Command::AddLine {
            name: "Custom".into(),
        },
        Command::AddLines { count: 4 },
    ] {
        let mut editor = Editor::new(ChartDocument::empty("Defaults", 120.).unwrap());
        let before = editor.document().as_json().clone();
        editor
            .execute_with_line_defaults(command, &defaults())
            .unwrap();
        for line in 1..editor.document().line_count() {
            assert_eq!(values(&editor, line), vec![123., -64., 35., 172., -12.]);
        }
        assert_eq!(
            editor.document().as_json()["judgeLineList"][0],
            before["judgeLineList"][0]
        );
        let after = editor.document().as_json().clone();
        editor.undo();
        assert_eq!(editor.document().as_json(), &before);
        editor.redo();
        assert_eq!(editor.document().as_json(), &after);
    }
}
#[test]
fn copies_and_explicit_chart_data_keep_their_original_values() {
    let mut raw = ChartDocument::empty("Copies", 120.)
        .unwrap()
        .as_json()
        .clone();
    raw["judgeLineList"][0]["unknown"] = json!({"keep":true});
    raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0]["start"] = json!(999.);
    let mut editor = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
    editor
        .execute_with_line_defaults(Command::DuplicateLine(0), &defaults())
        .unwrap();
    let source = &raw["judgeLineList"][0];
    let mut duplicate = editor.document().as_json()["judgeLineList"][1].clone();
    duplicate["Name"] = source["Name"].clone();
    assert_eq!(&duplicate, source);
    editor.undo();
    assert_eq!(editor.document().as_json(), &raw);
}
#[test]
fn stock_defaults_are_identical_and_invalid_settings_are_atomic() {
    let doc = ChartDocument::empty("Stock", 120.).unwrap();
    let mut stock = Editor::new(doc.clone());
    let mut custom = Editor::new(doc);
    stock.execute(Command::AddLines { count: 2 }).unwrap();
    custom
        .execute_with_line_defaults(
            Command::AddLines { count: 2 },
            &LineCreationDefaults::default(),
        )
        .unwrap();
    assert_eq!(stock.document().as_json(), custom.document().as_json());
    for invalid in [
        LineCreationDefaults {
            x: f64::NAN,
            ..defaults()
        },
        LineCreationDefaults {
            alpha: 256.,
            ..defaults()
        },
    ] {
        let before = custom.document().as_json().clone();
        let revision = custom.revision();
        assert!(
            custom
                .execute_with_line_defaults(Command::AddLines { count: 3 }, &invalid)
                .is_err()
        );
        assert_eq!(custom.document().as_json(), &before);
        assert_eq!(custom.revision(), revision);
    }
}
#[test]
fn preferences_migrate_persist_and_normalize_without_entering_chart_json() {
    let root = tempfile::tempdir().unwrap();
    let mut preferences: EditorPreferences =
        serde_json::from_value(json!({"version":1,"options":{}})).unwrap();
    assert_eq!(
        preferences.options.new_line_defaults,
        LineCreationDefaults::default()
    );
    preferences.options.new_line_defaults = defaults();
    phistudio_core::save_editor_preferences(root.path(), &preferences).unwrap();
    assert_eq!(
        phistudio_core::load_editor_preferences(root.path()).unwrap(),
        preferences.normalized()
    );
    let normalized = LineCreationDefaults {
        x: f64::INFINITY,
        y: -2e9,
        rotation: f64::NAN,
        alpha: 1000.,
        speed: f64::NAN,
    }
    .normalized();
    assert_eq!(
        normalized,
        LineCreationDefaults {
            x: 0.,
            y: -1e9,
            rotation: 0.,
            alpha: 255.,
            speed: 10.
        }
    );
}
#[test]
fn new_project_initial_line_uses_defaults_and_keeps_them_when_saved() {
    let root = tempfile::tempdir().unwrap();
    let doc = ChartDocument::empty_with_line_defaults("New project", 150., &defaults()).unwrap();
    assert_eq!(
        values(&Editor::new(doc.clone()), 0),
        vec![123., -64., 35., 172., -12.]
    );
    let audio = root.path().join("music.ogg");
    let illustration = root.path().join("cover.png");
    std::fs::write(&audio, b"fixture music").unwrap();
    std::fs::write(&illustration, b"fixture cover").unwrap();
    let location = phistudio_core::create_project_with_media(
        root.path(),
        &doc,
        &phistudio_core::NewProjectMedia {
            audio,
            audio_name: "music.ogg".into(),
            illustration,
            illustration_name: "cover.png".into(),
        },
    )
    .unwrap();
    let loaded = phistudio_core::load_chart(&location.chart).unwrap();
    assert_eq!(
        values(&Editor::new(loaded), 0),
        vec![123., -64., 35., 172., -12.]
    );
}
#[test]
fn tracing_new_lines_in_any_layer_subtracts_other_initial_layers() {
    for layer in [0, 3] {
        let mut raw = ChartDocument::empty("Trace defaults", 120.)
            .unwrap()
            .as_json()
            .clone();
        raw["judgeLineList"][0]["notes"] = json!([Note::default()]);
        let mut editor = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
        let spec = Spec {
            source: NoteId { line: 0, index: 0 },
            destination: None,
            layer,
            range: ["0".parse().unwrap(), "4".parse().unwrap()],
            sample_rate: 120,
            rotation: true,
            replace: false,
            position_tolerance: 0.15,
            rotation_tolerance: 0.015,
        };
        let points = [
            TracePoint {
                seconds: 0.,
                values: [200., 300., 40.],
                break_before: false,
            },
            TracePoint {
                seconds: 2.,
                values: [250., 350., 50.],
                break_before: false,
            },
        ];
        Snapshot::capture_with_line_defaults(editor.document(), spec, defaults())
            .unwrap()
            .prepare(
                editor.revision(),
                &points,
                |_, e, _, _| Ok(e.clone()),
                || false,
            )
            .unwrap()
            .apply(&mut editor)
            .unwrap();
        for (axis, kind) in [EventType::MoveX, EventType::MoveY, EventType::Rotate]
            .into_iter()
            .enumerate()
        {
            let generated = editor.document().event_values(1, layer, kind).unwrap();
            let initial = editor.document().event_values(1, 0, kind).unwrap();
            let offset = if layer == 0 {
                0.
            } else {
                initial[0]["start"].as_f64().unwrap()
            };
            assert_eq!(
                generated[0]["start"].as_f64().unwrap() + offset,
                points[0].values[axis]
            );
            assert_eq!(
                generated[0]["end"].as_f64().unwrap() + offset,
                points[1].values[axis]
            );
        }
        assert_eq!(
            editor
                .document()
                .event_values(1, 0, EventType::Alpha)
                .unwrap()[0]["start"],
            172.
        );
        assert_eq!(
            editor
                .document()
                .event_values(1, 0, EventType::Speed)
                .unwrap()[0]["start"],
            -12.
        );
        editor.undo();
        assert_eq!(editor.document().as_json(), &raw);
    }
}
