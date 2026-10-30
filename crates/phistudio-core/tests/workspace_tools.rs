use phistudio_core::{objects::ObjectClipboard, story_tools::StoryId, tools::Selection, *};
use std::fs;
#[test]
fn patterns_keep_raw_data_sounds_and_insert_new_lines_as_one_undo() {
    let root = tempfile::tempdir().unwrap();
    let assets = tempfile::tempdir().unwrap();
    fs::write(assets.path().join("custom.ogg"), b"native sound bytes").unwrap();
    let mut raw = ChartDocument::empty("Pattern", 120.)
        .unwrap()
        .as_json()
        .clone();
    raw["judgeLineList"][0]["extended"] = serde_json::Value::Null;
    let mut editor = Editor::new(ChartDocument::from_value(raw).unwrap());
    let mut note = serde_json::to_value(Note::default()).unwrap();
    note["hitsound"] = serde_json::json!("custom.ogg");
    note["futureExtension"] = serde_json::json!({"raw":null});
    let mut doc = editor.document().as_json().clone();
    doc["judgeLineList"][0]["notes"] = serde_json::json!([note]);
    editor = Editor::new(ChartDocument::from_value(doc).unwrap());
    editor
        .execute(Command::EditStoryboard {
            line: 0,
            kind: StoryKind::Text,
            index: None,
            event: Some(StoryEvent::new(
                StoryKind::Text,
                Beat::new(2, 0, 1).unwrap(),
                Beat::new(4, 0, 1).unwrap(),
            )),
        })
        .unwrap();
    let c = ObjectClipboard::copy(
        editor.document(),
        &Selection {
            notes: vec![NoteId { line: 0, index: 0 }],
            events: vec![],
        },
        &[StoryId {
            line: 0,
            kind: StoryKind::Text,
            index: 0,
        }],
    )
    .unwrap();
    let p = patterns::save(root.path(), "My pattern", c, Some(assets.path())).unwrap();
    patterns::favorite(root.path(), &p.id, true).unwrap();
    assert!(patterns::list(root.path()).unwrap()[0].favorite);
    let target = tempfile::tempdir().unwrap();
    let c = patterns::prepare(root.path(), &p.id, Some(target.path()))
        .unwrap()
        .configured(Beat::new(2, 0, 1).unwrap(), [20., 0.])
        .unwrap();
    let before = editor.document().as_json().clone();
    editor.mark_saved();
    let selected = editor
        .insert_pattern(&c, 1, 0, Beat::new(8, 0, 1).unwrap(), false)
        .unwrap();
    assert_eq!(selected.story.len(), 1);
    assert_eq!(editor.document().line_count(), 2);
    let raw = &editor.document().as_json()["judgeLineList"][1]["notes"][0];
    assert_eq!(raw["futureExtension"], serde_json::json!({"raw":null}));
    let sound = raw["hitsound"].as_str().unwrap();
    assert_eq!(
        fs::read(hitsound_path(target.path(), sound).unwrap()).unwrap(),
        b"native sound bytes"
    );
    assert_eq!(
        editor.document().story_events(1, StoryKind::Text).unwrap()[0].start_time,
        Beat::new(12, 0, 1).unwrap()
    );
    editor.undo();
    assert_eq!(editor.document().as_json(), &before);
    assert!(!editor.is_dirty());
    assert!(editor.can_redo());
    assert!(
        editor
            .insert_pattern(&c, 4096, 0, Beat::ZERO, false)
            .is_err()
    );
    assert_eq!(editor.document().as_json(), &before);
    assert!(editor.can_redo());
    patterns::delete(root.path(), &p.id).unwrap();
    assert!(patterns::list(root.path()).unwrap().is_empty());
}
#[test]
fn versions_capture_unsaved_data_restore_atomically_and_reopen_the_new_generation() {
    let root = tempfile::tempdir().unwrap();
    let doc = ChartDocument::empty("Versions", 123.).unwrap();
    let loc = create_project(root.path(), &doc).unwrap();
    let ws = loc.assets.clone().unwrap();
    fs::write(ws.join("cover.png"), b"original").unwrap();
    remember_project(root.path(), loc.clone()).unwrap();
    rename_project(root.path(), &ws, "Local title").unwrap();
    let mut raw = doc.as_json().clone();
    raw["future"] = serde_json::json!({"unknown":null,"order":[3,1,2]});
    let unsaved = ChartDocument::from_value(raw).unwrap();
    let options = ProjectOptions::default();
    let v = versions::save(root.path(), &ws, "First", &unsaved, &loc, &options).unwrap();
    assert_eq!(load_chart(&loc.chart).unwrap().as_json(), doc.as_json());
    fs::write(ws.join("cover.png"), b"changed").unwrap();
    let diff = versions::difference(root.path(), &ws, &v.id, &doc, &loc).unwrap();
    assert!(diff.chart_changed);
    assert!(diff.changed_files.contains(&"cover.png".into()));
    let restored = versions::restore(root.path(), &ws, &v.id, &doc, &loc, &options).unwrap();
    let newloc = ProjectLocation {
        chart: restored.charts[restored.suggested.unwrap()].path.clone(),
        assets: Some(restored.assets),
    };
    assert_eq!(
        load_chart(&newloc.chart).unwrap().as_json(),
        unsaved.as_json()
    );
    assert_eq!(
        fs::read(newloc.assets.as_ref().unwrap().join("cover.png")).unwrap(),
        b"original"
    );
    assert_eq!(fs::read(ws.join("cover.png")).unwrap(), b"changed");
    remember_project(root.path(), newloc.clone()).unwrap();
    assert_eq!(
        open_project(root.path(), &ws).unwrap().charts[0].path,
        newloc.chart
    );
    assert_eq!(
        list_projects(root.path()).unwrap().projects[0].name,
        "Local title"
    );
    let backup = versions::list(root.path(), &ws)
        .unwrap()
        .into_iter()
        .find(|v| v.name == "Before restore")
        .unwrap();
    assert!(backup.files.keys().all(|p| !p.starts_with("generation-")));
    let d = versions::difference(root.path(), &ws, &backup.id, &unsaved, &newloc).unwrap();
    assert!(d.chart_changed);
    let next = versions::save(root.path(), &ws, "Next", &unsaved, &newloc, &options).unwrap();
    assert!(next.files.keys().all(|p| !p.starts_with("generation-")));
    let descriptor = fs::read(ws.join(".phistudio-workspace.json")).unwrap();
    fs::write(
        root.path()
            .join("versions")
            .join(ws.file_name().unwrap())
            .join("objects")
            .join(&v.blobs["cover.png"]),
        b"bad zip",
    )
    .unwrap();
    assert!(versions::restore(root.path(), &ws, &v.id, &unsaved, &newloc, &options).is_err());
    assert_eq!(
        fs::read(ws.join(".phistudio-workspace.json")).unwrap(),
        descriptor
    );
    let copy =
        versions::duplicate(root.path(), &ws, "Independent", &unsaved, &newloc, &options).unwrap();
    assert_ne!(copy.workspace, ws);
    assert_eq!(
        fs::read(copy.assets.join("cover.png")).unwrap(),
        b"original"
    );
    assert_eq!(
        list_projects(root.path())
            .unwrap()
            .projects
            .iter()
            .find(|p| p.workspace == copy.workspace)
            .unwrap()
            .name,
        "Independent"
    );
}
#[test]
fn media_replacement_and_deletion_have_separate_undo_and_protect_references() {
    let root = tempfile::tempdir().unwrap();
    let doc = ChartDocument::empty("Media", 120.).unwrap();
    let loc = create_project(root.path(), &doc).unwrap();
    let assets = loc.assets.unwrap();
    fs::write(assets.join("cover.png"), b"old").unwrap();
    let external = tempfile::NamedTempFile::new().unwrap();
    fs::write(external.path(), b"new").unwrap();
    let options = ProjectOptions {
        illustration: Some("cover.png".into()),
        ..Default::default()
    };
    assert!(resources::delete(root.path(), &assets, "cover.png", &doc, &options).is_err());
    let change = resources::replace(
        root.path(),
        &assets,
        "cover.png",
        external.path(),
        "new.png",
    )
    .unwrap();
    assert_eq!(fs::read(assets.join("cover.png")).unwrap(), b"new");
    resources::undo(change).unwrap();
    assert_eq!(fs::read(assets.join("cover.png")).unwrap(), b"old");
    let (name, undo) = resources::add(&assets, external.path(), "import.png").unwrap();
    assert!(assets.join(&name).exists());
    resources::undo(undo).unwrap();
    assert!(!assets.join(&name).exists());
    let change = resources::delete(
        root.path(),
        &assets,
        "cover.png",
        &doc,
        &ProjectOptions::default(),
    )
    .unwrap();
    assert!(!assets.join("cover.png").exists());
    resources::undo(change).unwrap();
    assert_eq!(fs::read(external.path()).unwrap(), b"new");
    assert!(
        resources::replace(
            root.path(),
            &assets,
            "../outside.png",
            external.path(),
            "new.png"
        )
        .is_err()
    );
}

