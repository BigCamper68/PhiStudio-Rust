use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use phistudio_core::{Command, Note, NoteId};

struct Peer(Arc<Mutex<Vec<Request>>>);
impl Plugin for Peer {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        self.0.lock().unwrap().push(request.clone());
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![
                    Contribution::Command {
                        id: "call".into(),
                        title: "Inspect context".into(),
                        shortcut: None,
                        snapshot: false,
                    },
                    Contribution::TimelineTrack {
                        id: "audio".into(),
                        title: "Embedded audio".into(),
                        width: 180.,
                    },
                    Contribution::ContextMenu {
                        id: "note".into(),
                        title: "Inspect this note".into(),
                        target: ContextTarget::Note,
                        command: "call".into(),
                    },
                    Contribution::ContextMenu {
                        id: "line".into(),
                        title: "Inspect this line".into(),
                        target: ContextTarget::Line,
                        command: "call".into(),
                    },
                ],
                ..Default::default()
            },
            Request::Command { arguments, .. } if arguments.get("service").is_some() => Response {
                effects: vec![Effect::Call {
                    call: serde_json::from_value(arguments.clone()).unwrap(),
                }],
                ..Default::default()
            },
            _ => Response::default(),
        })
    }
}
fn setup(audio: bool) -> (Workbench, Arc<Mutex<Vec<Request>>>) {
    let mut capabilities = vec!["ui", "timeline", "services"];
    if audio {
        capabilities.push("audio");
    }
    let manifest=serde_json::from_value(json!({"api":1,"id":"test.expansion","name":"Expansion","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":capabilities})).unwrap();
    let seen = Arc::new(Mutex::new(vec![]));
    let calls = seen.clone();
    let mut app = Workbench::new(ChartDocument::empty("Expansion", 120.).unwrap(), None);
    app.plugins.runtime = Some(
        Runtime::spawn(
            None,
            vec![Builtin {
                manifest,
                enabled: true,
                create: Arc::new(move || Box::new(Peer(calls.clone()))),
            }],
        )
        .unwrap(),
    );
    app.plugins.pending.insert(0, None);
    settle(&mut app);
    (app, seen)
}
fn audio_call(
    app: &mut Workbench,
    seen: &Arc<Mutex<Vec<Request>>>,
    method: &str,
    args: Value,
) -> ServiceOutcome {
    seen.lock().unwrap().clear();
    app.plugin_command(
        "test.expansion",
        "call",
        json!(ServiceCall {
            request_id: "audio".into(),
            service: ServiceRef {
                provider: "host".into(),
                id: "audio".into(),
                version: "^1".parse().unwrap()
            },
            method: method.into(),
            arguments: args
        }),
    );
    settle(app);
    seen.lock()
        .unwrap()
        .iter()
        .find_map(|r| match r {
            Request::ServiceResult { reply, .. } => Some(reply.outcome.clone()),
            _ => None,
        })
        .expect("Audio reply")
}
fn value(outcome: ServiceOutcome) -> Value {
    match outcome {
        ServiceOutcome::Success { value } => value,
        other => panic!("{other:?}"),
    }
}

#[test]
fn audio_reads_real_decoded_samples_without_chart_permission_and_rejects_stale_sources() {
    let (mut app, seen) = setup(true);
    let clip = app.audio_guide.seed_generation_test(
        app.session,
        app.asset_revision,
        app.project_options.music.as_deref(),
        &app.options.audio_guide,
    );
    let assets = ProjectAssetsVersion {
        session: app.session,
        revision: app.asset_revision,
    };
    let query = AudioQuery {
        version: app.plugin_version(),
        assets: Some(assets),
    };
    let info: AudioInfo =
        serde_json::from_value(value(audio_call(&mut app, &seen, "info", json!(query)))).unwrap();
    assert_eq!(info.frames, clip.frame_count() as u64);
    assert_eq!(info.channels, 2);
    let pcm = AudioPcmQuery {
        version: app.plugin_version(),
        source: info.source,
        offset: info.frames - 1,
        frames: MAX_PCM_FRAMES,
    };
    let page: AudioPcm =
        serde_json::from_value(value(audio_call(&mut app, &seen, "pcm", json!(pcm)))).unwrap();
    assert!(page.eof);
    assert_eq!(page.frames, 1);
    assert_eq!(STANDARD.decode(page.f32le_base64).unwrap(), vec![0; 8]);
    let wave = AudioWaveformQuery {
        version: app.plugin_version(),
        source: info.source,
        start: 0.,
        end: info.duration,
        buckets: 64,
    };
    assert_eq!(
        value(audio_call(&mut app, &seen, "waveform", json!(wave)))
            .as_array()
            .unwrap()
            .len(),
        64
    );
    audio_call(&mut app, &seen, "analyze", json!(query));
    assert!(app.audio_guide.plugin_requested);
    audio_call(&mut app, &seen, "status", json!(query));
    assert!(
        app.audio_guide.plugin_requested,
        "Status must preserve the analysis lease"
    );
    audio_call(&mut app, &seen, "cancel", json!(query));
    assert!(!app.audio_guide.plugin_requested);
    let mut stale = pcm;
    stale.source.serial += 1;
    assert!(matches!(
        audio_call(&mut app, &seen, "pcm", json!(stale)),
        ServiceOutcome::Error { .. }
    ));
    let mut oversized = wave;
    oversized.buckets = MAX_WAVEFORM_BUCKETS + 1;
    assert!(matches!(
        audio_call(&mut app, &seen, "waveform", json!(oversized)),
        ServiceOutcome::Error { .. }
    ));
    assert!(
        !app.plugins.registry.plugins[0]
            .granted
            .contains(&Capability::DocumentRead)
    );
    app.plugins.shutdown().unwrap();
    let (mut denied, seen) = setup(false);
    let query = AudioQuery {
        version: denied.plugin_version(),
        assets: Some(ProjectAssetsVersion {
            session: denied.session,
            revision: denied.asset_revision,
        }),
    };
    assert!(matches!(
        audio_call(&mut denied, &seen, "info", json!(query)),
        ServiceOutcome::Error { .. }
    ));
    denied.plugins.shutdown().unwrap();
}

