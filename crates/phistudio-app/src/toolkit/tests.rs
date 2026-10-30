use super::*;
use phistudio_core::{ChartDocument, Command, Note};
#[test]
fn localized_workspaces_and_tool_pages_do_not_leave_canonical_captions() {
    for language in phistudio_core::Language::ALL
        .into_iter()
        .filter(|l| *l != phistudio_core::Language::English)
    {
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/locales")
            .join(format!("{}.json", language.code()));
        let catalogue: std::collections::BTreeMap<String, String> =
            serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        let mut app = Workbench::new(ChartDocument::empty("Raw chart title", 120.).unwrap(), None);
        app.options.language = language;
        // User-editable content stays verbatim, including default-looking names.
        app.text_composition.spec.name = "Raw text object name".into();
        let ctx = egui::Context::default();
        ctx.set_style(crate::theme::style());
        ctx.set_fonts(crate::fonts::definitions());
        let mut leftovers = std::collections::BTreeSet::new();
        for stage in 0_usize..15 {
            app.advanced.right_mode = match stage {
                1 => crate::advanced_ui::RightMode::Storyboard,
                2 => crate::advanced_ui::RightMode::Shaders,
                _ => crate::advanced_ui::RightMode::Events,
            };
            app.advanced.history = stage == 4;
            app.tools.show = stage >= 5;
            app.tools.page = [
                Page::Selection,
                Page::Transform,
                Page::Values,
                Page::Notes,
                Page::Events,
                Page::Motion,
                Page::Text,
                Page::Configurations,
                Page::Clipboard,
                Page::Story,
            ][stage.saturating_sub(5).min(9)];
            for _ in 0..3 {
                let out = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1194., 834.),
                        )),
                        ..Default::default()
                    },
                    |ctx| app.ui(ctx, false),
                );
                for shape in &out.shapes {
                    if let egui::Shape::Text(t) = &shape.1 {
                        let text = &t.galley.job.text;
                        if catalogue
                            .get(text)
                            .is_some_and(|translated| translated != text)
                        {
                            leftovers.insert(text.clone());
                        }
                    }
                }
            }
        }
        assert!(
            leftovers.is_empty(),
            "Untranslated {} UI captions: {leftovers:?}",
            language.code()
        );
    }
}

#[test]
fn mixed_selection_profile_uses_only_objects_matching_the_field() {
    let mut app = Workbench::new(ChartDocument::empty("Profile", 120.).unwrap(), None);
    for _ in 0..2 {
        app.execute(Command::AddNote {
            line: 0,
            note: Note::default(),
        });
    }
    app.set_selection(Selection {
        notes: vec![NoteId { line: 0, index: 0 }, NoteId { line: 0, index: 1 }],
        events: vec![EventId {
            line: 0,
            layer: 0,
            kind: EventType::Alpha,
            index: 0,
        }],
    });
    app.tools.values = [10., 30.];
    app.tools.field = ValueField::X;
    assert_eq!(app.profile_values().unwrap(), vec![10., 30.]);
    app.tools.field = ValueField::EventBoth;
    assert_eq!(app.profile_values().unwrap(), vec![10.]);
}
#[test]
fn range_filters_can_select_a_small_region_of_a_chart_over_the_batch_limit() {
    let mut root = ChartDocument::empty("Large range", 120.)
        .unwrap()
        .as_json()
        .clone();
    root["judgeLineList"][0]["notes"] = serde_json::Value::Array(
        (0..=20_000)
            .map(|i| {
                serde_json::to_value(Note {
                    start_time: Beat::new(i, 0, 1).unwrap(),
                    end_time: Beat::new(i, 0, 1).unwrap(),
                    ..Note::default()
                })
                .unwrap()
            })
            .collect(),
    );
    let mut app = Workbench::new(ChartDocument::from_value(root).unwrap(), None);
    app.tools.start = "19999".into();
    app.tools.end = "20000".into();
    app.select_range();
    assert!(app.error.is_none());
    assert_eq!(app.selection().notes.len(), 2);
    assert_eq!(app.selection().notes[0].index, 19999);
}
