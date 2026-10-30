use super::*;
#[test]
fn xy_pair_selection_adds_matching_intervals_without_losing_other_objects() {
    let mut app = Workbench::new(
        phistudio_core::ChartDocument::empty("Pairs", 120.).unwrap(),
        None,
    );
    app.select_event(Some(EventId {
        line: 0,
        layer: 0,
        kind: EventType::MoveX,
        index: 0,
    }));
    app.select_xy_pairs();
    assert_eq!(app.selection().events.len(), 2);
    assert!(
        app.selection()
            .events
            .iter()
            .any(|id| id.kind == EventType::MoveY)
    );
    assert!(app.error.is_none());
}
