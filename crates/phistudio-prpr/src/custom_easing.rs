//! Seed authoring curves from native tweens; verify portable compiled playback.
use phistudio_core::{
    ChartError, EventType, LineEvent,
    custom_easing::{Curve, Mode, Node},
};

/// Existing Beziers retain their handles. Standard easing/window shapes are
/// sampled from the original native tween, never a parallel easing table.
pub fn editable_easing(kind: EventType, event: &LineEvent) -> Result<Curve, ChartError> {
    event.validate(kind)?;
    if let Some(binding) = &event.custom_easing {
        return Ok(binding.curve.clone());
    }
    if kind == EventType::Speed || (event.bezier == 0 && event.easing_type == 1) {
        return Ok(Curve::default());
    }
    if event.bezier != 0 {
        let [x1, y1, x2, y2] = event.bezier_points;
        let curve = Curve {
            nodes: vec![
                Node {
                    x: 0.,
                    y: 0.,
                    mode: Mode::Corner,
                    outgoing: Some([x1, y1]),
                    incoming: None,
                },
                Node {
                    x: 1.,
                    y: 1.,
                    mode: Mode::Corner,
                    incoming: Some([x2, y2]),
                    outgoing: None,
                },
            ],
            ..Default::default()
        };
        if curve.validate().is_ok() {
            return Ok(curve);
        }
    }
    let native = crate::EventCurve::new(kind, event);
    let mut nodes = Vec::with_capacity(65);
    for i in 0..=64 {
        let x = i as f64 / 64.;
        let y = if i == 0 {
            0.
        } else if i == 64 {
            1.
        } else {
            f64::from(native.sample(x as f32))
        };
        nodes.push(Node::new(x, y, Mode::Linear));
    }
    let curve = Curve {
        nodes,
        ..Default::default()
    };
    curve.validate()?;
    Ok(curve)
}

