use super::*;
fn context() -> EditorContext {
    EditorContext {
        version: DocumentVersion {
            session: 7,
            revision: 0,
        },
        ..Default::default()
    }
}
fn chart() -> Value {
    let mut layer = json!({});
    for key in BASE {
        layer[key] =
            json!([{"startTime":[0,0,1],"endTime":[1,0,1],"start":0,"end":0,"easingType":0}]);
    }
    json!({"META":{"offset":0},"BPMList":[{"startTime":[0,0,1],"bpm":120}],"judgeLineList":[{"father":-1,"bpmfactor":1,"notes":[],"eventLayers":[layer]}]})
}
fn note(kind: u8, start: f64, end: f64, x: f64) -> Value {
    let fraction = |beat: f64| json!([0, (beat * 10000.).round() as i64, 10000]);
    json!({"type":kind,"startTime":fraction(start),"endTime":fraction(end),"positionX":x,"above":1,"isFake":0,"size":1,"speed":1,"alpha":255})
}
fn activate(plugin: &mut ChartLinter) {
    let manifest = serde_json::from_str(include_str!("../manifest.json")).unwrap();
    plugin
        .call(&Request::Activate {
            manifest,
            settings: BTreeMap::new(),
            storage: Value::Null,
        })
        .unwrap();
}
fn ui(plugin: &mut ChartLinter, widget: &str, value: Value) -> Response {
    plugin
        .call(&Request::Ui {
            event: UiEvent {
                panel: "report".into(),
                widget: widget.into(),
                value,
            },
            context: context(),
        })
        .unwrap()
}
fn host(call: &ServiceCall, source: &Value) -> ServiceOutcome {
    let version = context().version;
    if call.service.id == "project" {
        return ServiceOutcome::Success {
            value: json!(ProjectInfo {
                version,
                assets: ProjectAssetsVersion {
                    session: 7,
                    revision: 0
                },
                has_assets: false,
                chart_name: Some("chart.json".into()),
                music: None,
                illustration: None,
                music_duration: number(&source["_test_duration"]),
                music_offset: 0.,
                locale: "en".into(),
                plugins: vec![]
            }),
        };
    }
    if call.service.id == "preview" {
        let query: PreviewInspection = serde_json::from_value(call.arguments.clone()).unwrap();
        if source["_test_budget"] == true && query.offsets.len() > 8 {
            return ServiceOutcome::error(
                "inspection_rejected",
                "Native inspection sample budget exceeded; request fewer offsets",
            );
        }
        let points = query
            .offsets
            .iter()
            .map(|o| PreviewInspectionPoint {
                offset: *o,
                position: Some([0.5; 2]),
                inside: true,
                visible: if source["_test_gap"] == true {
                    !(-0.178..-0.1775).contains(o)
                } else {
                    *o >= -0.112
                },
                width: Some(1.),
            })
            .collect();
        return ServiceOutcome::Success {
            value: json!(PreviewInspectionResult {
                version,
                line: query.line,
                note: query.note,
                judgment_time: 1.,
                end_time: 1.,
                aspect_ratio: 1.5,
                points
            }),
        };
    }
    if call.service.id != "document" {
        return ServiceOutcome::error("unexpected_service", "Unexpected mock service");
    }
    let path = call.arguments["path"].as_str().unwrap();
    let Some(value) = source.pointer(path) else {
        return ServiceOutcome::error("path_not_found", "Missing path");
    };
    let value = match call.method.as_str() {
        "inspect" => {
            json!({"version":version,"kind":if value.is_array(){"array"}else{"object"},"length":value.as_array().map(Vec::len)})
        }
        "read" => json!(DocumentValue {
            version,
            value: value.clone()
        }),
        "validate" => json!(DocumentValidation {
            version,
            valid: true,
            message: None
        }),
        "fields" => {
            let mut projected = json!({});
            for key in call.arguments["fields"].as_array().unwrap() {
                let key = key.as_str().unwrap();
                if let Some(value) = value.get(key) {
                    projected[key] = value.clone()
                }
            }
            json!(DocumentValue {
                version,
                value: projected
            })
        }
        "rows" => {
            let offset = call.arguments["offset"].as_u64().unwrap() as usize;
            let values = value.as_array().unwrap();
            let items = values
                .iter()
                .skip(offset)
                .take(128)
                .map(|v| {
                    if !v.is_object() {
                        return v.clone();
                    }
                    let mut projected = json!({});
                    for key in call.arguments["fields"].as_array().unwrap() {
                        let key = key.as_str().unwrap();
                        if let Some(value) = v.get(key) {
                            projected[key] = value.clone()
                        }
                    }
                    projected
                })
                .collect::<Vec<_>>();
            let next = offset + items.len();
            json!(DocumentItems {
                version,
                offset,
                total: values.len(),
                next_offset: (next < values.len()).then_some(next),
                items
            })
        }
        _ => panic!("Unexpected method: {}", call.method),
    };
    ServiceOutcome::Success { value }
}
fn run(plugin: &mut ChartLinter, source: &Value, deep: bool) {
    let mut response = ui(plugin, if deep { "deep" } else { "scan" }, Value::Null);
    for _ in 0..20_000 {
        assert!(
            response
                .effects
                .iter()
                .all(|e| !matches!(e, Effect::Edit { .. })),
            "Scanning must never edit"
        );
        let call = response.effects.into_iter().find_map(|e| {
            if let Effect::Call { call } = e {
                Some(call)
            } else {
                None
            }
        });
        let Some(call) = call else {
            assert!(plugin.scan.is_none());
            return;
        };
        call.validate().unwrap();
        let outcome = host(&call, source);
        response = plugin
            .call(&Request::ServiceResult {
                reply: ServiceReply {
                    request_id: call.request_id,
                    service: call.service,
                    outcome,
                },
                context: context(),
            })
            .unwrap();
    }
    panic!("Scan did not finish");
}
fn codes(report: &Report) -> Vec<&str> {
    report.issues.iter().map(|i| i.code.as_str()).collect()
}

