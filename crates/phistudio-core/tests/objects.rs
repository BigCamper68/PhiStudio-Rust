use phistudio_core::{
    Beat, ChartDocument, Command, Editor, Note, NoteId, StoryEvent, StoryKind,
    objects::{ObjectClipboard, transform},
    story_tools::StoryId,
    tools::{Selection, Transform},
};
fn fixture() -> (Editor, Selection, Vec<StoryId>) {
    let mut doc = ChartDocument::empty("Mixed objects", 120.)
        .unwrap()
        .as_json()
        .clone();
    let line = doc["judgeLineList"][0].clone();
    doc["judgeLineList"].as_array_mut().unwrap().push(line);
    let mut editor = Editor::new(ChartDocument::from_value(doc).unwrap());
    editor
        .execute(Command::AddNote {
            line: 0,
            note: Note::default(),
        })
        .unwrap();
    editor
        .execute(Command::EditStoryboard {
            line: 0,
            kind: StoryKind::Color,
            index: None,
            event: Some(StoryEvent::new(
                StoryKind::Color,
                Beat::new(4, 0, 1).unwrap(),
                Beat::new(6, 0, 1).unwrap(),
            )),
        })
        .unwrap();
    (
        editor,
        Selection {
            notes: vec![NoteId { line: 0, index: 0 }],
            events: vec![],
        },
        vec![StoryId {
            line: 0,
            kind: StoryKind::Color,
            index: 0,
        }],
    )
}
#[test]
fn mixed_clipboard_reanchors_time_and_lines_and_rolls_back_late_story_conflicts() {
    let (mut editor, normal, story) = fixture();
    let original = editor.document().as_json().clone();
    let clipboard = ObjectClipboard::copy(editor.document(), &normal, &story).unwrap();
    assert_eq!(clipboard.len(), 2);
    let encoded = serde_json::to_vec(&clipboard).unwrap();
    let clipboard: ObjectClipboard = serde_json::from_slice(&encoded).unwrap();
    let selected = clipboard
        .paste(editor.document(), 1, 0, Beat::new(8, 0, 1).unwrap(), false)
        .unwrap()
        .apply(&mut editor)
        .unwrap()
        .unwrap();
    assert_eq!(selected.normal.notes.len(), 1);
    assert_eq!(selected.story.len(), 1);
    assert_eq!(
        editor.document().note(1, 0).unwrap().start_time,
        Beat::new(8, 0, 1).unwrap()
    );
    assert_eq!(
        editor.document().story_events(1, StoryKind::Color).unwrap()[0].start_time,
        Beat::new(12, 0, 1).unwrap()
    );
    editor.undo();
    assert_eq!(editor.document().as_json(), &original);
    assert!(
        clipboard
            .paste(editor.document(), 0, 0, Beat::ZERO, false)
            .unwrap()
            .apply(&mut editor)
            .is_err()
    );
    assert_eq!(editor.document().as_json(), &original);
    assert!(editor.can_redo());
}
#[test]
fn mixed_scale_and_reverse_use_one_global_anchor() {
    let (mut editor, normal, story) = fixture();
    let original = editor.document().as_json().clone();
    transform(
        editor.document(),
        &normal,
        &story,
        Transform::ScaleTime(Beat::new(2, 0, 1).unwrap()),
    )
    .unwrap()
    .apply(&mut editor)
    .unwrap();
    assert_eq!(
        editor.document().story_events(0, StoryKind::Color).unwrap()[0].start_time,
        Beat::new(8, 0, 1).unwrap()
    );
    editor.undo();
    assert_eq!(editor.document().as_json(), &original);
    transform(editor.document(), &normal, &story, Transform::ReverseTime)
        .unwrap()
        .apply(&mut editor)
        .unwrap();
    assert_eq!(
        editor.document().note(0, 0).unwrap().start_time,
        Beat::new(6, 0, 1).unwrap()
    );
    assert_eq!(
        editor.document().story_events(0, StoryKind::Color).unwrap()[0].start_time,
        Beat::ZERO
    );
    editor.undo();
    assert_eq!(editor.document().as_json(), &original);
}
