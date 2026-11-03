use phistudio_core::{ChartDocument, Command, Editor};
use phistudio_sdk::PatchOperation as Op;
use serde_json::{Value, json};

fn editor() -> Editor {
    Editor::new(ChartDocument::empty("Plugins", 120.).unwrap())
}
fn add(path: &str, value: Value) -> Op {
    Op::Add {
        path: path.into(),
        value,
    }
}
fn replace(path: &str, value: Value) -> Op {
    Op::Replace {
        path: path.into(),
        value,
    }
}
fn remove(path: &str) -> Op {
    Op::Remove { path: path.into() }
}

#[test]
fn mixed_edits_have_one_independent_history_entry_and_restore_exact_source() {
    let mut source = editor().document().as_json().clone();
    let line = source["judgeLineList"][0].as_object_mut().unwrap();
    line.shift_remove("notes");
    line.insert("numOfNotes".into(), json!(197));
    source["custom/~key"] = json!({"01": ["first", "last"], "keep": {"v":null}});
    let original = serde_json::to_string(&source).unwrap();
    let mut e = Editor::new(ChartDocument::from_value(source).unwrap());
    let label = String::from("Plugin: Generate and retime");
    assert!(
        e.apply_patch(
            0,
            label,
            vec![
                add("/judgeLineList/0/notes", json!([])),
                add(
                    "/judgeLineList/0/notes/-",
                    json!({"type":1,"startTime":[2,0,1],"extra":{"saved":true}})
                ),
                add("/custom~1~0key/01/1", json!("middle")),
                replace("/BPMList/0/bpm", json!(60)),
                add(
                    "/phiStudioPlugins",
                    json!({"demo.tool":{"schema":1,"events":[{"a":2}]}})
                ),
            ]
        )
        .unwrap()
    );
    assert_eq!(
        e.history_rows(),
        vec![("Plugin: Generate and retime", true)]
    );
    assert_eq!(e.revision(), 1);
    assert_eq!(e.document().note_count(), 1);
    assert_eq!(e.document().as_json()["judgeLineList"][0]["numOfNotes"], 1);
    assert_eq!(e.document().tempo().seconds_at(4.), 4.);
    let after = e.document().to_json().unwrap();
    e.mark_saved();
    assert!(e.undo());
    assert!(e.is_dirty());
    assert_eq!(
        serde_json::to_string(e.document().as_json()).unwrap(),
        original
    );
    assert_eq!(e.document().tempo().seconds_at(4.), 2.);
    assert!(e.redo());
    assert!(!e.is_dirty());
    assert_eq!(e.document().to_json().unwrap(), after);
    assert_eq!(
        ChartDocument::parse(&after).unwrap().as_json(),
        e.document().as_json()
    );
}

#[test]
fn failure_and_stale_work_leave_document_history_revision_and_redo_unchanged() {
    let mut e = editor();
    e.execute(Command::RenameChart("Other".into())).unwrap();
    e.undo();
    let original = e.document().to_json().unwrap();
    let rev = e.revision();
    for operations in [
        vec![
            replace("/META/name", json!("Partial")),
            replace("/BPMList/0/bpm", json!(-1)),
        ],
        vec![replace("/META/name", json!("Partial")), remove("/missing")],
        vec![
            replace("/META/name", json!("Partial")),
            Op::Test {
                path: "/META/name".into(),
                value: json!("Unexpected"),
            },
        ],
        vec![add("/judgeLineList/0/father", json!(0))],
        vec![add(
            "/judgeLineList/0/notes/-",
            json!({"type":2,"startTime":[3,0,1],"endTime":[1,0,1]}),
        )],
        vec![replace("/judgeLineList/0/eventLayers", json!([2]))],
        vec![replace(
            "/judgeLineList/0/eventLayers/0/moveXEvents/0/endTime",
            json!([-1, 0, 1]),
        )],
        vec![add(
            "/judgeLineList/0/extended",
            json!({"colorEvents":[{"start":[999,0,0],"end":[255,255,255]}]}),
        )],
        vec![replace("/judgeLineList/0/bpmfactor", json!(0))],
        vec![replace("/META/name", json!(99))],
        vec![replace("/META/offset", json!("invalid"))],
        vec![add(
            "/judgeLineList/0/extended",
            json!({"scaleXEvents":[
                {"startTime":[0,0,1],"endTime":[2,0,1],"start":1,"end":1},
                {"startTime":[1,0,1],"endTime":[3,0,1],"start":1,"end":1}
            ]}),
        )],
        vec![add("/blockAreaList", json!([{"unexpected":true}]))],
        vec![add("/phiStudioExtra", json!({"bpm":0,"effects":[]}))],
        vec![add(
            "/phiStudioShaderSources",
            json!({"../escape.glsl":"void main(){}"}),
        )],
    ] {
        assert!(e.apply_patch(rev, "Fail".into(), operations).is_err());
        assert_eq!(e.document().to_json().unwrap(), original);
        assert_eq!(e.revision(), rev);
        assert_eq!(e.history_cursor(), 0);
        assert!(e.can_redo());
        assert!(!e.is_dirty());
    }
    assert!(
        e.apply_patch(
            0,
            "Stale".into(),
            vec![replace("/META/name", json!("Stale"))]
        )
        .is_err()
    );
    assert_eq!(e.document().to_json().unwrap(), original);
}