#[test]
fn complete_basic_scan_ignores_unknown_large_payload_and_keeps_source_unchanged() {
    let mut source = chart();
    source["judgeLineList"][0]["foreignPayload"] = json!("x".repeat(2 * 1024 * 1024));
    let original = source.clone();
    let mut plugin = ChartLinter::default();
    activate(&mut plugin);
    run(&mut plugin, &source, false);
    assert!(plugin.report.complete);
    assert_eq!(plugin.report.counts, [0, 0, 0]);
    assert_eq!(source, original);
}

#[test]
fn empty_foreign_metadata_and_default_skin_do_not_create_missing_resources() {
    let mut source = chart();
    source["noiseAreaLayers"] = json!([{"skin":""}]);
    source["globalEvents"] = json!({"types":{}});
    let mut plugin = ChartLinter::default();
    activate(&mut plugin);
    ui(&mut plugin, "target", json!("phira"));
    run(&mut plugin, &source, false);
    assert!(plugin.report.complete);
    assert_eq!(plugin.report.counts, [0, 0, 0]);
}
#[test]
fn paginated_notes_detect_hold_body_touching_boundary_and_flick_duplicates() {
    let mut source = chart();
    let mut notes = vec![
        note(2, 0., 4., 0.),
        note(1, 2., 2., 0.),
        note(1, 4., 4., 0.),
        note(3, 1., 1., 400.),
        note(3, 1., 1., 400.),
    ];
    for i in 0..260 {
        notes.push(note(4, f64::from(i), f64::from(i), 800.))
    }
    let mut fake = note(1, 2., 2., 0.);
    fake["isFake"] = json!(true);
    notes.push(fake);
    source["judgeLineList"][0]["notes"] = json!(notes);
    let mut plugin = ChartLinter::default();
    activate(&mut plugin);
    run(&mut plugin, &source, false);
    assert_eq!(plugin.report.checked_notes, 266);
    let paths: Vec<_> = plugin
        .report
        .issues
        .iter()
        .filter(|i| i.code == "note_overlap")
        .map(|i| i.path.as_str())
        .collect();
    assert_eq!(
        paths,
        [
            "/judgeLineList/0/notes/1",
            "/judgeLineList/0/notes/2",
            "/judgeLineList/0/notes/4"
        ]
    );
}
#[test]
fn constant_events_negative_alpha_and_equal_easing_slice_do_not_create_false_errors() {
    let mut source = chart();
    let layer = &mut source["judgeLineList"][0]["eventLayers"][0];
    layer["alphaEvents"] = json!([{"startTime":[-2,0,1],"endTime":[-1,0,1],"start":-1,"end":-1,"easingType":900},
        {"startTime":[0,0,1],"endTime":[1,0,1],"start":-1,"end":255,"easingType":1,"easingLeft":0.4,"easingRight":0.4},
        {"startTime":[1,0,1],"endTime":[2,0,1],"start":0,"end":1,"bezier":true,"easingType":999,"easingLeft":-5,"bezierPoints":[0,2,1,-1]}]);
    let mut plugin = ChartLinter::default();
    activate(&mut plugin);
    run(&mut plugin, &source, false);
    // The pre-zero constant establishes a baseline, but is wholly before music.
    assert_eq!(codes(&plugin.report), ["audio_range"]);
}
#[test]
fn event_overlap_excludes_touching_and_zero_duration_constants() {
    let clock = Clock::new(
        &[json!({"startTime":[0,0,1],"bpm":120})],
        &mut Report::default(),
    );
    let mut report = Report::default();
    let mut track = Track::default();
    for (index, (a, b)) in [(0, 2), (2, 3), (2, 2), (1, 4)].into_iter().enumerate() {
        let e = json!({"startTime":[a,0,1],"endTime":[b,0,1],"start":1,"end":1});
        inspect_event(
            &e,
            EventSite {
                path: &format!("/track/{index}"),
                key: "moveXEvents",
                layer: Some(0),
                index,
            },
            &clock,
            &Bounds::default(),
            &mut track,
            &mut report,
        )
    }
    finish_track(track, "/track", "moveXEvents", Some(0), false, &mut report);
    let paths: Vec<_> = report.issues.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, ["/track/3", "/track/1"]);
}
#[test]
fn clock_honors_tempo_changes_duplicate_last_wins_and_no_placeholder_audio() {
    let mut report = Report::default();
    let clock = Clock::new(
        &[
            json!({"startTime":[0,0,1],"bpm":120}),
            json!({"startTime":[4,0,1],"bpm":60}),
            json!({"startTime":[4,0,1],"bpm":240}),
        ],
        &mut report,
    );
    assert_eq!(clock.seconds(8.), Some(3.));
    assert_eq!(codes(&report), ["bpm_duplicate"]);
    let bounds = Bounds {
        offset: 0.1,
        duration: Some(3.),
    };
    assert!(bounds.outside(2., 3., false));
    assert!(!bounds.outside(-2., 4., true));
    assert!(!Bounds::default().outside(100000., 100000., false));
    assert!(
        !Bounds {
            offset: 0.1,
            duration: None
        }
        .outside(-0.11, 0., false)
    );
}
#[test]
fn parent_cycle_reports_descendants_and_preserves_unrelated_lines() {
    let mut report = Report::default();
    parents(&[Some(1), Some(0), Some(0), Some(-1)], &mut report);
    assert_eq!(report.counts, [3, 0, 0]);
    assert!(!report.issues.iter().any(|i| i.path.contains("/3/")));
}

