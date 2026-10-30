use super::*;

fn merge_notes(base: Value, ours: Value, theirs: Value) -> (Value, Vec<Conflict>) {
    let mut conflicts = vec![];
    let value = merge(
        Some(&base),
        Some(&ours),
        Some(&theirs),
        "/chart/judgeLineList/0/notes",
        &BTreeMap::new(),
        &mut conflicts,
        0,
    )
    .unwrap()
    .unwrap();
    (value, conflicts)
}
fn note(beat: i32, x: i32) -> Value {
    json!({"type":1,"startTime":[beat,0,1],"endTime":[beat,0,1],"positionX":x,
        "speed":1,"size":1,"future":{"keep":null}})
}

#[test]
fn independent_notes_and_same_note_fields_merge_without_array_conflicts() {
    let base = json!([note(1, 0), note(2, 0), note(3, 0)]);
    let mut ours = base.clone();
    let mut theirs = base.clone();
    ours[0]["positionX"] = json!(125);
    ours[1]["speed"] = json!(2);
    theirs[1]["size"] = json!(1.5);
    theirs[2]["positionX"] = json!(-120);
    let (merged, conflicts) = merge_notes(base.clone(), ours.clone(), theirs.clone());
    assert!(conflicts.is_empty(), "{conflicts:?}");
    assert_eq!(merged[0], ours[0]);
    assert_eq!(merged[1]["speed"], 2);
    assert_eq!(merged[1]["size"], 1.5);
    assert_eq!(merged[1]["future"], base[1]["future"]);
    assert_eq!(merged[2], theirs[2]);
}

#[test]
fn property_conflicts_are_resolved_without_losing_other_edits() {
    let base = json!([note(1, 0), note(2, 0)]);
    let mut ours = base.clone();
    let mut theirs = base.clone();
    ours[0]["positionX"] = json!(100);
    ours[1]["speed"] = json!(2);
    theirs[0]["positionX"] = json!(200);
    theirs[1]["size"] = json!(3);
    let (_, conflicts) = merge_notes(base.clone(), ours.clone(), theirs.clone());
    assert_eq!(conflicts.len(), 1);
    assert_eq!(
        conflicts[0].path,
        "/chart/judgeLineList/0/notes/0/positionX"
    );
    let choices = BTreeMap::from([(conflicts[0].path.clone(), Side::Incoming)]);
    let merged = merge(
        Some(&base),
        Some(&ours),
        Some(&theirs),
        "/chart/judgeLineList/0/notes",
        &choices,
        &mut vec![],
        0,
    )
    .unwrap()
    .unwrap();
    assert_eq!(merged[0]["positionX"], 200);
    assert_eq!(merged[1]["speed"], 2);
    assert_eq!(merged[1]["size"], 3);
}

#[test]
fn insertions_deletions_and_edit_versus_delete_keep_base_object_addresses() {
    let base = json!([note(1, 0), note(2, 0), note(3, 0)]);
    let ours = json!([note(0, 50), note(1, 0), note(3, 0)]);
    let mut theirs = base.clone();
    theirs[2]["size"] = json!(2);
    let (merged, c) = merge_notes(base.clone(), ours.clone(), theirs.clone());
    assert!(c.is_empty(), "{c:?}");
    assert_eq!(merged.as_array().unwrap().len(), 3);
    assert_eq!(merged[0], note(0, 50));
    assert_eq!(merged[2]["size"], 2);
    theirs[1]["positionX"] = json!(50);
    let (_, c) = merge_notes(base, ours, theirs);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].path, "/chart/judgeLineList/0/notes/1");
    assert!(c[0].current.is_none());
    assert!(c[0].incoming.is_some());
}

#[test]
fn concurrent_insertion_conflicts_are_limited_to_the_gap() {
    let base = json!([note(1, 0), note(3, 0)]);
    let ours = json!([note(1, 0), note(2, 100), note(3, 0)]);
    let theirs = json!([note(1, 0), note(2, -100), note(3, 0)]);
    let (_, c) = merge_notes(base, ours, theirs);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].path, "/chart/judgeLineList/0/notes/insert-1");
}

#[test]
fn duplicate_reordered_and_unidentifiable_tracks_remain_explicit_conflicts() {
    for base in [
        json!([note(1, 0), note(1, 0)]),
        json!([note(1, 0), note(2, 0)]),
    ] {
        let mut ours = base.clone();
        let mut theirs = base.clone();
        ours[0]["size"] = json!(2);
        theirs.as_array_mut().unwrap().reverse();
        theirs[0]["speed"] = json!(3);
        let (_, c) = merge_notes(base, ours, theirs);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].path, "/chart/judgeLineList/0/notes");
    }
    let base = json!([{"opaque":1},{"opaque":2}]);
    let ours = json!([{"opaque":3},{"opaque":4}]);
    let theirs = json!([{"opaque":5},{"opaque":6}]);
    let (_, c) = merge_notes(base, ours, theirs);
    assert_eq!(c[0].path, "/chart/judgeLineList/0/notes");
}

