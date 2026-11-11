//! Authoring displays use native legacy tweens or retained custom geometry.
//! Custom chart playback is projected into the original prpr parser.
use phistudio_core::{EventType, LineEvent};
use prpr::core::{BezierTween, ClampedTween, StaticTween, TweenFunction};
use std::rc::Rc;

pub struct EventCurve {
    native: Rc<dyn TweenFunction>,
    custom: Option<(phistudio_core::custom_easing::Geometry, [f64; 2], [f64; 2])>,
    endpoints: [f64; 2],
}
impl EventCurve {
    pub fn new(kind: EventType, event: &LineEvent) -> Self {
        if let Some(binding) = &event.custom_easing
            && let (Ok(geometry), Ok(source)) = (
                binding.curve.geometry(),
                binding.scalar_source([event.start, event.end]),
            )
        {
            return Self {
                native: StaticTween::get_rc(2),
                custom: Some((geometry, binding.domain, source)),
                endpoints: [event.start, event.end],
            };
        }
        let tween = if kind == EventType::Speed {
            2
        } else {
            prpr::parse::RPE_TWEEN_MAP
                .get(event.easing_type.max(1) as usize)
                .copied()
                .unwrap_or(2)
        };
        let left = event.easing_left.clamp(0., 1.) as f32;
        let right = event.easing_right.clamp(0., 1.) as f32;
        let curve: Rc<dyn TweenFunction> = if kind != EventType::Speed && event.bezier != 0 {
            let p = event.bezier_points.map(|v| v as f32);
            Rc::new(BezierTween::new((p[0], p[1]), (p[2], p[3])))
        } else if tween <= 2 || (left.abs() < 1e-5 && (right - 1.).abs() < 1e-5) || left >= right {
            StaticTween::get_rc(tween)
        } else {
            Rc::new(ClampedTween::new(tween, left..right))
        };
        Self {
            native: curve,
            custom: None,
            endpoints: [event.start, event.end],
        }
    }
    pub fn sample(&self, fraction: f32) -> f32 {
        if let Some((geometry, domain, _)) = &self.custom {
            let reversed = domain[1] < domain[0];
            let p = geometry.sample_in_direction(domain[0], reversed);
            let q = geometry.sample_in_direction(domain[1], reversed);
            let v = geometry.sample_in_direction(
                domain[0] + (domain[1] - domain[0]) * f64::from(fraction.clamp(0., 1.)),
                reversed,
            );
            return if (q - p).abs() > 1e-12 {
                ((v - p) / (q - p)) as f32
            } else {
                v as f32
            };
        }
        self.native.y(fraction.clamp(0., 1.))
    }
    /// Actual values also support cropped loops with equal visible endpoints.
    /// Custom geometry is for authoring; chart playback uses the native projection.
    pub fn value(&self, fraction: f32) -> f64 {
        if let Some((geometry, domain, [a, b])) = &self.custom {
            let x = domain[0] + (domain[1] - domain[0]) * f64::from(fraction.clamp(0., 1.));
            return a + (b - a) * geometry.sample_in_direction(x, domain[1] < domain[0]);
        }
        self.endpoints[0]
            + (self.endpoints[1] - self.endpoints[0]) * f64::from(self.sample(fraction))
    }
}

