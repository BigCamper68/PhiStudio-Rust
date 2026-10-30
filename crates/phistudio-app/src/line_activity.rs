//! Revision-scoped event intervals for the sidebar's Active events filter.
use phistudio_core::{ChartDocument, EventType};
use std::sync::Arc;
#[derive(Default)]
pub(crate) struct ActivityCache {
    entry: Option<(u64, Arc<ActivityIndex>)>,
}
pub(crate) struct ActivityIndex {
    lines: Vec<Vec<(f64, f64)>>,
}
impl ActivityCache {
    pub fn get(&mut self, document: &ChartDocument, revision: u64) -> Arc<ActivityIndex> {
        if self.entry.as_ref().is_none_or(|(r, _)| *r != revision) {
            self.entry = Some((revision, Arc::new(ActivityIndex::build(document))));
        }
        Arc::clone(&self.entry.as_ref().unwrap().1)
    }
}
impl ActivityIndex {
    fn build(document: &ChartDocument) -> Self {
        let lines = (0..document.line_count())
            .map(|line| {
                let mut ranges = vec![];
                for layer in 0..document.event_layer_count(line) {
                    for kind in EventType::ALL {
                        // Match the old filter's per-track failure semantics. Full event projection
                        // runs only once per document revision, never once per popup frame.
                        if let Ok(events) = document.events(line, layer, kind) {
                            ranges.extend(events.into_iter().filter_map(|(_, event)| {
                                let (start, end) =
                                    (event.start_time.as_f64(), event.end_time.as_f64());
                                (start < end).then_some((start, end))
                            }));
                        }
                    }
                }
                ranges.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut merged: Vec<(f64, f64)> = vec![];
                for (start, end) in ranges {
                    if let Some(last) = merged.last_mut()
                        && start <= last.1
                    {
                        last.1 = last.1.max(end);
                    } else {
                        merged.push((start, end));
                    }
                }
                merged
            })
            .collect();
        Self { lines }
    }
    pub fn contains(&self, line: usize, beat: f64) -> bool {
        if !beat.is_finite() {
            return false;
        }
        let Some(ranges) = self.lines.get(line) else {
            return false;
        };
        let end = ranges.partition_point(|(start, _)| *start <= beat);
        end > 0 && beat < ranges[end - 1].1
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use phistudio_core::{Beat, Command, Editor, EventId, LineEvent};
    use serde_json::json;
    fn legacy(doc: &ChartDocument, line: usize, beat: f64) -> bool {
        (0..doc.event_layer_count(line)).any(|layer| {
            EventType::ALL.iter().any(|kind| {
                doc.events(line, layer, *kind).is_ok_and(|events| {
                    events
                        .iter()
                        .any(|(_, e)| e.start_time.as_f64() <= beat && beat < e.end_time.as_f64())
                })
            })
        })
    }
    fn fixture() -> ChartDocument {
        let mut raw = ChartDocument::empty("Activity", 120.)
            .unwrap()
            .as_json()
            .clone();
        let line = raw["judgeLineList"][0].clone();
        raw["judgeLineList"].as_array_mut().unwrap().push(line);
        raw["judgeLineList"][0]["eventLayers"] = json!([{"moveXEvents":[{"startTime":[6,0,1],"endTime":[9,0,1]},{"startTime":[1,0,1],"endTime":[3,0,1]},{"startTime":[2,0,1],"endTime":[5,0,1]},{"startTime":[5,0,1],"endTime":[6,0,1]}],"alphaEvents":[null]}]);
        raw["judgeLineList"][1]["eventLayers"] = json!([null,null,null,null,{"speedEvents":[{"startTime":[10,0,1],"endTime":[12,0,1],"easingType":"preserved speed metadata"}]}]);
        ChartDocument::from_value(raw).unwrap()
    }
    #[test]
    fn merged_index_matches_legacy_boundaries_layers_overlaps_and_bad_tracks() {
        let doc = fixture();
        let before = doc.as_json().clone();
        let index = ActivityIndex::build(&doc);
        for beat in [
            f64::NEG_INFINITY,
            f64::NAN,
            0.,
            0.999,
            1.,
            2.5,
            5.,
            6.,
            8.999,
            9.,
            10.,
            11.999,
            12.,
            100.,
        ] {
            for line in 0..2 {
                assert_eq!(
                    index.contains(line, beat),
                    legacy(&doc, line, beat),
                    "line {line} beat {beat}"
                );
            }
        }
        assert_eq!(index.lines[0], vec![(1., 9.)]);
        assert_eq!(doc.as_json(), &before);
    }
    #[test]
    fn cache_reuses_queries_but_rebuilds_after_edit_undo_and_redo() {
        let mut editor = Editor::new(fixture());
        let mut cache = ActivityCache::default();
        let first = cache.get(editor.document(), editor.revision());
        let same = cache.get(editor.document(), editor.revision());
        assert!(Arc::ptr_eq(&first, &same));
        let id = EventId {
            line: 1,
            layer: 4,
            kind: EventType::Speed,
            index: 0,
        };
        // Read-only imported layer 4 remains indexed; an editable track change must
        // invalidate the same cache and Undo must restore the original intervals.
        assert!(first.contains(id.line, 11.));
        let event = LineEvent {
            start_time: Beat::new(20, 0, 1).unwrap(),
            end_time: Beat::new(21, 0, 1).unwrap(),
            ..Default::default()
        };
        editor
            .execute(Command::AddEvent {
                line: 0,
                layer: 0,
                kind: EventType::MoveY,
                event,
            })
            .unwrap();
        let edited = cache.get(editor.document(), editor.revision());
        assert!(!Arc::ptr_eq(&first, &edited));
        assert!(edited.contains(0, 20.5));
        editor.undo();
        assert!(
            !cache
                .get(editor.document(), editor.revision())
                .contains(0, 20.5)
        );
        editor.redo();
        assert!(
            cache
                .get(editor.document(), editor.revision())
                .contains(0, 20.5)
        );
    }
    #[test]
    fn open_line_popup_tracks_cursor_and_edits_without_rebuilding_each_frame() {
        use crate::workbench::Workbench;
        fn frame(
            app: &mut Workbench,
            ctx: &egui::Context,
            events: Vec<egui::Event>,
        ) -> Vec<(String, egui::Pos2)> {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1194., 834.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| app.ui(ctx, false),
            )
            .shapes
            .into_iter()
            .filter_map(|shape| match shape.1 {
                egui::Shape::Text(t) => {
                    Some((t.galley.job.text.clone(), t.pos + t.galley.size() * 0.5))
                }
                _ => None,
            })
            .collect()
        }
        let mut raw = fixture().as_json().clone();
        raw["judgeLineList"][0]["Name"] = json!("Selected idle");
        raw["judgeLineList"][1]["Name"] = json!("Active now");
        let mut idle = raw["judgeLineList"][0].clone();
        idle["Name"] = json!("Inactive");
        raw["judgeLineList"].as_array_mut().unwrap().push(idle);
        let mut app = Workbench::new(ChartDocument::from_value(raw).unwrap(), None);
        app.advanced.line_filter = 3;
        app.options.mark_active_line_id = true;
        app.beat = 11.;
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![]);
        }
        assert!(
            app.line_activity.entry.is_none(),
            "Closed popup needs no index"
        );
        let labels = frame(&mut app, &ctx, vec![]);
        let pos = labels
            .iter()
            .find(|(t, _)| t == "L0 · Selected idle")
            .unwrap()
            .1;
        let events = [true, false].map(|pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        frame(&mut app, &ctx, events.into());
        let labels = frame(&mut app, &ctx, vec![]);
        assert!(
            labels
                .iter()
                .any(|(t, _)| t.starts_with("L0 · Selected idle ·"))
        );
        assert!(
            labels
                .iter()
                .any(|(t, _)| t.starts_with("L1 · Active now ·"))
        );
        assert!(!labels.iter().any(|(t, _)| t.starts_with("L2 · Inactive ·")));
        let first = Arc::clone(&app.line_activity.entry.as_ref().unwrap().1);
        app.beat = 12.5;
        for _ in 0..5 {
            let labels = frame(&mut app, &ctx, vec![]);
            assert!(
                !labels
                    .iter()
                    .any(|(t, _)| t.starts_with("L1 · Active now ·"))
            );
            assert!(Arc::ptr_eq(
                &first,
                &app.line_activity.entry.as_ref().unwrap().1
            ));
        }
        app.editor
            .execute(Command::AddEvent {
                line: 2,
                layer: 0,
                kind: EventType::MoveY,
                event: LineEvent {
                    start_time: Beat::new(12, 0, 1).unwrap(),
                    end_time: Beat::new(13, 0, 1).unwrap(),
                    ..Default::default()
                },
            })
            .unwrap();
        let labels = frame(&mut app, &ctx, vec![]);
        assert!(labels.iter().any(|(t, _)| t.starts_with("L2 · Inactive ·")));
        assert!(!Arc::ptr_eq(
            &first,
            &app.line_activity.entry.as_ref().unwrap().1
        ));
        app.editor.undo();
        let labels = frame(&mut app, &ctx, vec![]);
        assert!(!labels.iter().any(|(t, _)| t.starts_with("L2 · Inactive ·")));
        assert_eq!(app.line, 0, "Filtering must retain the selected idle line");
    }
    #[test]
    #[ignore = "manual dense-filter timing"]
    fn dense_filter_benchmark() {
        let mut raw = ChartDocument::empty("Dense events", 120.)
            .unwrap()
            .as_json()
            .clone();
        let mut base = raw["judgeLineList"][0].clone();
        let events: Vec<_> = (0..400)
            .map(|i| json!({"startTime":[i*2,0,1],"endTime":[i*2+1,0,1]}))
            .collect();
        base["eventLayers"] = json!([{"moveXEvents":events}]);
        raw["judgeLineList"] = vec![base; 200].into();
        let doc = ChartDocument::from_value(raw).unwrap();
        let index = ActivityIndex::build(&doc);
        let old = std::time::Instant::now();
        let mut count = 0;
        for frame in 0..5 {
            for line in 0..200 {
                count += usize::from(legacy(&doc, line, 400. + frame as f64 * 0.1));
            }
        }
        let old_ms = old.elapsed().as_secs_f64() * 1000. / 5.;
        let new = std::time::Instant::now();
        for frame in 0..10000 {
            for line in 0..200 {
                count += usize::from(
                    std::hint::black_box(&index).contains(line, 400. + (frame % 5) as f64 * 0.1),
                );
            }
        }
        let new_ms = new.elapsed().as_secs_f64() * 1000. / 10000.;
        std::hint::black_box(count);
        println!(
            "200 lines / 80,000 events: old {old_ms:.4} ms/frame, indexed {new_ms:.4} ms/frame, {:.1}x",
            old_ms / new_ms
        );
    }
}