#[cfg(test)]
mod tests {
    use super::*;
    use phistudio_core::{Beat, ChartDocument, Editor, EventId, custom_easing::Snapshot};
    use serde_json::json;
    fn parse(doc: &ChartDocument, modern: bool) -> prpr::core::Chart {
        let mut fs = crate::assets::ChartAssets::new(None).unwrap();
        pollster::block_on(prpr::parse::parse_rpe(
            &crate::compatibility::render_source(doc).unwrap(),
            &mut fs,
            prpr::core::ChartExtra::default(),
            modern,
        ))
        .unwrap()
    }
    fn value(curve: &Curve, x: f64) -> f64 {
        if x >= 1. {
            return 1.;
        }
        let segments = curve.segments().unwrap();
        let s = segments.iter().find(|s| x < s.points[3][0]).unwrap();
        s.point(s.parameter_at(x))[1]
    }
    fn authored(kind: EventType, curve: &Curve) -> ChartDocument {
        let mut raw = ChartDocument::empty("Native easing", 120.)
            .unwrap()
            .as_json()
            .clone();
        raw["BPMList"] = json!([{"bpm":120,"startTime":[0,0,1]},{"bpm":75,"startTime":[3,0,1]}]);
        // Keep the exercised interval within prpr's actual judgment clock.
        // Its terminal height sentinel is based on notes/non-speed events.
        raw["judgeLineList"][0]["notes"] = json!([phistudio_core::Note {
            start_time: Beat::new(8, 0, 1).unwrap(),
            end_time: Beat::new(8, 0, 1).unwrap(),
            ..Default::default()
        }]);
        let event = LineEvent {
            end_time: Beat::new(8, 0, 1).unwrap(),
            start: 10.,
            end: 100.,
            ..Default::default()
        };
        raw["judgeLineList"][0]["eventLayers"][0][kind.key()] = json!([event]);
        let mut editor = Editor::new(ChartDocument::from_value(raw).unwrap());
        let id = EventId {
            line: 0,
            layer: 0,
            kind,
            index: 0,
        };
        Snapshot::capture(editor.document(), &[id], editor.revision())
            .unwrap()
            .prepare(curve, || false)
            .unwrap()
            .apply(&mut editor, false)
            .unwrap();
        editor.document().clone()
    }
    fn curves() -> Vec<Curve> {
        vec![
            Curve {
                nodes: vec![
                    Node {
                        x: 0.,
                        y: 0.,
                        mode: Mode::Corner,
                        outgoing: Some([0.08, 1.7]),
                        incoming: None,
                    },
                    Node {
                        x: 0.45,
                        y: 0.5,
                        mode: Mode::Corner,
                        incoming: Some([0.35, -0.7]),
                        outgoing: Some([0.5, 1.3]),
                    },
                    // Equal endpoint values with a genuine excursion in between.
                    Node {
                        x: 0.7,
                        y: 0.5,
                        mode: Mode::Corner,
                        incoming: Some([0.6, 1.3]),
                        outgoing: Some([0.8, -0.2]),
                    },
                    Node {
                        x: 1.,
                        y: 1.,
                        mode: Mode::Corner,
                        incoming: Some([0.9, 0.8]),
                        outgoing: None,
                    },
                ],
                ..Default::default()
            },
            Curve {
                nodes: vec![
                    Node::new(0., 0., Mode::Hold),
                    Node::new(0.4, 0.7, Mode::Hold),
                    Node::new(1., 1., Mode::Linear),
                ],
                ..Default::default()
            },
        ]
    }
    #[test]
    fn all_native_property_tracks_follow_multi_node_curves_across_bpm_changes() {
        for curve in curves() {
            for kind in [
                EventType::MoveX,
                EventType::MoveY,
                EventType::Rotate,
                EventType::Alpha,
            ] {
                let doc = authored(kind, &curve);
                let mut chart = parse(&doc, false);
                let duration = doc.tempo().seconds_at(8.);
                let mut max_error = 0_f64;
                for i in 0..=1000 {
                    let t = i as f64 / 1000.;
                    let time = t * duration;
                    let line = &mut chart.lines[0];
                    let (anim, factor) = match kind {
                        EventType::MoveX => (&mut line.object.translation.0, 675.),
                        EventType::MoveY => (&mut line.object.translation.1, 450.),
                        EventType::Rotate => (&mut line.object.rotation, -1.),
                        EventType::Alpha => (&mut line.object.alpha, 255.),
                        _ => unreachable!(),
                    };
                    anim.set_time(time);
                    let actual = f64::from(anim.now()) * factor;
                    let expected = 10. + 90. * value(&curve, t);
                    max_error = max_error.max((actual - expected).abs());
                }
                assert!(
                    max_error <= 0.0011,
                    "{kind:?}, max native error {max_error}"
                );
            }
        }
    }
    #[test]
    fn native_speed_heights_follow_compiled_curves_in_both_playback_modes() {
        for curve in curves() {
            for version in [140, 170] {
                let mut raw = authored(EventType::Speed, &curve).as_json().clone();
                raw["META"]["RPEVersion"] = json!(version);
                let doc = ChartDocument::from_value(raw).unwrap();
                let duration = doc.tempo().seconds_at(8.);
                for modern in [false, true] {
                    let mut chart = parse(&doc, modern);
                    let mut integral = 0.;
                    let mut max_error = 0_f64;
                    let steps = 10000;
                    for i in 0..steps {
                        let t = (i as f64 + 0.5) / steps as f64;
                        integral += (10. + 90. * value(&curve, t)) * duration / steps as f64;
                        if (i + 1) % 100 == 0 {
                            let h = &mut chart.lines[0].height;
                            h.set_time((i + 1) as f64 * duration / steps as f64);
                            let actual = f64::from(h.now());
                            // Native speed uses SPEED_RATIO; the legacy parser
                            // samples 0.1 ms before interval ends during integration.
                            let expected = integral * (2. / 9. / prpr::core::HEIGHT_RATIO);
                            max_error = max_error.max((actual - expected).abs());
                        }
                    }
                    assert!(
                        max_error < 0.02,
                        "RPE {version}, modern={modern}: max height error {max_error}"
                    );
                }
            }
        }
    }

