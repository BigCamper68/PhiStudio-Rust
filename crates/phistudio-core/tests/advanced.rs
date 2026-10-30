#[test]
fn storyboard_batch_erase_preserves_unknown_rows_source_order_and_failure_history() {
    use phistudio_core::{Beat, ChartDocument, Command, Editor, StoryEvent, StoryKind};
    let mut raw = ChartDocument::empty("Erase", 120.)
        .unwrap()
        .as_json()
        .clone();
    let mut a = serde_json::to_value(StoryEvent::new(
        StoryKind::ScaleX,
        Beat::ONE,
        Beat::new(2, 0, 1).unwrap(),
    ))
    .unwrap();
    a["unknown"] = serde_json::json!({"keep":true});
    let b = serde_json::to_value(StoryEvent::new(
        StoryKind::ScaleX,
        Beat::new(3, 0, 1).unwrap(),
        Beat::new(4, 0, 1).unwrap(),
    ))
    .unwrap();
    raw["judgeLineList"][0]["extended"] =
        serde_json::json!({"scaleXEvents":[b,a],"scaleYEvents":null,"unknownTrack":[2,8,9]});
    let mut editor = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
    editor
        .execute(Command::EraseStoryboard(vec![(0, StoryKind::ScaleX, 1)]))
        .unwrap();
    assert_eq!(
        editor.document().as_json()["judgeLineList"][0]["extended"]["scaleXEvents"][0],
        raw["judgeLineList"][0]["extended"]["scaleXEvents"][0]
    );
    editor.undo();
    assert_eq!(editor.document().as_json(), &raw);
    assert!(editor.can_redo());
    let revision = editor.revision();
    assert!(
        editor
            .execute(Command::EraseStoryboard(vec![
                (0, StoryKind::ScaleX, 0),
                (0, StoryKind::ScaleY, 99)
            ]))
            .is_err()
    );
    assert_eq!(editor.document().as_json(), &raw);
    assert_eq!(editor.revision(), revision);
    assert!(editor.can_redo());
    editor.redo();
    editor.undo();
    assert_eq!(editor.document().as_json(), &raw);
}