#[test]
fn distance_control_uses_its_named_value_and_optional_easing() {
    let mut report = Report::default();
    let value = json!({"alphaControl":[{"x":0,"alpha":1}],"posControl":[{"x":100,"pos":0.5,"easing":1}],"sizeControl":[{"x":0,"size":0,"easing":1}],"yControl":[{"x":0,"y":1,"easing":1}]});
    line_values(&value, 0, &mut report);
    assert!(report.issues.is_empty());
    line_values(&json!({"alphaControl":[{"x":0,"value":1}]}), 0, &mut report);
    assert_eq!(codes(&report), ["control"]);
}

#[test]
fn native_shader_intervals_use_their_own_tempo_and_uniforms_keep_correct_types() {
    let mut source = chart();
    source["_test_duration"] = json!(1.5);
    source["phiStudioExtra"] = json!({"bpm":60,"effects":[{"start":[2,0,1],"end":[3,0,1],"shader":"grayscale","vars":{"axis":[0.5,0.2],"flag":true}}]});
    let mut plugin = ChartLinter::default();
    activate(&mut plugin);
    run(&mut plugin, &source, false);
    assert!(
        plugin
            .report
            .issues
            .iter()
            .any(|i| i.code == "audio_range" && i.path == "/phiStudioExtra/effects/0")
    );
    let mut report = Report::default();
    let clock = Clock::new(&[json!({"startTime":[0,0,1],"bpm":120})], &mut report);
    inspect_aux_interval(
        &json!({"startTime":[0,0,1],"endTime":[1,0,1],"start":[1.,2.],"end":[3.,4.]}),
        "/uniform/0",
        &clock,
        &Bounds::default(),
        &mut report,
    );
    assert!(report.issues.is_empty());
    inspect_aux_interval(
        &json!({"startTime":[0,0,0],"endTime":[1,0,1],"start":true,"end":false}),
        "/uniform/1",
        &clock,
        &Bounds::default(),
        &mut report,
    );
    assert_eq!(codes(&report), ["beat"]);
    assert!(!report.issues[0].removable);
}

