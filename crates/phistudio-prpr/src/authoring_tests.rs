//! Authored extended events must reach the unchanged native RPE evaluator.
use phistudio_core::{Beat, ChartDocument, Command, Editor, StoryEvent, StoryKind};
use serde_json::json;
#[test]
fn edited_offsets_and_legacy_floats_reach_native_parser_without_losing_timing() {
    for offset in [150., -150., 0., -125.5] {
        let mut editor = Editor::new(ChartDocument::empty("Offset", 120.).unwrap());
        let original = editor.document().as_json().clone();
        let mut meta = phistudio_core::ChartMetadata::read(editor.document(), &Default::default());
        meta.offset_ms = offset;
        editor.execute(Command::EditMetadata(meta)).unwrap();
        let mut fs = crate::assets::ChartAssets::new(None).unwrap();
        let source = crate::compatibility::render_source(editor.document()).unwrap();
        let mut native = pollster::block_on(prpr::parse::parse_rpe(
            &source,
            &mut fs,
            prpr::core::ChartExtra::default(),
            false,
        ))
        .unwrap();
        crate::compatibility::apply(editor.document(), &mut native);
        assert_eq!(native.offset, (offset / 1000.) as f32);
        editor.undo();
        assert_eq!(editor.document().as_json(), &original);

        let mut raw = original.clone();
        raw["META"]["offset"] = json!(offset);
        let legacy = ChartDocument::from_value(raw.clone()).unwrap();
        let source = crate::compatibility::render_source(&legacy).unwrap();
        let mut native = pollster::block_on(prpr::parse::parse_rpe(
            &source,
            &mut fs,
            prpr::core::ChartExtra::default(),
            false,
        ))
        .unwrap();
        crate::compatibility::apply(&legacy, &mut native);
        assert_eq!(native.offset, (offset / 1000.) as f32);
        assert_eq!(legacy.as_json(), &raw);
    }
}
#[test]
fn native_text_faces_cover_project_names_storyboard_text_and_emoji() {
    use glyph_brush::ab_glyph::{Font, FontArc};
    let text = FontArc::try_from_slice(crate::TEXT_FONT_BYTES).unwrap();
    let emoji = FontArc::try_from_slice(crate::EMOJI_FONT_BYTES).unwrap();
    for c in "Русский ё Україна Ω Ⅱ ← ↗ ↔ ★ ♯ ∞ 中文 日本語 한글 ไทย 😀 🎵"
        .chars()
        .filter(|c| !c.is_whitespace())
    {
        assert!(
            text.glyph_id(c).0 != 0 || emoji.glyph_id(c).0 != 0,
            "Native glyph missing: {c}"
        );
    }
}
#[test]
fn storyboard_scale_color_text_and_incline_reach_original_native_animation() {
    for text in [false, true] {
        let mut editor = Editor::new(ChartDocument::empty("Storyboard", 120.).unwrap());
        for (kind, a, b) in [
            (StoryKind::ScaleX, json!(1.), json!(2.)),
            (StoryKind::ScaleY, json!(2.), json!(4.)),
            (StoryKind::Color, json!([255, 0, 0]), json!([0, 0, 255])),
            (StoryKind::Incline, json!(10.), json!(30.)),
        ] {
            let mut event = StoryEvent::new(kind, Beat::ZERO, Beat::new(2, 0, 1).unwrap());
            event.start = a;
            event.end = b;
            editor
                .execute(Command::EditStoryboard {
                    line: 0,
                    kind,
                    index: None,
                    event: Some(event),
                })
                .unwrap();
        }
        if text {
            let mut event =
                StoryEvent::new(StoryKind::Text, Beat::ZERO, Beat::new(2, 0, 1).unwrap());
            event.start = json!("Native text");
            event.end = event.start.clone();
            editor
                .execute(Command::EditStoryboard {
                    line: 0,
                    kind: StoryKind::Text,
                    index: None,
                    event: Some(event),
                })
                .unwrap();
        }
        let mut fs = crate::assets::ChartAssets::new(None).unwrap();
        let mut native = pollster::block_on(prpr::parse::parse_rpe(
            &editor.document().to_json().unwrap(),
            &mut fs,
            prpr::core::ChartExtra::default(),
            false,
        ))
        .unwrap();
        let line = &mut native.lines[0];
        line.object.scale.0.set_time(0.5);
        line.object.scale.1.set_time(0.5);
        line.color.set_time(0.5);
        line.incline.set_time(0.5);
        assert!((line.object.scale.0.now() - if text { 1.5 } else { 0.75 }).abs() < 1e-5);
        assert!((line.object.scale.1.now() - 3.).abs() < 1e-5);
        assert!((line.color.now().r - 0.5).abs() < 1e-5);
        assert!((line.color.now().b - 0.5).abs() < 1e-5);
        assert!((line.incline.now() - 20.).abs() < 1e-5);
        if text {
            if let prpr::core::JudgeLineKind::Text(anim) = &mut line.kind {
                anim.set_time(0.5);
                assert_eq!(anim.now(), "Native text");
            } else {
                panic!("Authored text did not reach native text line");
            }
        }
    }
}