    #[test]
    fn legacy_easing_seeds_use_native_shapes_and_beziers_keep_exact_handles() {
        for easing_type in 1..=29 {
            let event = LineEvent {
                easing_type,
                easing_left: 0.1,
                easing_right: 0.85,
                ..Default::default()
            };
            let curve = editable_easing(EventType::MoveX, &event).unwrap();
            let native = crate::EventCurve::new(EventType::MoveX, &event);
            for i in 0..=64 {
                let x = i as f64 / 64.;
                assert!(
                    (value(&curve, x) - f64::from(native.sample(x as f32))).abs() < 1e-5,
                    "easing {easing_type}"
                );
            }
        }
        let event = LineEvent {
            bezier: 1,
            bezier_points: [0.15, -0.5, 0.8, 1.5],
            ..Default::default()
        };
        let curve = editable_easing(EventType::MoveX, &event).unwrap();
        assert_eq!(curve.nodes[0].outgoing, Some([0.15, -0.5]));
        assert_eq!(curve.nodes[1].incoming, Some([0.8, 1.5]));
    }

    #[test]
    fn native_cropped_loops_and_reversed_curves_retain_their_source_mapping() {
        use phistudio_core::{Command, custom_easing::Binding};
        let c = Curve {
            nodes: vec![
                Node::new(0., 0., Mode::Linear),
                Node {
                    x: 0.25,
                    y: 0.5,
                    mode: Mode::Corner,
                    incoming: None,
                    outgoing: Some([0.4, 1.5]),
                },
                Node {
                    x: 0.75,
                    y: 0.5,
                    mode: Mode::Linear,
                    incoming: Some([0.6, 1.5]),
                    outgoing: None,
                },
                Node::new(1., 1., Mode::Linear),
            ],
            ..Default::default()
        };
        let original = Binding::new(c).crop([10., 100.], 0.25, 0.75).unwrap();
        for reversed in [false, true] {
            let mut binding = original.clone();
            if reversed {
                binding.reverse([55., 55.]);
            }
            let mut raw = ChartDocument::empty("Cropped", 120.)
                .unwrap()
                .as_json()
                .clone();
            let event = LineEvent {
                start: 55.,
                end: 55.,
                end_time: Beat::new(8, 0, 1).unwrap(),
                custom_easing: Some(Box::new(binding)),
                ..Default::default()
            };
            raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"] = json!([event]);
            let mut editor = Editor::new(ChartDocument::from_value(raw).unwrap());
            let id = EventId {
                line: 0,
                layer: 0,
                kind: EventType::MoveX,
                index: 0,
            };
            let event = editor.document().event(id).unwrap();
            let mut sliced = crate::slice_event(EventType::MoveX, &event, 0.1, 0.7).unwrap();
            sliced.start_time = Beat::ZERO;
            sliced.end_time = event.end_time;
            editor
                .execute(Command::UpdateEvent { id, event: sliced })
                .unwrap();
            let mut edited = editor.document().event(id).unwrap();
            edited.start = 25.;
            edited.end = 125.;
            editor
                .execute(Command::UpdateEvent { id, event: edited })
                .unwrap();
            let event = editor.document().event(id).unwrap();
            let curve = crate::EventCurve::new(EventType::MoveX, &event);
            let mut native = parse(editor.document(), false);
            for i in 0..=1000 {
                let fraction = i as f32 / 1000.;
                native.lines[0]
                    .object
                    .translation
                    .0
                    .set_time(f64::from(fraction) * 4.);
                let actual = f64::from(native.lines[0].object.translation.0.now()) * 675.;
                assert!(
                    (actual - curve.value(fraction)).abs() < 0.0012,
                    "reversed={reversed}, at {fraction}"
                );
            }
        }
    }

