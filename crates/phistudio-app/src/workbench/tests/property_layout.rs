use super::*;
use egui::{Context, FullOutput, Rect, Shape, pos2, vec2};
use phistudio_core::{EventId, EventType, Language};

fn render(
    app: &mut Workbench,
    ctx: &Context,
    width: f32,
    floating: bool,
    event: bool,
) -> FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1200., 1000.))),
            ..Default::default()
        },
        |ctx| {
            let _language = crate::i18n::Scope::enter(app.options.language);
            let content = |ui: &mut egui::Ui| {
                if event {
                    app.event_properties(ui);
                } else {
                    app.properties(ui);
                }
            };
            if floating {
                crate::i18n::window("Properties")
                    .default_pos([24., 24.])
                    .default_width(width)
                    .resize(|r| r.max_size([width, 900.]))
                    .show(ctx, content);
            } else {
                egui::SidePanel::right("properties")
                    .exact_width(width)
                    .frame(egui::Frame::none())
                    .show(ctx, content);
            }
        },
    )
}

fn text_rect(output: &FullOutput, value: &str) -> Rect {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.1 {
            Shape::Text(text) if text.galley.job.text == value => {
                Some(Rect::from_min_size(text.pos, text.galley.size()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing {value}"))
}

#[test]
fn property_controls_stay_inline_and_close_to_measured_labels_in_all_languages() {
    for language in Language::ALL {
        for event in [false, true] {
            for floating in [false, true] {
                for width in [190., 220., 400.] {
                    let mut app = Workbench::new(ChartDocument::empty("Form", 120.).unwrap(), None);
                    app.options.language = language;
                    if event {
                        let id = EventId {
                            line: 0,
                            layer: 0,
                            kind: EventType::MoveX,
                            index: 0,
                        };
                        let mut value = app.editor.document().event(id).unwrap();
                        value.start = 123.45;
                        app.commit_authoring(Command::UpdateEvent { id, event: value });
                    } else {
                        app.execute(Command::AddNote {
                            line: 0,
                            note: Note {
                                speed: 2.34,
                                ..Default::default()
                            },
                        });
                        app.select(Some(NoteId { line: 0, index: 0 }));
                    }
                    let ctx = Context::default();
                    ctx.set_fonts(crate::fonts::definitions());
                    ctx.set_style(crate::theme::style());
                    for _ in 0..3 {
                        render(&mut app, &ctx, width, floating, event);
                    }
                    let out = render(&mut app, &ctx, width, floating, event);
                    let _language = crate::i18n::Scope::enter(language);
                    let labels: &[&str] = if event {
                        &[
                            "Start beat",
                            "End beat",
                            "Start value",
                            "End value",
                            "Constant value",
                            "Link group",
                        ]
                    } else {
                        &[
                            "Start",
                            "Position X",
                            "Size",
                            "Speed",
                            "Visible time",
                            "Y offset",
                            "Alpha",
                        ]
                    };
                    let label = text_rect(
                        &out,
                        &crate::i18n::tr(if event { "Start value" } else { "Speed" }),
                    );
                    let control = text_rect(&out, if event { "123.45" } else { "2.34" });
                    // The actual forms must remain paired well below the old 280pt threshold.
                    if width >= 220. || language == Language::English {
                        assert!(
                            (control.center().y - label.center().y).abs() < 2.,
                            "Stacked too early: {language:?}/{width}/{floating}/{event}"
                        );
                    }
                    if width == 400. {
                        let right = labels
                            .iter()
                            .map(|name| text_rect(&out, &crate::i18n::tr(name)).right())
                            .fold(f32::NEG_INFINITY, f32::max);
                        assert!(
                            control.left() - right <= 22.,
                            "Excess label gap: {language:?}/{floating}/{event}: {}",
                            control.left() - right
                        );
                        assert!(control.left() > label.right() + 2.);
                    }
                    for shape in &out.shapes {
                        if let Shape::Text(text) = &shape.1
                            && labels
                                .iter()
                                .any(|label| crate::i18n::tr(label) == text.galley.job.text)
                        {
                            assert!(
                                text.galley.rows.len() <= 2,
                                "Fragmented label: {language:?}: {}",
                                text.galley.job.text
                            );
                        }
                    }
                    assert!(control.min.x >= pos2(0., 0.).x);
                }
            }
        }
    }
}