#[test]
fn canceled_asset_open_is_closed_when_its_late_reply_arrives() {
    let mut plugin = ChartLinter::default();
    activate(&mut plugin);
    plugin.start(context().version, true);
    plugin.pending = Some((
        "scan1.call1".into(),
        Task::Open("config.json".into(), "/META".into()),
    ));
    ui(&mut plugin, "cancel", Value::Null);
    let response = plugin
        .call(&Request::ServiceResult {
            reply: ServiceReply {
                request_id: "scan1.call1".into(),
                service: ServiceRef {
                    provider: "host".into(),
                    id: "assets".into(),
                    version: VersionReq::STAR,
                },
                outcome: ServiceOutcome::Success {
                    value: json!(AssetFile {
                        id: 9,
                        source: AssetSource::Project,
                        path: "config.json".into(),
                        size: 3,
                        project: None
                    }),
                },
            },
            context: context(),
        })
        .unwrap();
    assert!(response.effects.iter().any(|e|matches!(e,Effect::Call{call} if call.service.id=="assets"&&call.method=="close"&&call.arguments["file"]==9)));
    assert!(plugin.scan.is_none());
}
#[test]
fn deep_scan_samples_native_preview_and_refines_readability() {
    let mut source = chart();
    source["judgeLineList"][0]["notes"] = json!([note(1, 2., 2., 0.)]);
    let mut plugin = ChartLinter::default();
    activate(&mut plugin);
    run(&mut plugin, &source, true);
    assert!(plugin.report.deep_complete);
    let issue = plugin
        .report
        .issues
        .iter()
        .find(|i| i.code == "short_read")
        .unwrap();
    assert!(issue.message.contains("112 ms"));
    assert!(issue.deep);
    assert!(!issue.removable);
}

#[test]
fn deep_scan_retries_smaller_native_batches_without_losing_samples() {
    let mut source = chart();
    source["_test_budget"] = json!(true);
    source["judgeLineList"][0]["notes"] = json!([note(1, 2., 2., 0.)]);
    let mut plugin = ChartLinter::default();
    activate(&mut plugin);
    run(&mut plugin, &source, true);
    assert!(plugin.report.deep_complete);
    assert!(!codes(&plugin.report).contains(&"native_inspection"));
    assert!(
        plugin
            .report
            .issues
            .iter()
            .any(|i| i.code == "short_read" && i.message.contains("112 ms"))
    );
}