/// Split editor intervals using the original prpr tween, including clipped
/// easing windows and Bezier control points. Fractions are in playback seconds.
pub fn slice_event(
    kind: EventType,
    event: &LineEvent,
    left: f64,
    right: f64,
) -> Result<LineEvent, phistudio_core::ChartError> {
    use phistudio_core::ChartError;
    let invalid = |message: &str| ChartError::Invalid(message.into());
    if !left.is_finite() || !right.is_finite() || left < 0. || right > 1. + 1e-8 || left >= right {
        return Err(invalid("Invalid event slice"));
    }
    let right = right.min(1.);
    let mut result = event.clone();
    if let Some(binding) = &event.custom_easing {
        result.start = binding.scalar_value([event.start, event.end], left)?;
        result.end = binding.scalar_value([event.start, event.end], right)?;
        result.custom_easing = Some(Box::new(binding.crop(
            [event.start, event.end],
            left,
            right,
        )?));
        return Ok(result);
    }
    let curve = EventCurve::new(kind, event);
    result.start = event.start + (event.end - event.start) * f64::from(curve.sample(left as f32));
    result.end = event.start + (event.end - event.start) * f64::from(curve.sample(right as f32));
    if kind == EventType::Speed
        || event.start == event.end
        || (event.easing_type == 1 && event.bezier == 0)
    {
        return Ok(result);
    }
    if event.bezier == 0 {
        let (a, b) = if event.easing_left >= event.easing_right {
            (0., 1.)
        } else {
            (event.easing_left, event.easing_right)
        };
        result.easing_left = a + (b - a) * left;
        result.easing_right = a + (b - a) * right;
        return Ok(result);
    }
    let [x1, y1, x2, y2] = event.bezier_points;
    if !(0. ..=1.).contains(&x1) || !(0. ..=1.).contains(&x2) {
        return Err(invalid(
            "Bezier splitting requires control X values between 0 and 1",
        ));
    }
    let native = BezierTween::new((x1 as f32, y1 as f32), (x2 as f32, y2 as f32));
    let a = f64::from(native.t_for_x(left as f32));
    let b = f64::from(native.t_for_x(right as f32));
    type Point = [f64; 2];
    fn split(p: [Point; 4], t: f64) -> ([Point; 4], [Point; 4]) {
        let lerp = |a: Point, b: Point| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        let a = lerp(p[0], p[1]);
        let b = lerp(p[1], p[2]);
        let c = lerp(p[2], p[3]);
        let d = lerp(a, b);
        let e = lerp(b, c);
        let f = lerp(d, e);
        ([p[0], a, d, f], [f, e, c, p[3]])
    }
    let (_, tail) = split([[0., 0.], [x1, y1], [x2, y2], [1., 1.]], a);
    let (segment, _) = split(tail, (b - a) / (1. - a));
    let dx = segment[3][0] - segment[0][0];
    let dy = segment[3][1] - segment[0][1];
    if dx.abs() < 1e-12 || dy.abs() < 1e-12 {
        return Err(invalid(
            "This Bezier slice has equal endpoint values; choose another split beat",
        ));
    }
    result.start = event.start + (event.end - event.start) * segment[0][1];
    result.end = event.start + (event.end - event.start) * segment[3][1];
    let normalize = |p: Point| [(p[0] - segment[0][0]) / dx, (p[1] - segment[0][1]) / dy];
    let c1 = normalize(segment[1]);
    let c2 = normalize(segment[2]);
    result.bezier_points = [c1[0], c1[1], c2[0], c2[1]];
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use phistudio_core::{Beat, ChartDocument};
    use prpr::core::ChartExtra;
    #[test]
    fn curve_graph_matches_original_rpe_animation_for_all_easings_and_windows() {
        for easing in 1..=29 {
            for window in [false, true] {
                let event = LineEvent {
                    end_time: Beat::ONE,
                    end: 255.,
                    easing_type: easing,
                    easing_left: if window { 0.2 } else { 0. },
                    easing_right: if window { 0.85 } else { 1. },
                    ..Default::default()
                };
                compare(event);
            }
        }
        compare(LineEvent {
            end: 255.,
            bezier: 1,
            bezier_points: [0.2, -0.3, 0.7, 1.2],
            ..Default::default()
        });
    }
    fn compare(event: LineEvent) {
        let curve = EventCurve::new(EventType::Alpha, &event);
        let mut raw = ChartDocument::empty("Curve", 120.)
            .unwrap()
            .as_json()
            .clone();
        raw["judgeLineList"][0]["eventLayers"][0]["alphaEvents"] = serde_json::json!([event]);
        let mut fs = crate::assets::ChartAssets::new(None).unwrap();
        let mut chart = pollster::block_on(prpr::parse::parse_rpe(
            &serde_json::to_string(&raw).unwrap(),
            &mut fs,
            ChartExtra::default(),
            false,
        ))
        .unwrap();
        for i in 1..100 {
            let x = i as f32 / 100.;
            chart.lines[0].object.alpha.set_time(x as f64 * 0.5);
            assert!(
                (chart.lines[0].object.alpha.now() - curve.sample(x)).abs() < 0.00002,
                "{event:?} at {x}"
            );
        }
    }
}