    #[test]
    fn projected_hold_endpoints_preserve_the_native_terminal_clock() {
        let hold = curves().remove(1);
        for kind in EventType::ALL {
            let doc = authored(kind, &hold);
            let before = crate::speed_curve::clock::native_horizon(&doc);
            let projected = ChartDocument::from_value(doc.render_value().unwrap()).unwrap();
            assert_eq!(
                crate::speed_curve::clock::native_horizon(&projected),
                before,
                "{kind:?}"
            );
            let mut chart = parse(&doc, true);
            let end = doc.tempo().seconds_at(8.);
            if kind == EventType::Speed {
                let h = &mut chart.lines[0].height;
                h.set_time(end);
                let first = h.now();
                h.set_time(end + 0.2);
                assert!(
                    (f64::from(h.now() - first) - 100. * 0.2 * 2. / 9. / prpr::core::HEIGHT_RATIO)
                        .abs()
                        < 0.001
                );
            }
        }
    }

    #[test]
    fn note_distance_graph_uses_the_same_logical_speed_projection_as_playback() {
        use phistudio_core::{NoteId, speed_curve::SpeedCurveSource};
        let mut raw = authored(EventType::Speed, &curves().remove(0))
            .as_json()
            .clone();
        // Old renderer metadata is superseded by the explicit custom binding.
        // Registering this unused control in prpr's legacy cache would overflow.
        raw["judgeLineList"][0]["eventLayers"][0]["speedEvents"][0]["bezier"] = json!(1);
        raw["judgeLineList"][0]["eventLayers"][0]["speedEvents"][0]["bezierPoints"] =
            json!([0.2, 1e6, 0.8, 1e6]);
        let doc = ChartDocument::from_value(raw).unwrap();
        let mut source = SpeedCurveSource::capture(&doc, NoteId { line: 0, index: 0 }).unwrap();
        source.native_horizon = Some(crate::speed_curve::clock::native_horizon(&doc));
        for modern in [false, true] {
            let mut native = parse(&doc, modern);
            let height = &mut native.lines[0].height;
            height.set_time(doc.tempo().seconds_at(8.));
            let hit = f64::from(height.now());
            let profile = pollster::block_on(crate::speed_curve_profile(
                &source,
                0,
                [Beat::ZERO, Beat::new(8, 0, 1).unwrap()],
                modern,
            ))
            .unwrap();
            for sample in &profile.samples {
                height.set_time(doc.tempo().seconds_at(sample[0]));
                let expected = (hit - f64::from(height.now())) * 450.;
                assert!(
                    (expected - sample[1]).abs() < 0.02,
                    "modern={modern}, {sample:?} vs {expected}"
                );
            }
        }
    }

    #[test]
    fn reversed_hold_boundaries_match_native_values_and_double_reverse_is_exact() {
        use phistudio_core::tools::{self, Selection, Transform};
        let doc = authored(EventType::MoveX, &curves().remove(1));
        let before = doc.as_json().clone();
        let mut editor = Editor::new(doc);
        let id = EventId {
            line: 0,
            layer: 0,
            kind: EventType::MoveX,
            index: 0,
        };
        let selected = Selection {
            notes: vec![],
            events: vec![id],
        };
        tools::transform(editor.document(), &selected, Transform::ReverseTime)
            .unwrap()
            .apply(&mut editor)
            .unwrap();
        let event = editor.document().event(id).unwrap();
        assert!((event.start - 73.).abs() < 1e-9);
        let curve = crate::EventCurve::new(EventType::MoveX, &event);
        let mut native = parse(editor.document(), false);
        let duration = editor.document().tempo().seconds_at(8.);
        for i in 0..=1000 {
            let f = i as f32 / 1000.;
            native.lines[0]
                .object
                .translation
                .0
                .set_time(f64::from(f) * duration);
            assert!(
                (f64::from(native.lines[0].object.translation.0.now()) * 675. - curve.value(f))
                    .abs()
                    < 0.0012,
                "at {f}"
            );
        }
        tools::transform(editor.document(), &selected, Transform::ReverseTime)
            .unwrap()
            .apply(&mut editor)
            .unwrap();
        assert_eq!(editor.document().as_json(), &before);
    }
}