#[test]
fn event_and_story_tracks_merge_fields_and_notes_are_recounted() {
    for path in [
        "/chart/judgeLineList/0/eventLayers/0/moveXEvents",
        "/chart/judgeLineList/0/extended/scaleXEvents",
    ] {
        let base = json!([{"startTime":[0,0,1],"endTime":[4,0,1],"start":0,"end":1,
            "bezierPoints":[0.2,0.3,0.8,0.9],"future":null}]);
        let mut ours = base.clone();
        let mut theirs = base.clone();
        ours[0]["start"] = json!(50);
        theirs[0]["end"] = json!(100);
        let mut c = vec![];
        let merged = merge(
            Some(&base),
            Some(&ours),
            Some(&theirs),
            path,
            &BTreeMap::new(),
            &mut c,
            0,
        )
        .unwrap()
        .unwrap();
        assert!(c.is_empty());
        assert_eq!(merged[0]["start"], 50);
        assert_eq!(merged[0]["end"], 100);
        assert_eq!(merged[0]["bezierPoints"], base[0]["bezierPoints"]);
    }
    let base = json!({"Name":"A","notes":[note(1,0),note(2,0)],"numOfNotes":2});
    let mut ours = base.clone();
    let mut theirs = base.clone();
    ours["notes"].as_array_mut().unwrap().remove(0);
    ours["numOfNotes"] = json!(1);
    theirs["notes"][1]["size"] = json!(2);
    let mut c = vec![];
    let merged = merge(
        Some(&base),
        Some(&ours),
        Some(&theirs),
        "/chart/judgeLineList/0",
        &BTreeMap::new(),
        &mut c,
        0,
    )
    .unwrap()
    .unwrap();
    assert!(c.is_empty());
    assert_eq!(merged["numOfNotes"], 1);
    assert_eq!(merged["notes"][0]["size"], 2);
}
#[test]
fn pending_branch_publication_recovers_by_the_atomic_project_descriptor() {
    let root = tempfile::tempdir().unwrap();
    let doc = ChartDocument::empty("Journal", 120.).unwrap();
    let location = crate::create_project(root.path(), &doc).unwrap();
    let ws = location.assets.clone().unwrap();
    let v = versions::save(
        root.path(),
        &ws,
        "Base",
        &doc,
        &location,
        &ProjectOptions::default(),
    )
    .unwrap();
    let before = load(root.path(), &ws).unwrap();
    let mut after = before.clone();
    after.heads.insert("target".into(), Some(v.id.clone()));
    after.current = "target".into();
    let staging = stage(root.path()).unwrap();
    versions::expand(root.path(), &ws, &v, staging.path()).unwrap();
    package::generation_description(staging.path(), &v.chart, &v.assets, v.options).unwrap();
    let generation = ws.join(staging.path().file_name().unwrap());
    fs::rename(staging.path(), &generation).unwrap();
    let pending = Pending {
        before: before.clone(),
        after: after.clone(),
        generation: generation
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
    };
    let folder = versions::folder(root.path(), &ws).unwrap();
    let path = folder.join("branches.pending.json");
    storage::write_json(&path, &pending).unwrap();
    assert_eq!(load(root.path(), &ws).unwrap(), before);
    assert!(!path.exists());
    storage::write_json(&path, &pending).unwrap();
    package::publish_generation(&ws, &generation).unwrap();
    assert_eq!(load(root.path(), &ws).unwrap(), after);
    assert!(!path.exists());
    assert_eq!(load(root.path(), &ws).unwrap(), after);
}
#[test]
fn duplicate_line_names_and_reordered_lines_are_a_whole_array_conflict() {
    for duplicate in [false, true] {
        let base = json!({"chart":{"judgeLineList":[{"Name":"A","notes":[]},{"Name":if duplicate{"A"}else{"B"},"notes":[]}]}});
        let mut ours = base.clone();
        let mut theirs = base.clone();
        ours["chart"]["judgeLineList"][0]["notes"] = json!([1]);
        if duplicate {
            theirs["chart"]["judgeLineList"][1]["notes"] = json!([2]);
        } else {
            theirs["chart"]["judgeLineList"]
                .as_array_mut()
                .unwrap()
                .reverse();
        }
        let mut conflicts = vec![];
        merge(
            Some(&base),
            Some(&ours),
            Some(&theirs),
            "",
            &BTreeMap::new(),
            &mut conflicts,
            0,
        )
        .unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].path, "/chart/judgeLineList");
    }
}