#[test]
fn saved_trajectory_events_play_through_original_prpr_with_bpm_changes() {
    use phistudio_core::{
        EventType,
        trajectories::{self, Shape, Spec},
    };
    let mut root = ChartDocument::empty("Trajectory", 120.)
        .unwrap()
        .as_json()
        .clone();
    root["BPMList"] = json!([{"startTime":[0,0,1],"bpm":120},{"startTime":[2,0,1],"bpm":60}]);
    let mut editor = Editor::new(ChartDocument::from_value(root).unwrap());
    let spec = Spec {
        shape: Shape::Ellipse,
        tangent: true,
        ..Default::default()
    };
    let edit = trajectories::prepare(
        editor.document(),
        0,
        1,
        None,
        "Original renderer",
        spec.clone(),
        |_, t| t,
    )
    .unwrap()
    .0;
    let points = edit.points().to_vec();
    edit.apply(&mut editor).unwrap();
    let reloaded = ChartDocument::parse(&editor.document().to_json().unwrap()).unwrap();
    assert_eq!(
        trajectories::recipes(&reloaded, 0).unwrap()[0].name,
        "Original renderer"
    );
    let mut fs = crate::assets::ChartAssets::new(None).unwrap();
    let mut native = pollster::block_on(prpr::parse::parse_rpe(
        &reloaded.to_json().unwrap(),
        &mut fs,
        prpr::core::ChartExtra::default(),
        false,
    ))
    .unwrap();
    for (i, p) in points.iter().enumerate().skip(1).take(points.len() - 2) {
        let time = reloaded.tempo().seconds_at(i as f64 * spec.step.as_f64());
        let line = &mut native.lines[0];
        line.object.translation.0.set_time(time);
        line.object.translation.1.set_time(time);
        assert!((f64::from(line.object.translation.0.now()) - p[0] / 675.).abs() < 1e-5);
        assert!((f64::from(line.object.translation.1.now()) - p[1] / 450.).abs() < 1e-5);
    }
    assert!(
        !reloaded
            .event_values(0, 1, EventType::Rotate)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn composed_text_lines_and_stationary_fake_notes_reach_original_parser() {
    use phistudio_core::text_composition::{self, Mode, Spec};
    for mode in [Mode::Lines, Mode::Tap, Mode::Drag, Mode::Flick] {
        let mut editor = Editor::new(ChartDocument::empty("Text objects", 120.).unwrap());
        let spec = Spec {
            text: "P".into(),
            mode,
            center: [80., 55.],
            rotation: 37.,
            start: Beat::ONE,
            end: Beat::new(3, 0, 1).unwrap(),
            ..Default::default()
        };
        let plan =
            text_composition::prepare(spec, crate::TEXT_FONT_BYTES, editor.document().tempo())
                .unwrap();
        let aspect = plan.spec.aspect;
        let expected = if mode == Mode::Lines {
            let pair = plan
                .contours
                .iter()
                .flat_map(|p| p.windows(2))
                .find(|p| (p[0][0] - p[1][0]).hypot(p[0][1] - p[1][1]) > 1e-8)
                .unwrap();
            vec![[
                (pair[0][0] + pair[1][0]) / 2.,
                (pair[0][1] + pair[1][1]) / 2.,
            ]]
        } else {
            plan.marks.clone()
        };
        let count = plan.notes;
        let lines = plan.lines;
        plan.apply(&mut editor).unwrap();
        let mut fs = crate::assets::ChartAssets::new(None).unwrap();
        let mut native = pollster::block_on(prpr::parse::parse_rpe(
            &editor.document().to_json().unwrap(),
            &mut fs,
            prpr::core::ChartExtra::default(),
            false,
        ))
        .unwrap();
        assert_eq!(native.lines.len(), lines + 1);
        if mode == Mode::Lines {
            assert!(
                native
                    .lines
                    .iter()
                    .skip(2)
                    .all(|l| l.parent == Some(1) && l.rot_with_parent)
            );
        }
        native.lines[1].object.set_time(1.);
        let controller = &native.lines[1].object;
        let root = [
            f64::from(controller.translation.0.now()),
            f64::from(controller.translation.1.now()) / aspect,
        ];
        let (sn, cs) = f64::from(controller.rotation.now()).to_radians().sin_cos();
        let line = &mut native.lines[if mode == Mode::Lines { 2 } else { 1 }];
        line.object.set_time(1.);
        let locals: Vec<_> = if mode == Mode::Lines {
            vec![[
                f64::from(line.object.translation.0.now()),
                f64::from(line.object.translation.1.now()) / aspect,
            ]]
        } else {
            line.notes
                .iter()
                .map(|note| {
                    [
                        f64::from(note.object.translation.0.now()),
                        f64::from(note.object.translation.1.now()) / aspect,
                    ]
                })
                .collect()
        };
        // Native JudgeLineCache may reorder equal-time notes. Compare positions,
        // never source-array indices, against the preview's complete point set.
        assert_eq!(locals.len(), expected.len());
        let actuals: Vec<_> = locals
            .into_iter()
            .map(|local| {
                [
                    root[0] + local[0] * cs - local[1] * sn,
                    root[1] + local[0] * sn + local[1] * cs,
                ]
            })
            .collect();
        let matches = |a: &[f64; 2], p: &[f64; 2]| {
            (a[0] - p[0] / 675.).hypot(a[1] - p[1] / 450. / aspect) < 2e-6
        };
        assert!(
            actuals
                .iter()
                .all(|a| expected.iter().any(|p| matches(a, p))),
            "{mode:?}: native output has unexpected positions"
        );
        assert!(
            expected
                .iter()
                .all(|p| actuals.iter().any(|a| matches(a, p))),
            "{mode:?}: native output lost preview positions"
        );
        line.height.set_time(1.);
        assert_eq!(line.height.now(), 0.);
        if mode == Mode::Lines {
            line.object.alpha.set_time(0.);
            assert_eq!(line.object.alpha.now(), 0.);
            line.object.alpha.set_time(1.);
            assert_eq!(line.object.alpha.now(), 1.);
            line.object.alpha.set_time(2.);
            assert_eq!(line.object.alpha.now(), 0.);
            assert!(matches!(line.kind, prpr::core::JudgeLineKind::Normal));
        } else {
            assert_eq!(line.notes.len(), count);
            for note in &mut line.notes {
                assert!(note.fake);
                assert_eq!(note.speed, 1.);
                assert_eq!(note.height, 0.);
                note.object.alpha.set_time(0.);
                assert_eq!(note.object.alpha.now(), 0.);
                note.object.alpha.set_time(1.);
                assert_eq!(note.object.alpha.now(), 1.);
            }
        }
    }
}
