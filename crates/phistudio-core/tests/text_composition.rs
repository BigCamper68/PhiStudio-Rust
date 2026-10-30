use phistudio_core::{
    text_composition::{self, Mode, Spec},
    *,
};
const FONT: &[u8] = include_bytes!("../../../assets/fonts/phira.ttf");
#[test]
fn unicode_outlines_and_fake_notes_generate_ordinary_objects_with_one_exact_undo() {
    for mode in [Mode::Lines, Mode::Tap, Mode::Drag, Mode::Flick] {
        let mut editor = Editor::new(ChartDocument::empty("Original", 120.).unwrap());
        editor.mark_saved();
        let before = editor.document().as_json().clone();
        let revision = editor.revision();
        let spec = Spec {
            text: "Пи 中文".into(),
            mode,
            ..Default::default()
        };
        let plan = text_composition::prepare(spec, FONT, editor.document().tempo()).unwrap();
        assert!(plan.lines > 0 && plan.lines <= 1024);
        assert!(!plan.contours.is_empty());
        let line_count = plan.lines;
        let count = plan.notes;
        let selection = plan.apply(&mut editor).unwrap();
        assert_eq!(editor.document().line_count(), line_count + 1);
        assert_eq!(editor.document().note_count(), count);
        assert_eq!(selection.notes.len(), count);
        assert_eq!(editor.revision(), revision + 1);
        if mode != Mode::Lines {
            for (_, note) in editor.document().notes(1) {
                assert_eq!(note.is_fake, 1);
                assert_eq!(note.start_time, Beat::new(4, 0, 1).unwrap());
                assert_eq!(note.speed, 1.);
            }
        }
        let saved = editor.document().to_json().unwrap();
        let reloaded = ChartDocument::parse(&saved).unwrap();
        assert_eq!(*reloaded.as_json(), *editor.document().as_json());
        editor.undo();
        assert_eq!(*editor.document().as_json(), before);
        assert!(!editor.is_dirty());
        editor.redo();
        assert_eq!(editor.document().line_count(), line_count + 1);
    }
}
#[test]
fn invalid_text_and_excessive_output_do_not_touch_saved_state_or_redo() {
    let mut editor = Editor::new(ChartDocument::empty("Atomic", 120.).unwrap());
    editor
        .execute(Command::AddLine {
            name: "Undo".into(),
        })
        .unwrap();
    editor.undo();
    let before = editor.document().as_json().clone();
    for spec in [
        Spec {
            text: " ".into(),
            ..Default::default()
        },
        Spec {
            text: "\u{10ffff}".into(),
            ..Default::default()
        },
        Spec {
            height: f64::NAN,
            ..Default::default()
        },
        Spec {
            text: "字".repeat(512),
            flatness: 0.1,
            ..Default::default()
        },
    ] {
        assert!(text_composition::prepare(spec, FONT, editor.document().tempo()).is_err());
    }
    assert_eq!(*editor.document().as_json(), before);
    assert!(editor.can_redo());
    let fine = text_composition::prepare(
        Spec {
            text: "S".into(),
            flatness: 0.5,
            ..Default::default()
        },
        FONT,
        editor.document().tempo(),
    )
    .unwrap();
    let coarse = text_composition::prepare(
        Spec {
            text: "S".into(),
            flatness: 4.,
            ..Default::default()
        },
        FONT,
        editor.document().tempo(),
    )
    .unwrap();
    assert!(fine.lines > coarse.lines);
}
