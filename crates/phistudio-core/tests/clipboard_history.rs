use phistudio_core::{
    clipboard_history::History, objects::ObjectClipboard, story_tools::StoryId, tools::Selection, *,
};
use serde_json::json;
#[test]
fn bounded_history_pins_and_mixed_raw_copies_survive_restart_and_source_removal() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("sound.ogg"), b"original sound").unwrap();
    let mut raw = ChartDocument::empty("Clipboard", 120.)
        .unwrap()
        .as_json()
        .clone();
    let mut note = serde_json::to_value(Note::default()).unwrap();
    note["hitsound"] = json!("sound.ogg");
    note["future"] = json!({"keep":null});
    raw["judgeLineList"][0]["notes"] = json!([note]);
    let mut editor = Editor::new(ChartDocument::from_value(raw).unwrap());
    editor
        .execute(Command::EditStoryboard {
            line: 0,
            kind: StoryKind::Text,
            index: None,
            event: Some(StoryEvent::new(StoryKind::Text, Beat::ZERO, Beat::ONE)),
        })
        .unwrap();
    let clip = ObjectClipboard::copy(
        editor.document(),
        &Selection {
            notes: vec![NoteId { line: 0, index: 0 }],
            events: vec![EventId {
                line: 0,
                layer: 0,
                kind: EventType::MoveX,
                index: 0,
            }],
        },
        &[StoryId {
            line: 0,
            kind: StoryKind::Text,
            index: 0,
        }],
    )
    .unwrap();
    let mut history = History::load(root.path()).unwrap();
    history
        .push(root.path(), clip.clone(), Some(source.path()))
        .unwrap();
    let pinned = history.entries[0].id.clone();
    history.pin(root.path(), &pinned, true).unwrap();
    history.rename(root.path(), &pinned, "中文 Имя").unwrap();
    for _ in 0..24 {
        history
            .push(root.path(), clip.clone(), Some(source.path()))
            .unwrap();
    }
    assert_eq!(history.entries.len(), 21);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 22);
    source.close().unwrap();
    let mut history = History::load(root.path()).unwrap();
    assert_eq!(history.entries.last().unwrap().name, "中文 Имя");
    assert_eq!(history.entries.last().unwrap().preview.len(), 3);
    let target = tempfile::tempdir().unwrap();
    let copied = history
        .activate(root.path(), &pinned, Some(target.path()))
        .unwrap();
    let mut receiver = Editor::new(ChartDocument::empty("Target", 120.).unwrap());
    let before = receiver.document().as_json().clone();
    receiver
        .insert_pattern(&copied, 1, 0, Beat::new(8, 0, 1).unwrap(), false)
        .unwrap();
    let raw = &receiver.document().as_json()["judgeLineList"][1]["notes"][0];
    assert_eq!(raw["future"], json!({"keep":null}));
    assert_eq!(
        std::fs::read(hitsound_path(target.path(), raw["hitsound"].as_str().unwrap()).unwrap())
            .unwrap(),
        b"original sound"
    );
    receiver.undo();
    assert_eq!(*receiver.document().as_json(), before);
    history.remove(root.path(), &pinned).unwrap();
    assert_eq!(History::load(root.path()).unwrap().entries.len(), 20);
    assert!(history.activate(root.path(), &pinned, None).is_err());
}
