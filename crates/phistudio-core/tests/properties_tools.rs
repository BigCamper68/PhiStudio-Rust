use phistudio_core::tools::{self, Selection, Transform};
use phistudio_core::*;
use serde_json::json;
fn fixture() -> Editor {
    let mut raw = ChartDocument::empty("Extras", 120.)
        .unwrap()
        .as_json()
        .clone();
    raw["judgeLineList"][0]["notes"] = json!([{"type":1,"startTime":[1,0,1],"endTime":[1,0,1],"positionX":0.,"extension":{"keep":true},"hitsound":null}]);
    Editor::new(ChartDocument::from_value(raw).unwrap())
}
#[test]
fn sound_batch_retains_sparse_fields_unknowns_and_atomic_undo() {
    let mut e = fixture();
    let before = e.document().as_json().clone();
    let id = NoteId { line: 0, index: 0 };
    e.execute(Command::SetNoteHitsound {
        ids: vec![id, id],
        reference: Some("sounds/click.wav".into()),
    })
    .unwrap();
    assert_eq!(
        e.document().note_hitsound(id).unwrap(),
        Some("sounds/click.wav")
    );
    assert_eq!(
        e.document().as_json()["judgeLineList"][0]["notes"][0]["extension"],
        json!({"keep":true})
    );
    assert!(e.undo());
    assert_eq!(e.document().as_json(), &before);
    assert!(e.redo());
    let snapshot = e.document().as_json().clone();
    assert!(
        e.execute(Command::SetNoteHitsound {
            ids: vec![id, NoteId { line: 0, index: 99 }],
            reference: None
        })
        .is_err()
    );
    assert_eq!(e.document().as_json(), &snapshot);
    for path in ["../escape", "a/../b", "/x", "C:/x", "a//b", "a/./b"] {
        assert!(validate_hitsound(path).is_err());
    }
}
#[test]
fn controls_keep_original_extensions_order_and_missing_fields_on_undo() {
    let mut e = fixture();
    let before = e.document().as_json().clone();
    e.execute(Command::SetControlCurve {
        line: 0,
        kind: ControlKind::Size,
        points: Some(vec![
            ControlPoint {
                source: None,
                x: 0.,
                value: 1.,
                easing: 1,
            },
            ControlPoint {
                source: None,
                x: 900.,
                value: 0.5,
                easing: 6,
            },
        ]),
    })
    .unwrap();
    let points = e.document().control_points(0, ControlKind::Size).unwrap();
    assert_ne!(points[0].easing, 1);
    assert_eq!(points[1].easing, 6);
    e.undo();
    assert_eq!(e.document().as_json(), &before);
    let mut raw = before;
    raw["judgeLineList"][0]["posControl"] =
        json!([{"x":100.,"pos":2.,"easing":3,"unknown":[1,2]},{"x":0.,"pos":1.,"easing":1}]);
    let mut e = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
    let mut points = e
        .document()
        .control_points(0, ControlKind::Position)
        .unwrap();
    points[0].value = 3.;
    e.execute(Command::SetControlCurve {
        line: 0,
        kind: ControlKind::Position,
        points: Some(points),
    })
    .unwrap();
    assert_eq!(
        e.document().as_json()["judgeLineList"][0]["posControl"][1]["unknown"],
        json!([1, 2])
    );
    e.undo();
    assert_eq!(e.document().as_json(), &raw);
    let bad = vec![
        ControlPoint {
            source: None,
            x: 0.,
            value: 1.,
            easing: 1
        };
        2
    ];
    assert!(
        e.execute(Command::SetControlCurve {
            line: 0,
            kind: ControlKind::Position,
            points: Some(bad)
        })
        .is_err()
    );
    assert_eq!(e.document().as_json(), &raw);
}
#[test]
fn group_deletion_reorder_and_membership_are_reversible_without_changing_line_ids() {
    let mut e = fixture();
    e.execute(Command::AddLine {
        name: "Child".into(),
    })
    .unwrap();
    let before = e.document().as_json().clone();
    for name in ["A", "B"] {
        e.execute(Command::EditGroups(GroupAction::Add(name.into())))
            .unwrap();
    }
    e.execute(Command::EditGroups(GroupAction::Assign {
        lines: vec![0, 1],
        group: 1,
    }))
    .unwrap();
    e.execute(Command::EditGroups(GroupAction::Move { from: 1, to: 2 }))
        .unwrap();
    assert_eq!(e.document().line_group(1), 2);
    e.execute(Command::EditGroups(GroupAction::Delete(2)))
        .unwrap();
    assert_eq!(e.document().line_group(1), 0);
    for _ in 0..5 {
        assert!(e.undo());
    }
    assert_eq!(e.document().as_json(), &before);
    assert!(
        e.execute(Command::EditGroups(GroupAction::Delete(0)))
            .is_err()
    );
    assert_eq!(e.document().as_json(), &before);
}
fn event(a: i64, b: i64) -> LineEvent {
    LineEvent {
        start_time: Beat::new(a as i32, 0, 1).unwrap(),
        end_time: Beat::new(b as i32, 0, 1).unwrap(),
        start: 50.,
        end: 100.,
        ..Default::default()
    }
}
#[test]
fn linked_xy_add_time_edit_value_edit_delete_and_rejected_overlap_are_atomic() {
    let mut e = fixture();
    // Remove the empty chart's default move tracks before testing paired creation.
    let mut raw = e.document().as_json().clone();
    raw["judgeLineList"][0]["eventLayers"][0] = json!({});
    e = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
    tools::create_event(e.document(), 0, 0, EventType::MoveX, event(0, 2), true)
        .unwrap()
        .apply_linked(&mut e)
        .unwrap();
    let x = EventId {
        line: 0,
        layer: 0,
        kind: EventType::MoveX,
        index: 0,
    };
    let y = e.document().xy_pair(x).unwrap();
    let y_value = e.document().event(y).unwrap();
    let mut edited = e.document().event(x).unwrap();
    edited.end_time = Beat::new(3, 0, 1).unwrap();
    edited.end = 999.;
    tools::update_event(e.document(), x, edited, true)
        .unwrap()
        .apply_linked(&mut e)
        .unwrap();
    assert_eq!(
        e.document().event(y).unwrap().end_time,
        Beat::new(3, 0, 1).unwrap()
    );
    assert_eq!(e.document().event(y).unwrap().end, y_value.end);
    let snapshot = e.document().as_json().clone();
    assert!(
        tools::create_event(e.document(), 0, 0, EventType::MoveY, event(1, 4), true)
            .unwrap()
            .apply_linked(&mut e)
            .is_err()
    );
    assert_eq!(e.document().as_json(), &snapshot);
    let selection = linked_selection(
        e.document(),
        &Selection {
            notes: vec![],
            events: vec![x],
        },
    )
    .unwrap();
    assert_eq!(selection.events.len(), 2);
    tools::transform(
        e.document(),
        &selection,
        Transform::Shift {
            beats: Beat::ONE,
            x: 0.,
        },
    )
    .unwrap()
    .apply_linked(&mut e)
    .unwrap();
    e.document().validate_xy_layer(0, 0).unwrap();
    let selection = linked_selection(
        e.document(),
        &Selection {
            notes: vec![],
            events: vec![x],
        },
    )
    .unwrap();
    tools::delete(e.document(), &selection)
        .unwrap()
        .apply_linked(&mut e)
        .unwrap();
    assert!(
        e.document()
            .events(0, 0, EventType::MoveX)
            .unwrap()
            .is_empty()
    );
    for _ in 0..4 {
        e.undo();
    }
    assert_eq!(e.document().as_json(), &raw);
}
#[test]
fn private_sound_import_uses_unique_paths_preserves_source_and_rejects_links() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("hit.wav");
    std::fs::write(&source, b"audio bytes").unwrap();
    let a = import_hitsound_asset(root.path(), &source, "hit.wav").unwrap();
    let b = import_hitsound_asset(root.path(), &source, "hit.wav").unwrap();
    assert_ne!(a, b);
    assert_eq!(
        std::fs::read(hitsound_path(root.path(), &a).unwrap()).unwrap(),
        b"audio bytes"
    );
    assert_eq!(std::fs::read(&source).unwrap(), b"audio bytes");
    assert!(
        list_hitsound_assets(root.path(), Some("hit.wav"))
            .unwrap()
            .contains(&a)
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&source, root.path().join("link.wav")).unwrap();
        assert!(hitsound_path(root.path(), "link.wav").is_err());
    }
}
#[test]
fn settings_upgrade_defaults_and_clamps_native_note_scale() {
    let old: EditorPreferences =
        serde_json::from_value(json!({"version":1,"music_volume":0.3})).unwrap();
    assert_eq!(old.options.preview_note_scale, 1.);
    assert!(old.options.mark_active_line_id);
    assert!(old.options.highlight_current_line);
    let mut options = EditorOptions {
        preview_note_scale: 2.,
        preview_dim: Some(-1.),
        scroll_speed: f32::NAN,
        ..Default::default()
    };
    options = options.normalized();
    assert_eq!(options.preview_note_scale, 1.2);
    assert_eq!(options.preview_dim, Some(0.));
    assert_eq!(options.scroll_speed, 1.);
    let root = tempfile::tempdir().unwrap();
    let settings = EditorPreferences {
        options: EditorOptions {
            preview_aspect: PreviewAspect::FourThree,
            preview_note_scale: 0.875,
            ..Default::default()
        },
        ..old
    };
    save_editor_preferences(root.path(), &settings).unwrap();
    assert_eq!(load_editor_preferences(root.path()).unwrap(), settings);
}