#[test]
fn deep_scan_uses_event_boundaries_to_find_a_brief_non_monotonic_gap() {
    let mut source = chart();
    source["_test_gap"] = json!(true);
    source["judgeLineList"][0]["notes"] = json!([note(1, 2., 2., 0.)]);
    source["judgeLineList"][0]["eventLayers"][0]["alphaEvents"]
        .as_array_mut()
        .unwrap()
        .push(json!({"startTime":[0,16450,10000],"endTime":[0,16450,10000],"start":255,"end":255}));
    let mut plugin = ChartLinter::default();
    activate(&mut plugin);
    run(&mut plugin, &source, true);
    let issue = plugin
        .report
        .issues
        .iter()
        .find(|i| i.code == "short_read")
        .unwrap();
    let ms = issue
        .message
        .split_whitespace()
        .nth(3)
        .unwrap()
        .parse::<u32>()
        .unwrap();
    assert!((176..=179).contains(&ms), "{}", issue.message);
}

#[test]
fn retained_report_limits_do_not_change_totals_or_claim_a_completed_scan() {
    let mut report = Report::default();
    for _ in 0..10001 {
        report.push(Severity::Warning, "test", "", None, "A warning", false)
    }
    assert_eq!(report.counts, [0, 10001, 0]);
    assert_eq!(report.issues.len(), 10000);
    let mut plugin = ChartLinter::default();
    activate(&mut plugin);
    plugin.start(context().version, true);
    plugin.calls = 40000;
    plugin.advance();
    assert!(plugin.scan.is_none());
    assert!(!plugin.report.complete);
    assert!(plugin.message.contains("incomplete"));
}
#[test]
fn timing_uses_strict_x_boundary_inclusive_time_and_fake_filter() {
    let mut report = Report::default();
    let clock = Clock::new(&[json!({"startTime":[0,0,1],"bpm":120})], &mut report);
    let raw = [
        note(3, 0., 0., 0.),
        note(1, 0.08, 0.08, 199.9),
        note(1, 0.1, 0.1, 200.),
        note(4, 2., 2., 500.),
        note(1, 2.32, 2.32, 500.),
    ];
    let notes: Vec<_> = raw
        .iter()
        .enumerate()
        .filter_map(|(i, v)| inspect_note(v, 0, i, &clock, &Bounds::default(), &mut report))
        .collect();
    timing(&notes, &mut report);
    let result: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.deep)
        .map(|i| (&*i.code, i.severity))
        .collect();
    assert_eq!(
        result,
        [
            ("tap_after_flick", Severity::Error),
            ("tap_after_drag", Severity::Warning)
        ]
    );
}
#[test]
fn timing_has_contextual_hold_rules_and_scales_without_pair_lists() {
    let mut report = Report::default();
    let clock = Clock::new(&[json!({"startTime":[0,0,1],"bpm":120})], &mut report);
    let raw = [
        note(2, 0., 2., 0.),
        note(3, 2., 2., 0.),
        note(1, 2.1, 2.1, 0.),
    ];
    let notes: Vec<_> = raw
        .iter()
        .enumerate()
        .filter_map(|(i, v)| inspect_note(v, 0, i, &clock, &Bounds::default(), &mut report))
        .collect();
    timing(&notes, &mut report);
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.code == "tap_after_flick" && i.severity == Severity::Info)
    );
    let mut notes = Vec::with_capacity(20000);
    let template = inspect_note(&raw[0], 0, 0, &clock, &Bounds::default(), &mut report).unwrap();
    for index in 0..20000 {
        let mut n = template.clone();
        n.index = index;
        n.kind = if index % 2 == 0 { 3 } else { 1 };
        n.start = if n.kind == 3 { 0. } else { 0.1 };
        notes.push(n)
    }
    report = Report::default();
    timing(&notes, &mut report);
    assert_eq!(report.counts, [0, 0, 10000]);
}
#[test]
fn resources_resolve_relative_case_unique_basename_and_ambiguity_without_duplicate_issues() {
    let mut resources = Resources::default();
    resources.files = [
        "charts/a.png",
        "textures/UPPER.PNG",
        "a/same.png",
        "b/same.png",
    ]
    .map(String::from)
    .into();
    resources.add("a.png", "/line", "charts", false);
    resources.add("upper.png", "/line2", "", false);
    resources.add("same.png", "/line3", "", false);
    resources.add("missing.png", "/line4", "", false);
    let mut report = Report::default();
    resources.verify(&mut report);
    assert_eq!(report.counts, [2, 0, 0]);
    resources.verify(&mut report);
    assert_eq!(report.counts, [2, 0, 0]);
    assert_eq!(
        resources::normalize("a/../b\\c.png"),
        Some("b/c.png".into())
    );
    assert_eq!(resources::normalize("../../private"), None);
}
#[test]
fn indirect_resources_follow_json_and_shader_references_and_break_cycles() {
    let mut resources = Resources::default();
    let mut report = Report::default();
    resources.inspect_text(
        "skin/config.json",
        "/noise",
        br#"{"author":"missing.png","texture":"image.png","nested":{"file":"other.json"}}"#,
        &mut report,
    );
    resources.inspect_text(
        "skin/main.glsl",
        "/noise",
        b"#include \"common.glsl\"\n// @texture diffuse glow.png",
        &mut report,
    );
    assert_eq!(resources.references.len(), 4);
    assert!(resources.references.iter().all(|r| r.directory == "skin"));
    resources.inspect_text("bad.json", "/bad", b"{", &mut report);
    assert_eq!(codes(&report), ["resource_json"]);
}
#[test]
fn cancellation_and_stale_replies_cannot_resume_scanning_or_edit() {
    let mut plugin = ChartLinter::default();
    activate(&mut plugin);
    let first = ui(&mut plugin, "deep", Value::Null);
    let call = first
        .effects
        .into_iter()
        .find_map(|e| {
            if let Effect::Call { call } = e {
                Some(call)
            } else {
                None
            }
        })
        .unwrap();
    ui(&mut plugin, "cancel", Value::Null);
    let response = plugin
        .call(&Request::ServiceResult {
            reply: ServiceReply {
                request_id: call.request_id.clone(),
                service: call.service.clone(),
                outcome: host(&call, &chart()),
            },
            context: context(),
        })
        .unwrap();
    assert!(plugin.scan.is_none());
    assert!(
        response
            .effects
            .iter()
            .all(|e| !matches!(e, Effect::Call { .. } | Effect::Edit { .. }))
    );
}
#[test]
fn deletion_requires_explicit_confirmation_and_exact_document_version() {
    let mut source = chart();
    source["judgeLineList"][0]["notes"] = json!([note(2, 0., 0., 0.)]);
    let mut plugin = ChartLinter::default();
    activate(&mut plugin);
    run(&mut plugin, &source, false);
    let index = plugin
        .report
        .issues
        .iter()
        .position(|i| i.code == "hold_zero")
        .unwrap();
    let response = ui(&mut plugin, &format!("remove.r{index}"), Value::Null);
    assert!(
        response
            .effects
            .iter()
            .all(|e| !matches!(e, Effect::Edit { .. }))
    );
    let response = ui(&mut plugin, "remove_confirm", Value::Null);
    assert!(response.effects.iter().any(|e|matches!(e,Effect::Edit{transaction} if transaction.operations==vec![PatchOperation::Remove{path:"/judgeLineList/0/notes/0".into()}])));
    run(&mut plugin, &source, false);
    ui(&mut plugin, &format!("remove.r{index}"), Value::Null);
    let mut context = context();
    context.version.revision += 1;
    let response = plugin
        .call(&Request::Ui {
            event: UiEvent {
                panel: "report".into(),
                widget: "remove_confirm".into(),
                value: Value::Null,
            },
            context,
        })
        .unwrap();
    assert!(
        response
            .effects
            .iter()
            .all(|e| !matches!(e, Effect::Edit { .. }))
    );
}