#[test]
fn embedded_track_uses_native_timeline_clock_and_enable_reload_stay_closed() {
    let (mut app, seen) = setup(true);
    let ctx = egui::Context::default();
    let origin = app.plugins.registry.plugins[0].origin.clone().unwrap();
    assert!(app.plugins.rendering.tracks.is_empty());
    app.plugins.rendering.tracks.insert(
        (origin.plugin.clone(), "audio".into()),
        super::super::rendering::TrackWindow { origin, open: true },
    );
    for _ in 0..3 {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 600.),
                )),
                ..Default::default()
            },
            |ctx| app.ui(ctx, false),
        );
        settle(&mut app);
        app.pump_plugin_renders();
        settle(&mut app);
    }
    let frame = seen
        .lock()
        .unwrap()
        .iter()
        .find_map(|r| match r {
            Request::Render { frame, .. } if frame.instance == "embedded-track" => {
                Some(frame.clone())
            }
            _ => None,
        })
        .expect("Embedded render");
    let timeline = frame.viewport.timeline.unwrap();
    assert_eq!(timeline.beat, app.grid_beat());
    assert_eq!(timeline.pixels_per_beat, app.timeline.zoom);
    assert_eq!(timeline.baseline, frame.viewport.size[1] - 24.);
    assert!(app.timeline.area.right() < 1000.);
    app.plugin_action(Action::Reload);
    settle(&mut app);
    assert!(app.plugins.rendering.tracks.values().all(|t| !t.open));
    app.plugins.shutdown().unwrap();
}

#[test]
fn touch_context_command_receives_frozen_source_ids_and_document_version() {
    let (mut app, seen) = setup(true);
    app.editor
        .execute(Command::AddNote {
            line: 0,
            note: Note::default(),
        })
        .unwrap();
    app.select(Some(NoteId { line: 0, index: 0 }));
    let version = app.plugin_version();
    let ctx = egui::Context::default();
    let mut touch = crate::touch::TouchRouter::default();
    let mut frame = |events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 500.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    app.plugin_context_menu(ui, ContextTarget::Note);
                });
            },
        )
    };
    for label in ["Plugin actions…", "Inspect this note"] {
        for _ in 0..3 {
            frame(vec![]);
        }
        let output = frame(vec![]);
        let point = output
            .shapes
            .iter()
            .find_map(|s| match &s.1 {
                egui::Shape::Text(t) if t.galley.job.text == label => {
                    Some(t.pos + t.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .expect("Context control");
        for (phase, time) in [(egui::TouchPhase::Start, 0.1), (egui::TouchPhase::End, 0.2)] {
            frame(touch.event_with_context(&ctx, phase, 1, point, time).0);
        }
    }
    settle(&mut app);
    let args = seen
        .lock()
        .unwrap()
        .iter()
        .find_map(|r| match r {
            Request::Command { arguments, .. } => Some(arguments.clone()),
            _ => None,
        })
        .unwrap();
    let args: ContextArguments = serde_json::from_value(args).unwrap();
    assert_eq!(args.version, version);
    assert_eq!(args.notes[0].index, 0);
    assert_eq!(args.target, ContextTarget::Note);
    app.plugins.shutdown().unwrap();
}