#[test]
fn imported_hitsound_bytes_and_reference_survive_package_export_reimport() {
    use std::io::Read;
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("phone.wav");
    std::fs::write(&source, b"exact phone file bytes").unwrap();
    let assets = root.path().join("assets");
    std::fs::create_dir(&assets).unwrap();
    let reference = import_hitsound_asset(&assets, &source, "phone.wav").unwrap();
    let mut e = fixture();
    e.execute(Command::SetNoteHitsound {
        ids: vec![NoteId { line: 0, index: 0 }],
        reference: Some(reference.clone()),
    })
    .unwrap();
    let export = prepare_project_export(
        e.document(),
        &ProjectOptions::default(),
        Some(&assets),
        None,
    )
    .unwrap();
    let destination = root.path().join("export.pez");
    export.write_to(&destination).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&destination).unwrap()).unwrap();
    let mut bytes = Vec::new();
    zip.by_name(&reference)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, b"exact phone file bytes");
    let project = import_project(
        &destination,
        &root.path().join("library"),
        "export.pez",
        true,
    )
    .unwrap();
    let doc = load_chart(&project.charts[project.suggested.unwrap()].path).unwrap();
    assert_eq!(
        doc.note_hitsound(NoteId { line: 0, index: 0 }).unwrap(),
        Some(reference.as_str())
    );
    assert_eq!(
        std::fs::read(project.assets.join(reference)).unwrap(),
        bytes
    );
}
