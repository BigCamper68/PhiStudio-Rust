use phistudio_core::{
    branches::{self, Side},
    versions, *,
};
use serde_json::json;
use std::fs;
fn changed(doc: &ChartDocument, f: impl FnOnce(&mut serde_json::Value)) -> ChartDocument {
    let mut value = doc.as_json().clone();
    f(&mut value);
    ChartDocument::from_value(value).unwrap()
}
fn location(project: ImportedProject) -> ProjectLocation {
    ProjectLocation {
        chart: project.charts[project.suggested.unwrap_or(0)].path.clone(),
        assets: Some(project.assets),
    }
}
fn setup() -> (
    tempfile::TempDir,
    ChartDocument,
    ProjectLocation,
    std::path::PathBuf,
) {
    let root = tempfile::tempdir().unwrap();
    let mut editor = Editor::new(ChartDocument::empty("Branches", 120.).unwrap());
    editor
        .execute(Command::AddLine {
            name: "Second".into(),
        })
        .unwrap();
    let doc = changed(editor.document(), |v| {
        v["future"] = json!({"keep":null,"deleteMe":1,"opaque":[9,0]})
    });
    let loc = create_project(root.path(), &doc).unwrap();
    let ws = loc.assets.clone().unwrap();
    remember_project(root.path(), loc.clone()).unwrap();
    let path = ws.join(".phistudio-workspace.json");
    let mut raw = json!({"phistudio_workspace":1,"source_name":"Branches.pez","assets":".","charts":[{"source":"chart.json","working":"chart.json","format":ChartFormat::Rpe,"options":ProjectOptions::default()}],"suggested":0});
    raw["futureProject"] = json!({"keep":null});
    raw["charts"][0]["futureChart"] = json!([1, 2]);
    raw["charts"][0]["options"]["futureOption"] = json!({"raw":9});
    fs::write(path, serde_json::to_vec(&raw).unwrap()).unwrap();
    fs::write(ws.join("cover.bin"), b"base").unwrap();
    versions::save(
        root.path(),
        &ws,
        "Baseline",
        &doc,
        &loc,
        &ProjectOptions::default(),
    )
    .unwrap();
    (root, doc, loc, ws)
}

