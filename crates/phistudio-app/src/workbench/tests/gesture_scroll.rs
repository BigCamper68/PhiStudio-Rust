use super::*;
use crate::advanced_ui::{RightMode, StoryId};
use phistudio_core::{EventId, EventType, StoryEvent, StoryKind};

#[test]
fn grid_moves_and_interval_resizes_preserve_docked_and_floating_scroll() {
    for floating in [false, true] {
        for kind in ["note", "hold", "group", "event", "story"] {
            if floating && kind == "group" {
                continue;
            }
            let mut app = Workbench::new(ChartDocument::empty("Scroll", 120.).unwrap(), None);
            let id = NoteId { line: 0, index: 0 };
            let event_id = EventId {
                line: 0,
                layer: 0,
                kind: EventType::MoveX,
                index: 0,
            };
            if kind == "event" {
                let mut event = app.editor.document().event(event_id).unwrap();
                event.end_time = Beat::new(2, 0, 1).unwrap();
                app.commit_authoring(Command::UpdateEvent {
                    id: event_id,
                    event,
                });
            } else if kind == "story" {
                app.open_storyboard();
                app.apply_story(
                    None,
                    StoryKind::ScaleX,
                    Some(StoryEvent {
                        end_time: Beat::new(2, 0, 1).unwrap(),
                        ..Default::default()
                    }),
                );
                app.select_story(Some(StoryId {
                    line: 0,
                    kind: StoryKind::ScaleX,
                    index: 0,
                }));
            } else {
                app.execute(Command::AddNote {
                    line: 0,
                    note: Note {
                        kind: if kind == "hold" {
                            NoteKind::Hold
                        } else {
                            NoteKind::Tap
                        },
                        start_time: Beat::ONE,
                        end_time: Beat::new(if kind == "hold" { 2 } else { 1 }, 0, 1).unwrap(),
                        x: if kind == "group" { -135. } else { 0. },
                        ..Default::default()
                    },
                });
                app.select(Some(id));
                if kind == "group" {
                    app.execute(Command::AddNote {
                        line: 0,
                        note: Note {
                            start_time: Beat::ONE,
                            end_time: Beat::ONE,
                            x: 135.,
                            ..Default::default()
                        },
                    });
                    app.set_selection(phistudio_core::tools::Selection {
                        notes: vec![id, NoteId { line: 0, index: 1 }],
                        events: vec![],
                    });
                }
            }
            app.compact_properties = floating;
            app.tool = Tool::Select;
            let ctx = egui::Context::default();
            ctx.set_style(crate::theme::style());
            let size = Vec2::new(873., 393.);
            for _ in 0..5 {
                frame(&mut app, &ctx, size, vec![], false);
            }
            let zone = crate::scroll::at(
                &ctx,
                if floating {
                    egui::pos2(190., 155.)
                } else {
                    egui::pos2(800., 180.)
                },
            )
            .unwrap_or_else(|| panic!("Missing scroll zone: {kind}/{floating}"));
            let mut state = egui::scroll_area::State::load(&ctx, zone.id).unwrap();
            state.offset.y = 0.;
            state.store(&ctx, zone.id);
            for _ in 0..3 {
                frame(&mut app, &ctx, size, vec![], false);
            }
            let before_offset = egui::scroll_area::State::load(&ctx, zone.id)
                .unwrap()
                .offset;
            assert!(before_offset.y < 1., "Initial scroll: {before_offset:?}");
            assert!(!app.properties_scroll);
            let before = app.editor.document().as_json().clone();
            let grid = test_grid(&app);
            let (start, end) = if kind == "event" || kind == "story" {
                let tracks = if app.advanced.right_mode == RightMode::Storyboard {
                    6.
                } else {
                    5.
                };
                let x = grid.event_rect.left() + grid.event_rect.width() / tracks / 2.;
                (egui::pos2(x, grid.y(2.)), egui::pos2(x, grid.y(2.5)))
            } else if kind == "hold" {
                (
                    egui::pos2(grid.x(0.), grid.y(2.)),
                    egui::pos2(grid.x(0.), grid.y(2.5)),
                )
            } else {
                let x = if kind == "group" { -135. } else { 0. };
                let start = egui::pos2(grid.x(x), grid.y(1.));
                (start, start + egui::vec2(25., -48.))
            };
            grid_events(
                &mut app,
                &[
                    pointer(start, true),
                    egui::Event::PointerMoved(end),
                    pointer(end, false),
                ],
                false,
            );
            assert!(app.error.is_none(), "{kind}: {:?}", app.error);
            assert_ne!(
                app.editor.document().as_json(),
                &before,
                "No grid edit: {kind}"
            );
            assert!(
                !app.properties_scroll,
                "Gesture requested Properties: {kind}/{floating}"
            );
            for _ in 0..4 {
                frame(&mut app, &ctx, size, vec![], false);
            }
            let offset = egui::scroll_area::State::load(&ctx, zone.id)
                .unwrap()
                .offset;
            assert!(
                (offset - before_offset).length() < 1.,
                "Scroll changed after {kind}/{floating}: {before_offset:?} -> {offset:?}"
            );
            assert!(app.editor.undo());
            assert_eq!(app.editor.document().as_json(), &before);
        }
    }
}