#[cfg(test)]
mod editing_tests {
    use super::*;
    #[test]
    fn time_reverse_matches_original_prpr_curves_including_windows_and_bezier() {
        for easing in 1..=29 {
            for window in [false, true] {
                for bezier in [false, true] {
                    let original = LineEvent {
                        start: 20.,
                        end: 120.,
                        easing_type: easing,
                        easing_left: if window { 0.2 } else { 0. },
                        easing_right: if window { 0.85 } else { 1. },
                        bezier: u8::from(bezier),
                        bezier_points: [0.2, -0.3, 0.7, 1.2],
                        ..Default::default()
                    };
                    let mut reversed = original.clone();
                    std::mem::swap(&mut reversed.start, &mut reversed.end);
                    phistudio_core::tools::reversed_easing(&mut reversed);
                    let before = EventCurve::new(EventType::Rotate, &original);
                    let after = EventCurve::new(EventType::Rotate, &reversed);
                    for i in 1..100 {
                        let t = i as f32 / 100.;
                        let a = original.start
                            + (original.end - original.start) * f64::from(before.sample(1. - t));
                        let b = reversed.start
                            + (reversed.end - reversed.start) * f64::from(after.sample(t));
                        assert!(
                            (a - b).abs() < 0.004,
                            "Easing {easing}, window {window}, Bezier {bezier}, t={t}: {a} vs {b}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn subdivided_json_reaches_original_rpe_animation_with_bpm_changes() {
        use phistudio_core::{
            Beat, ChartDocument, Editor, EventId,
            tools::{Selection, subdivide_events},
        };
        use prpr::core::ChartExtra;
        for easing in [1, 6, 20, 24, 29] {
            for bezier in [false, true] {
                let mut root = ChartDocument::empty("Native split", 120.)
                    .unwrap()
                    .as_json()
                    .clone();
                root["BPMList"] = serde_json::json!([{"bpm":120.,"startTime":[0,0,1]},{"bpm":60.,"startTime":[2,0,1]}]);
                let event = LineEvent {
                    start_time: Beat::ZERO,
                    end_time: Beat::new(4, 0, 1).unwrap(),
                    start: 100.,
                    end: 110.,
                    easing_type: easing,
                    easing_left: 0.2,
                    easing_right: 0.85,
                    bezier: u8::from(bezier),
                    bezier_points: [0.2, -0.3, 0.7, 1.2],
                    ..Default::default()
                };
                root["judgeLineList"][0]["eventLayers"][0]["alphaEvents"] =
                    serde_json::json!([event]);
                let mut editor = Editor::new(ChartDocument::from_value(root).unwrap());
                let parse = |doc: &ChartDocument| {
                    let mut fs = crate::assets::ChartAssets::new(None).unwrap();
                    pollster::block_on(prpr::parse::parse_rpe(
                        &doc.to_json().unwrap(),
                        &mut fs,
                        ChartExtra::default(),
                        false,
                    ))
                    .unwrap()
                };
                let mut original = parse(editor.document());
                let selected = Selection {
                    notes: vec![],
                    events: vec![EventId {
                        line: 0,
                        layer: 0,
                        kind: EventType::Alpha,
                        index: 0,
                    }],
                };
                subdivide_events(editor.document(), &selected, 4, None, slice_event)
                    .unwrap()
                    .apply(&mut editor)
                    .unwrap();
                let mut split = parse(editor.document());
                for i in 1..120 {
                    let time = i as f64 / 40.;
                    original.lines[0].object.alpha.set_time(time);
                    split.lines[0].object.alpha.set_time(time);
                    assert!(
                        (original.lines[0].object.alpha.now() - split.lines[0].object.alpha.now())
                            .abs()
                            < 0.0002,
                        "easing {easing}, Bezier {bezier}, seconds {time}"
                    );
                }
            }
        }
    }
    #[test]
    fn subdivided_curves_match_native_tweens_at_the_original_seconds() {
        for easing in 1..=29 {
            for window in [false, true] {
                for bezier in [false, true] {
                    let original = LineEvent {
                        start: 20.,
                        end: 120.,
                        easing_type: easing,
                        easing_left: if window { 0.2 } else { 0. },
                        easing_right: if window { 0.85 } else { 1. },
                        bezier: u8::from(bezier),
                        bezier_points: [0.2, -0.3, 0.7, 1.2],
                        ..Default::default()
                    };
                    let before = EventCurve::new(EventType::Rotate, &original);
                    for (left, right) in [(0., 0.33), (0.33, 0.7), (0.7, 1.)] {
                        let sliced =
                            slice_event(EventType::Rotate, &original, left, right).unwrap();
                        let after = EventCurve::new(EventType::Rotate, &sliced);
                        for i in 1..100 {
                            let t = i as f64 / 100.;
                            let a = original.start
                                + (original.end - original.start)
                                    * f64::from(before.sample((left + (right - left) * t) as f32));
                            let b = sliced.start
                                + (sliced.end - sliced.start) * f64::from(after.sample(t as f32));
                            assert!(
                                (a - b).abs() < 0.008,
                                "Easing {easing}, window {window}, Bezier {bezier}, {left}..{right} at {t}: {a} vs {b}"
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Distance controls use the next point's incoming RPE easing, as parse_ctrl_events.
pub struct ControlCurve(prpr::core::AnimFloat);
impl ControlCurve {
    pub fn new(points: &[phistudio_core::ControlPoint]) -> Self {
        let mut points = points.to_vec();
        points.sort_by(|a, b| a.x.total_cmp(&b.x));
        if points.is_empty()
            || (points.len() == 2 && points[0].easing == 1 && (points[0].value - 1.).abs() < 1e-4)
        {
            return Self(prpr::core::Anim::fixed(1.));
        }
        Self(prpr::core::Anim::new(
            points
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    prpr::core::Keyframe::new(
                        p.x,
                        p.value as f32,
                        points
                            .get(i + 1)
                            .and_then(|n| prpr::parse::RPE_TWEEN_MAP.get(n.easing.max(1) as usize))
                            .copied()
                            .unwrap_or(2),
                    )
                })
                .collect(),
        ))
    }
    pub fn sample(&mut self, distance: f64) -> f32 {
        self.0.set_time(distance);
        self.0.now()
    }
}

#[cfg(test)]
mod control_tests {
    use super::*;
    use phistudio_core::{ChartDocument, Command, ControlKind, ControlPoint, Editor};
    #[test]
    fn authored_control_graph_matches_actual_prpr_incoming_easing_for_all_four_controls() {
        for kind in ControlKind::ALL {
            for easing in 1..=29 {
                let mut editor = Editor::new(ChartDocument::empty("Control", 120.).unwrap());
                editor
                    .execute(Command::SetControlCurve {
                        line: 0,
                        kind,
                        points: Some(vec![
                            ControlPoint {
                                source: None,
                                x: 0.,
                                value: 1.,
                                easing: 1,
                            },
                            ControlPoint {
                                source: None,
                                x: 900.,
                                value: 0.4,
                                easing,
                            },
                        ]),
                    })
                    .unwrap();
                let mut fs = crate::assets::ChartAssets::new(None).unwrap();
                let chart = pollster::block_on(prpr::parse::parse_rpe(
                    &editor.document().to_json().unwrap(),
                    &mut fs,
                    prpr::core::ChartExtra::default(),
                    false,
                ))
                .unwrap();
                let mut graph =
                    ControlCurve::new(&editor.document().control_points(0, kind).unwrap());
                let mut native = chart.lines[0].ctrl_obj.borrow_mut();
                for i in 0..=100 {
                    let distance = i as f64 * 9.;
                    native.set_height(distance);
                    let value = match kind {
                        ControlKind::Position => &native.pos,
                        ControlKind::Size => &native.size,
                        ControlKind::Alpha => &native.alpha,
                        ControlKind::Y => &native.y,
                    }
                    .now_opt()
                    .unwrap_or(1.);
                    assert!(
                        (value - graph.sample(distance)).abs() < 1e-6,
                        "{kind:?} easing {easing}"
                    );
                }
            }
        }
    }
    #[test]
    fn synchronize_xy_retains_original_prpr_x_curve_and_raw_extensions_through_bpm_changes() {
        use phistudio_core::{Beat, EventType, LineEvent, tools};
        let mut raw = ChartDocument::empty("Sync", 120.)
            .unwrap()
            .as_json()
            .clone();
        raw["BPMList"] =
            serde_json::json!([{"bpm":120,"startTime":[0,0,1]},{"bpm":60,"startTime":[2,0,1]}]);
        let x = LineEvent {
            end_time: Beat::new(4, 0, 1).unwrap(),
            start: 0.,
            end: 400.,
            easing_type: 6,
            ..Default::default()
        };
        raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"] = serde_json::json!([x]);
        raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0]["custom"] =
            serde_json::json!(["keep"]);
        raw["judgeLineList"][0]["eventLayers"][0]["moveYEvents"] = serde_json::json!([
            LineEvent {
                end_time: Beat::new(1, 0, 1).unwrap(),
                ..Default::default()
            },
            LineEvent {
                start_time: Beat::new(1, 0, 1).unwrap(),
                end_time: Beat::new(4, 0, 1).unwrap(),
                ..Default::default()
            }
        ]);
        let mut e = Editor::new(ChartDocument::from_value(raw.clone()).unwrap());
        let parse = |doc: &ChartDocument| {
            let mut fs = crate::assets::ChartAssets::new(None).unwrap();
            pollster::block_on(prpr::parse::parse_rpe(
                &doc.to_json().unwrap(),
                &mut fs,
                prpr::core::ChartExtra::default(),
                false,
            ))
            .unwrap()
        };
        let mut original = parse(e.document());
        tools::synchronize_xy(e.document(), 0, 0, slice_event)
            .unwrap()
            .apply_linked(&mut e)
            .unwrap();
        e.document().validate_xy_layer(0, 0).unwrap();
        let mut synced = parse(e.document());
        for i in 0..=100 {
            let time = i as f64 * 3. / 100.;
            original.lines[0].object.translation.0.set_time(time);
            synced.lines[0].object.translation.0.set_time(time);
            assert!(
                (original.lines[0].object.translation.0.now()
                    - synced.lines[0].object.translation.0.now())
                .abs()
                    < 1e-5
            );
        }
        assert_eq!(
            e.document().as_json()["judgeLineList"][0]["eventLayers"][0][EventType::MoveX.key()][1]
                ["custom"],
            serde_json::json!(["keep"])
        );
        e.undo();
        assert_eq!(e.document().as_json(), &raw);
    }
}

#[cfg(test)]
mod sound_tests {
    #[test]
    fn custom_audio_reference_reaches_original_rpe_parser_without_rewriting_note() {
        use phistudio_core::{ChartDocument, Command, Editor, NoteId};
        let root = tempfile::tempdir().unwrap();
        let mut wav = Vec::new();
        let len = 960_u32;
        wav.extend(b"RIFF");
        wav.extend((36 + len).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16_u32.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(48000_u32.to_le_bytes());
        wav.extend(96000_u32.to_le_bytes());
        wav.extend(2_u16.to_le_bytes());
        wav.extend(16_u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(len.to_le_bytes());
        wav.resize(44 + len as usize, 0);
        std::fs::write(root.path().join("hit.wav"), wav).unwrap();
        let mut editor = Editor::new(ChartDocument::empty("Sound", 120.).unwrap());
        editor
            .execute(Command::AddNote {
                line: 0,
                note: Default::default(),
            })
            .unwrap();
        editor
            .execute(Command::SetNoteHitsound {
                ids: vec![NoteId { line: 0, index: 0 }],
                reference: Some("hit.wav".into()),
            })
            .unwrap();
        let raw = editor.document().as_json().clone();
        let mut fs = crate::assets::ChartAssets::new(Some(root.path())).unwrap();
        let chart = pollster::block_on(prpr::parse::parse_rpe(
            &editor.document().to_json().unwrap(),
            &mut fs,
            prpr::core::ChartExtra::default(),
            false,
        ))
        .unwrap();
        assert_eq!(chart.hitsounds.len(), 1);
        assert!(chart.hitsounds["hit.wav"].length() > 0.);
        assert!(
            matches!(&chart.lines[0].notes[0].hitsound,prpr::judge::HitSound::Custom(name) if name=="hit.wav")
        );
        assert_eq!(editor.document().as_json(), &raw);
    }
}
