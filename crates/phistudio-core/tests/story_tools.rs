use phistudio_core::{
    Beat, ChartDocument, Editor, StoryEvent, StoryKind,
    story_tools::{StoryClipboard, StoryEdit, transform},
    tools::Transform,
};
use serde_json::json;
fn fixture() -> Editor {
    let mut value = ChartDocument::empty("Story tools", 120.)
        .unwrap()
        .as_json()
        .clone();
    value["judgeLineList"][0]["extended"] = json!(null);
    let line = value["judgeLineList"][0].clone();
    value["judgeLineList"].as_array_mut().unwrap().push(line);
    Editor::new(ChartDocument::from_value(value).unwrap())
}
fn event(kind: StoryKind, start: i32, end: i32) -> serde_json::Value {
    let mut value = serde_json::to_value(StoryEvent::new(
        kind,
        Beat::new(start, 0, 1).unwrap(),
        Beat::new(end, 0, 1).unwrap(),
    ))
    .unwrap();
    value["futureExtension"] = json!({"untouched":[null,17,"kept"]});
    value
}
#[test]
fn multi_track_null_parent_undo_and_failed_transaction_preserve_redo() {
    let mut editor = fixture();
    let original = editor.document().as_json().clone();
    let mut edit = StoryEdit::new("Insert test pattern");
    edit.add(0, StoryKind::ScaleX, event(StoryKind::ScaleX, 0, 2));
    edit.add(0, StoryKind::Color, event(StoryKind::Color, 1, 3));
    edit.add(1, StoryKind::Text, event(StoryKind::Text, 2, 4));
    let ids = edit.apply(&mut editor).unwrap().unwrap();
    assert_eq!(ids.len(), 3);
    let changed = editor.document().as_json().clone();
    assert!(editor.undo());
    assert_eq!(editor.document().as_json(), &original);
    assert!(!editor.is_dirty());
    assert!(editor.redo());
    assert_eq!(editor.document().as_json(), &changed);
    editor.undo();
    let mut bad = StoryEdit::new("Invalid pattern");
    bad.add(0, StoryKind::ScaleX, event(StoryKind::ScaleX, 0, 1));
    let mut invalid = event(StoryKind::Color, 0, 1);
    invalid["start"] = json!([999, 0, 0]);
    bad.add(0, StoryKind::Color, invalid);
    assert!(bad.apply(&mut editor).is_err());
    assert_eq!(editor.document().as_json(), &original);
    assert!(editor.can_redo());
    assert!(!editor.is_dirty());
}
#[test]
fn clipboard_and_transforms_retain_raw_extensions_and_reject_conflicts_atomically() {
    let mut editor = fixture();
    let mut edit = StoryEdit::new("Source");
    edit.add(0, StoryKind::ScaleX, event(StoryKind::ScaleX, 0, 2));
    edit.add(0, StoryKind::Text, event(StoryKind::Text, 2, 4));
    let ids = edit.apply(&mut editor).unwrap().unwrap();
    let original = editor.document().as_json().clone();
    let copy = StoryClipboard::copy(editor.document(), &ids).unwrap();
    let inserted = copy
        .paste(editor.document(), 1, Beat::new(4, 0, 1).unwrap())
        .unwrap()
        .apply(&mut editor)
        .unwrap()
        .unwrap();
    assert!(inserted.iter().all(|id| id.line == 1));
    assert_eq!(
        editor.document().as_json()["judgeLineList"][1]["extended"]["scaleXEvents"][0]["futureExtension"],
        json!({"untouched":[null,17,"kept"]})
    );
    editor.undo();
    assert_eq!(editor.document().as_json(), &original);
    assert!(
        copy.paste(editor.document(), 0, Beat::ZERO)
            .unwrap()
            .apply(&mut editor)
            .is_err()
    );
    assert!(editor.can_redo());
    let transformed = transform(
        editor.document(),
        &ids,
        Transform::ScaleTime(Beat::new(2, 0, 1).unwrap()),
    )
    .unwrap()
    .apply(&mut editor)
    .unwrap()
    .unwrap();
    assert_eq!(transformed.len(), 2);
    assert_eq!(
        editor.document().story_events(0, StoryKind::Text).unwrap()[0].end_time,
        Beat::new(8, 0, 1).unwrap()
    );
    editor.undo();
    assert_eq!(editor.document().as_json(), &original);
}
