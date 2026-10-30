mod hit_test;
pub(crate) use hit_test::hits;
use hit_test::{column, kind_at};
pub use hit_test::{event_rect, pick};
mod gestures;
pub use gestures::{begin, finish, update};
mod paint;
use crate::{
    grid_input::{Gesture, Grid, Intent},
    theme::*,
    workbench::{Tool, Workbench},
};
use egui::{Align2, Color32, FontId, Pos2, Rect, Shape, Stroke, pos2, vec2};
pub use paint::paint;
#[cfg(test)]
use paint::{draw, header_label};
use phistudio_core::{Beat, Command, EventId, EventType, LineEvent};

pub enum EventGesture {
    Create {
        line: usize,
        kind: EventType,
        event: LineEvent,
        grid: Grid,
    },
    Edit {
        id: EventId,
        original: LineEvent,
        current: LineEvent,
        origin: Pos2,
        grid: Grid,
        handle: Option<bool>,
        moved: bool,
    },
}

// Keep the existing compact captions, and measure translations before they
// can spill into the neighbouring track after an inspector resize.

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn translated_event_headers_fit_resized_columns() {
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::definitions());
        let _ = ctx.run(Default::default(), |ctx| {
            let painter = ctx.debug_painter();
            for language in phistudio_core::Language::ALL {
                let _language = crate::i18n::Scope::enter(language);
                for width in [22., 32., 48., 64., 120., 200.] {
                    for kind in EventType::ALL {
                        let text = header_label(&painter, kind, width);
                        let actual = painter
                            .layout_no_wrap(text.clone(), FontId::proportional(12.), Color32::WHITE)
                            .size()
                            .x;
                        assert!(
                            actual <= width - 4.,
                            "{language:?} {kind:?}: {text} ({actual}) exceeds {width}"
                        );
                        if width == 200. {
                            assert_eq!(text, crate::i18n::tr(kind.label()));
                        }
                    }
                }
            }
        });
    }

    #[test]
    fn event_labels_keep_their_endpoint_anchor_and_scissor_at_both_viewport_edges() {
        for width in [250., 500.] {
            for edge in [50., 250.] {
                for offset in [-2., 2.] {
                    let ctx = egui::Context::default();
                    let viewport = Rect::from_min_max(pos2(0., 50.), pos2(width, 250.));
                    let mut grid = Grid {
                        rect: viewport,
                        event_rect: viewport,
                        beat: 0.,
                        zoom: 50.,
                        division: 4,
                        snap_x_enabled: true,
                        scroll_speed: 1.,
                        note_width: 46.,
                        camera_x: 0.,
                        lanes: 11,
                    };
                    // The top label is four points below the event's end edge.
                    grid.beat = 2. - (grid.baseline() - (edge + offset - 4.)) as f64 / 50.;
                    let event = LineEvent {
                        start_time: phistudio_core::Beat::ZERO,
                        end_time: phistudio_core::Beat::new(2, 0, 1).unwrap(),
                        start: 123.,
                        end: 456.,
                        ..Default::default()
                    };
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 300.))),
                            ..Default::default()
                        },
                        |ctx| {
                            let painter = ctx
                                .layer_painter(egui::LayerId::new(
                                    egui::Order::Middle,
                                    egui::Id::new("event-label-test"),
                                ))
                                .with_clip_rect(viewport);
                            draw(
                                &painter,
                                grid,
                                EventId {
                                    line: 0,
                                    layer: 0,
                                    kind: EventType::Rotate,
                                    index: 0,
                                },
                                &event,
                                false,
                                false,
                            );
                        },
                    );
                    let expected = if width < 400. { "456" } else { "123 → 456" };
                    let labels: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.1 {
                            Shape::Text(text) if text.galley.job.text == expected => {
                                Some((&shape.0, text))
                            }
                            _ => None,
                        })
                        .collect();
                    assert_eq!(
                        labels.len(),
                        1,
                        "width {width}, edge {edge}, offset {offset}"
                    );
                    assert!((labels[0].1.pos.y - edge - offset).abs() < 0.1);
                    assert_eq!(labels[0].0.top(), viewport.top());
                    assert_eq!(labels[0].0.bottom(), viewport.bottom());
                }
            }
        }
    }
}
