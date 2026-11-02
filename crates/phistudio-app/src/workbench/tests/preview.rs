use super::*;

#[test]
fn x_scale_handles_preview_commit_and_cancel_share_one_grid_transaction() {
    for anchor_mode in [
        crate::selection_scaling::Anchor::First,
        crate::selection_scaling::Anchor::Center,
        crate::selection_scaling::Anchor::Last,
    ] {
        for cancel in [false, true] {
            let mut app = group_app();
            app.timeline.tool = Tool::Select;
            app.tools.scale_anchor = anchor_mode;
            app.set_selection(phistudio_core::tools::Selection {
                notes: (0..3).map(|index| NoteId { line: 0, index }).collect(),
                events: vec![],
            });
            let before = app.editor.document().as_json().clone();
            let revision = app.editor.revision();
            let grid = test_grid(&app);
            let frame = crate::selection_scaling::frame(&mut app, grid).unwrap();
            let a = frame.handles[0];
            let anchor = frame.anchor;
            let b = egui::pos2(grid.x(anchor) + (a.x - grid.x(anchor)) * 1.5, a.y);
            group_input(
                &mut app,
                grid,
                vec![seek_pointer_button(a, true), egui::Event::PointerMoved(b)],
                false,
            );
            assert!(matches!(
                app.timeline.drag,
                Some(crate::grid_input::Gesture::SelectionScale { moved: true, .. })
            ));
            assert_eq!(*app.editor.document().as_json(), before);
            assert_eq!(app.editor.revision(), revision);
            group_input(&mut app, grid, vec![seek_pointer_button(b, false)], cancel);
            if cancel {
                assert_eq!(*app.editor.document().as_json(), before);
                assert_eq!(app.editor.revision(), revision);
            } else {
                for (i, x) in [-270., 135., 540.].iter().enumerate() {
                    assert!(
                        (app.editor.document().notes(0)[i].1.x - (anchor + (x - anchor) * 1.5))
                            .abs()
                            < 1e-3
                    );
                }
                assert_eq!(app.editor.revision(), revision + 1);
                app.editor.undo();
                assert_eq!(*app.editor.document().as_json(), before);
            }
        }
    }
}