#[test]
fn inventory_uses_working_references_and_still_protects_other_chart_and_sidecar_files() {
    let root = tempfile::tempdir().unwrap();
    let mut raw = ChartDocument::empty("Inventory", 120.)
        .unwrap()
        .as_json()
        .clone();
    raw["META"]["background"] = serde_json::json!("working.png");
    let doc = ChartDocument::from_value(raw).unwrap();
    let active = root.path().join("chart.json");
    // A saved original is not parsed a second time for the inventory display.
    fs::write(&active, r#"{"saved":"old.png"}"#).unwrap();
    fs::write(
        root.path().join("other.json"),
        r#"{"nested":[{"texture":"shared.png"}]}"#,
    )
    .unwrap();
    fs::write(
        root.path().join("extra.json"),
        r#"{"unknown":{"font":"font.ttf"}}"#,
    )
    .unwrap();
    for name in [
        "working.png",
        "old.png",
        "shared.png",
        "font.ttf",
        "music.mp3",
    ] {
        fs::write(root.path().join(name), b"fixture").unwrap();
    }
    let options = ProjectOptions {
        music: Some("music.mp3".into()),
        ..Default::default()
    };
    let entries = resources::list_with_references(
        root.path(),
        resources::references(&doc, &options).unwrap(),
        Some(&active),
    )
    .unwrap();
    let count = |name: &str| entries.iter().find(|r| r.name == name).unwrap().references;
    assert_eq!(count("working.png"), 1);
    assert_eq!(count("shared.png"), 1);
    assert_eq!(count("font.ttf"), 1);
    assert_eq!(count("music.mp3"), 1);
    assert_eq!(count("old.png"), 0);
    // Deletion still revalidates saved charts as well as the working document.
    assert!(resources::delete(root.path(), root.path(), "old.png", &doc, &options).is_err());
    assert_eq!(
        fs::read_to_string(active).unwrap(),
        r#"{"saved":"old.png"}"#
    );
}