#[test]
fn merge_publishes_independent_note_properties_and_restores_them_from_disk() {
    let (root, base, loc, ws) = setup();
    let root = root.path();
    let options = ProjectOptions::default();
    let mut editor = Editor::new(base);
    for beat in [1, 2] {
        editor
            .execute(Command::AddNote {
                line: 0,
                note: Note {
                    start_time: Beat::new(beat, 0, 1).unwrap(),
                    end_time: Beat::new(beat, 0, 1).unwrap(),
                    ..Default::default()
                },
            })
            .unwrap();
    }
    let base = editor.document().clone();
    versions::save(root, &ws, "Notes baseline", &base, &loc, &options).unwrap();
    branches::create(root, &ws, "notes", &base, &loc, &options).unwrap();
    let theirs = changed(&base, |v| {
        v["judgeLineList"][0]["notes"][0]["positionX"] = json!(150.);
        v["judgeLineList"][0]["notes"][1]["size"] = json!(1.5);
    });
    versions::save(root, &ws, "Incoming notes", &theirs, &loc, &options).unwrap();
    let loc = location(branches::checkout(root, &ws, "main", &theirs, &loc, &options).unwrap());
    let ours = changed(&load_chart(&loc.chart).unwrap(), |v| {
        v["judgeLineList"][0]["notes"][0]["speed"] = json!(2.);
        v["judgeLineList"][0]["notes"][1]["positionX"] = json!(-200.);
    });
    let plan = branches::prepare_merge(root, &ws, "notes", &ours, &loc, &options).unwrap();
    assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
    let loc = location(
        branches::apply_merge(root, &ws, plan, "Merged notes", &ours, &loc, &options).unwrap(),
    );
    let merged = load_chart(&loc.chart).unwrap();
    assert_eq!(merged.note(0, 0).unwrap().x, 150.);
    assert_eq!(merged.note(0, 0).unwrap().speed, 2.);
    assert_eq!(merged.note(0, 1).unwrap().x, -200.);
    assert_eq!(merged.note(0, 1).unwrap().size, 1.5);
    let saved = versions::list(root, &ws).unwrap();
    assert!(
        saved
            .iter()
            .any(|v| v.system_name.as_deref() == Some("Before merge"))
    );
    assert_eq!(
        saved
            .iter()
            .find(|v| v.name == "Merged notes")
            .unwrap()
            .parents
            .len(),
        2
    );
}
#[test]
fn branches_merge_disjoint_line_fields_resources_and_options_without_mutating_preview() {
    let (root, base, loc, ws) = setup();
    let root = root.path();
    let options = ProjectOptions::default();
    branches::create(root, &ws, "分支", &base, &loc, &options).unwrap();
    let incoming = changed(&base, |v| {
        v["META"]["composer"] = json!("incoming");
        v["judgeLineList"][1]["eventLayers"][0]["rotateEvents"][0]["start"] = json!(37.);
    });
    fs::write(ws.join("cover.bin"), b"incoming cover").unwrap();
    fs::write(ws.join("incoming.bin"), b"incoming file").unwrap();
    let incoming_version =
        versions::save(root, &ws, "Incoming", &incoming, &loc, &options).unwrap();
    let loc = location(branches::checkout(root, &ws, "main", &incoming, &loc, &options).unwrap());
    let ours = changed(&load_chart(&loc.chart).unwrap(), |v| {
        v["META"]["charter"] = json!("current");
        v["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0]["start"] = json!(22.);
    });
    fs::write(
        loc.assets.as_ref().unwrap().join("current.bin"),
        b"current file",
    )
    .unwrap();
    let ours_options = ProjectOptions {
        aspect_ratio: Some(4. / 3.),
        ..Default::default()
    };
    let count = versions::list(root, &ws).unwrap().len();
    let before = fs::read(ws.join(".phistudio-workspace.json")).unwrap();
    let plan = branches::prepare_merge(root, &ws, "分支", &ours, &loc, &ours_options).unwrap();
    assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
    assert_eq!(versions::list(root, &ws).unwrap().len(), count);
    assert_eq!(
        fs::read(ws.join(".phistudio-workspace.json")).unwrap(),
        before
    );
    let result = location(
        branches::apply_merge(root, &ws, plan, "Merged", &ours, &loc, &ours_options).unwrap(),
    );
    let description: Value =
        serde_json::from_slice(&fs::read(ws.join(".phistudio-workspace.json")).unwrap()).unwrap();
    assert_eq!(description["futureProject"], json!({"keep":null}));
    assert_eq!(description["charts"][0]["futureChart"], json!([1, 2]));
    assert_eq!(
        description["charts"][0]["options"]["futureOption"],
        json!({"raw":9})
    );
    let merged = load_chart(&result.chart).unwrap();
    assert_eq!(merged.as_json()["META"]["composer"], "incoming");
    assert_eq!(merged.as_json()["META"]["charter"], "current");
    assert_eq!(merged.as_json()["future"], base.as_json()["future"]);
    assert_eq!(
        merged.as_json()["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0]["start"],
        22.
    );
    assert_eq!(
        merged.as_json()["judgeLineList"][1]["eventLayers"][0]["rotateEvents"][0]["start"],
        37.
    );
    for (name, bytes) in [
        ("cover.bin", b"incoming cover".as_slice()),
        ("incoming.bin", b"incoming file".as_slice()),
        ("current.bin", b"current file".as_slice()),
    ] {
        assert_eq!(
            fs::read(result.assets.as_ref().unwrap().join(name)).unwrap(),
            bytes
        );
    }
    let book = branches::load(root, &ws).unwrap();
    assert_eq!(book.current, "main");
    let version = versions::load(root, &ws, book.head().unwrap()).unwrap();
    assert_eq!(version.parents.len(), 2);
    let incoming_head = book.heads["分支"].as_ref().unwrap();
    assert!(version.parents.contains(incoming_head));
    assert!(
        versions::load(root, &ws, incoming_head)
            .unwrap()
            .parents
            .contains(&incoming_version.id)
    );
    assert_eq!(version.options.aspect_ratio, Some(4. / 3.));
    assert!(versions::delete(root, &ws, &incoming_version.id).is_err());
    branches::remove(root, &ws, "分支").unwrap();
    assert!(versions::load(root, &ws, &incoming_version.id).is_ok());
}
#[test]
fn conflicts_keep_missing_null_arrays_and_binary_choices_explicit_and_stale_plans_fail() {
    let (root, base, loc, ws) = setup();
    let root = root.path();
    let options = ProjectOptions::default();
    branches::create(root, &ws, "other", &base, &loc, &options).unwrap();
    let incoming = changed(&base, |v| {
        v["future"]["deleteMe"] = Value::Null;
        v["future"]["opaque"] = json!([1, 2]);
    });
    fs::write(ws.join("cover.bin"), b"theirs").unwrap();
    versions::save(root, &ws, "Theirs", &incoming, &loc, &options).unwrap();
    let loc = location(branches::checkout(root, &ws, "main", &incoming, &loc, &options).unwrap());
    let ours = changed(&load_chart(&loc.chart).unwrap(), |v| {
        v["future"].as_object_mut().unwrap().remove("deleteMe");
        v["future"]["opaque"] = json!([3, 4]);
    });
    fs::write(loc.assets.as_ref().unwrap().join("cover.bin"), b"ours").unwrap();
    let mut plan = branches::prepare_merge(root, &ws, "other", &ours, &loc, &options).unwrap();
    assert_eq!(plan.conflicts.len(), 3);
    let paths: std::collections::BTreeSet<_> = plan.conflicts.iter().map(|c| &c.path).collect();
    assert_eq!(paths.len(), 3);
    assert!(
        plan.conflicts
            .iter()
            .any(|c| c.path == "/chart/future/deleteMe"
                && c.current.is_none()
                && c.incoming == Some(Value::Null))
    );
    let before = fs::read(ws.join(".phistudio-workspace.json")).unwrap();
    let count = versions::list(root, &ws).unwrap().len();
    let stale = changed(&ours, |v| {
        v["META"]["name"] = json!("changed after preview")
    });
    let fail = branches::prepare_merge(root, &ws, "other", &ours, &loc, &options).unwrap();
    assert!(branches::apply_merge(root, &ws, fail, "Stale", &stale, &loc, &options).is_err());
    assert_eq!(
        fs::read(ws.join(".phistudio-workspace.json")).unwrap(),
        before
    );
    assert_eq!(versions::list(root, &ws).unwrap().len(), count);
    for i in 0..plan.conflicts.len() {
        let side = if plan.conflicts[i].path.ends_with("deleteMe") {
            Side::Current
        } else {
            Side::Incoming
        };
        plan.choose(i, side);
    }
    let result = location(
        branches::apply_merge(root, &ws, plan, "Resolved", &ours, &loc, &options).unwrap(),
    );
    let merged = load_chart(&result.chart).unwrap();
    assert!(merged.as_json()["future"].get("deleteMe").is_none());
    assert_eq!(merged.as_json()["future"]["keep"], Value::Null);
    assert_eq!(merged.as_json()["future"]["opaque"], json!([1, 2]));
    assert_eq!(
        fs::read(result.assets.unwrap().join("cover.bin")).unwrap(),
        b"theirs"
    );
}
#[test]
fn malformed_branch_checkout_leaves_descriptor_and_heads_untouched() {
    let (root, doc, loc, ws) = setup();
    let root = root.path();
    let options = ProjectOptions::default();
    let book = branches::create(root, &ws, "other", &doc, &loc, &options).unwrap();
    assert!(book.head().is_some());
    let _ = branches::checkout(root, &ws, "main", &doc, &loc, &options).unwrap();
    let book = branches::load(root, &ws).unwrap();
    let id = book.heads["other"].as_ref().unwrap();
    let descriptor = fs::read(ws.join(".phistudio-workspace.json")).unwrap();
    fs::write(
        root.join("versions")
            .join(ws.file_name().unwrap())
            .join("objects")
            .join(&versions::load(root, &ws, id).unwrap().blobs["chart.json"]),
        b"corrupt",
    )
    .unwrap();
    let active = open_project(root, &ws).unwrap();
    let loc = location(active);
    let current = load_chart(&loc.chart).unwrap();
    assert!(branches::checkout(root, &ws, "other", &current, &loc, &options).is_err());
    assert_eq!(branches::load(root, &ws).unwrap(), book);
    assert_eq!(
        fs::read(ws.join(".phistudio-workspace.json")).unwrap(),
        descriptor
    );
    assert!(branches::create(root, &ws, "", &current, &loc, &options).is_err());
    assert!(branches::remove(root, &ws, "main").is_err());
}
use serde_json::Value;