#[test]
fn noop_does_not_invalidate_redo_and_label_ownership_does_not_leak() {
    let mut e = editor();
    e.execute(Command::RenameChart("Other".into())).unwrap();
    e.undo();
    let rev = e.revision();
    assert!(
        !e.apply_patch(
            rev,
            "No change".into(),
            vec![
                replace("/META/name", json!("Temporary")),
                replace("/META/name", json!("Plugins")),
            ]
        )
        .unwrap()
    );
    assert_eq!(e.revision(), rev);
    assert!(e.can_redo());
    e.apply_patch(
        rev,
        format!("Dynamic {}", 7),
        vec![replace("/META/name", json!("Branch"))],
    )
    .unwrap();
    assert!(!e.can_redo());
    assert_eq!(e.undo_label(), Some("Dynamic 7"));
}

#[test]
fn pointer_validation_and_exact_object_and_array_restoration() {
    let mut e = editor();
    e.apply_patch(
        0,
        "Extensions".into(),
        vec![add(
            "/extensions",
            json!({"a":1,"b":2,"c":3,"list":[0,1,2]}),
        )],
    )
    .unwrap();
    let before = serde_json::to_string(e.document().as_json()).unwrap();
    for path in [
        "bad",
        "/bad~2",
        "/extensions/list/01",
        "/extensions/list/+1",
        "/extensions/list/-",
        "/extensions/list/99",
        "/missing/child",
    ] {
        assert!(
            e.apply_patch(1, "Invalid pointer".into(), vec![replace(path, json!(0))])
                .is_err(),
            "{path}"
        );
    }
    e.apply_patch(
        1,
        "Sparse".into(),
        vec![
            remove("/extensions/b"),
            remove("/extensions/list/1"),
            add("/extensions/list/1", json!(10)),
        ],
    )
    .unwrap();
    e.undo();
    assert_eq!(
        serde_json::to_string(e.document().as_json()).unwrap(),
        before
    );
    e.redo();
    assert_eq!(
        e.document().as_json()["extensions"]["list"],
        json!([0, 10, 2])
    );
}

#[test]
fn unrelated_edit_does_not_reject_or_normalize_imported_overlaps_or_extensions() {
    let mut source = editor().document().as_json().clone();
    let event = source["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0].clone();
    source["judgeLineList"][0]["eventLayers"][0]["moveXEvents"]
        .as_array_mut()
        .unwrap()
        .push(event);
    let mut e = Editor::new(ChartDocument::from_value(source.clone()).unwrap());
    e.apply_patch(
        0,
        "Metadata".into(),
        vec![replace("/META/name", json!("Renamed"))],
    )
    .unwrap();
    e.undo();
    assert_eq!(e.document().as_json(), &source);
    assert!(
        e.apply_patch(
            e.revision(),
            "Overlaps".into(),
            vec![replace(
                "/judgeLineList/0/eventLayers/0/moveXEvents/1/start",
                json!(4)
            )]
        )
        .is_err()
    );
}

#[test]
fn removing_notes_restores_imported_counts_and_container_on_undo() {
    let mut e = editor();
    let source = e.document().to_json().unwrap();
    e.apply_patch(
        0,
        "Remove container".into(),
        vec![remove("/judgeLineList/0/notes")],
    )
    .unwrap();
    assert_eq!(e.document().note_count(), 0);
    e.undo();
    assert_eq!(e.document().to_json().unwrap(), source);
}

#[test]
fn extension_data_survives_recovery_and_saved_chart_without_loading_its_plugin() {
    let root = tempfile::tempdir().unwrap();
    let mut e = editor();
    let data =
        json!({"example.tool":{"schema":1,"events":[{"time":[2,1,4],"custom":{"enabled":true}}]}});
    e.apply_patch(
        0,
        "Custom plugin data".into(),
        vec![add("/phiStudioPlugins", data.clone())],
    )
    .unwrap();
    phistudio_core::save_recovery(
        root.path(),
        &phistudio_core::RecoverySnapshot {
            document: e.document().clone(),
            location: None,
            beat: 2.,
            line: 0,
        },
    )
    .unwrap();
    let recovery = phistudio_core::load_recovery(root.path()).unwrap().unwrap();
    assert_eq!(recovery.document.as_json()["phiStudioPlugins"], data);
    let path = root.path().join("chart.json");
    phistudio_core::save_chart(&recovery.document, &path).unwrap();
    assert_eq!(
        phistudio_core::load_chart(&path).unwrap().as_json()["phiStudioPlugins"],
        data
    );
}
