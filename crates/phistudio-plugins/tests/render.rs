use phistudio_plugins::*;
use phistudio_sdk::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

type Handler = dyn FnMut(&Request) -> Result<Response, String> + Send;
struct Renderer(Box<Handler>);
impl Plugin for Renderer {
    fn call(&mut self, r: &Request) -> Result<Response, String> {
        (self.0)(r)
    }
}
fn manifest() -> Manifest {
    serde_json::from_value(json!({"api":1,"id":"test.render","name":"Renderer","version":"1.0.0",
        "backend":{"kind":"builtin"},"capabilities":["preview","timeline","ui","document_read","document_edit","services"]})).unwrap()
}
fn contribution() -> Vec<Contribution> {
    vec![
        Contribution::Preview {
            id: "overlay".into(),
        },
        Contribution::Track {
            id: "clock".into(),
            title: "Clock".into(),
        },
        Contribution::Panel {
            id: "settings".into(),
            title: "Settings".into(),
            location: PanelLocation::Window,
            snapshot: false,
        },
        Contribution::Command {
            id: "run".into(),
            title: "Run".into(),
            shortcut: None,
            snapshot: false,
        },
    ]
}
fn frame(kind: RenderKind) -> RenderFrame {
    RenderFrame {
        token: 7,
        instance: "test-view".into(),
        target: RenderTarget {
            kind,
            id: if kind == RenderKind::Preview {
                "overlay"
            } else {
                "clock"
            }
            .into(),
        },
        viewport: Viewport {
            size: [320., 200.],
            timeline: (kind == RenderKind::Track).then_some(TimelineViewport {
                beat: 4.,
                pixels_per_beat: 96.,
                baseline: 176.,
                x_min: -675.,
                x_max: 675.,
                division: 4,
                lanes: 11,
            }),
        },
    }
}
fn marker() -> DrawPrimitive {
    DrawPrimitive::Circle {
        center: [0.5, 0.5],
        radius: 3.,
        color: DrawColor::Theme(ThemeColor::Accent),
    }
}
fn host(handler: impl FnMut(&Request) -> Result<Response, String> + Send + 'static) -> Host {
    let mut host = Host::default();
    let manifest = manifest();
    host.install(
        manifest.clone(),
        Box::new(Renderer(Box::new(handler))),
        manifest.capabilities,
    )
    .unwrap();
    host.enable("test.render").unwrap();
    host
}

#[test]
fn render_calls_own_their_targets_tokens_and_never_receive_chart_snapshots() {
    let calls = Arc::new(Mutex::new(0));
    let observed = calls.clone();
    let mut host = host(move |r| {
        Ok(match r {
            Request::Activate { .. } => Response {
                contributions: contribution(),
                ..Default::default()
            },
            Request::Render { frame, context } => {
                assert!(context.chart.is_none());
                assert!(context.selection.is_null());
                *observed.lock().unwrap() += 1;
                Response {
                    effects: vec![Effect::Draw {
                        frame: frame.token,
                        layers: vec![DrawLayer {
                            space: DrawSpace::Normalized,
                            primitives: vec![marker()],
                        }],
                    }],
                    ..Default::default()
                }
            }
            _ => Response::default(),
        })
    });
    for kind in [RenderKind::Preview, RenderKind::Track] {
        let result = host
            .render(
                "test.render",
                frame(kind),
                EditorContext {
                    chart: Some(json!({"secret":1})),
                    selection: json!({"notes":[{"line":0,"index":0}]}),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(result.effects[0].origin.plugin, "test.render");
        assert!(matches!(
            result.effects[0].effect,
            Effect::Draw { frame: 7, .. }
        ));
    }
    let mut foreign = frame(RenderKind::Preview);
    foreign.target.id = "unregistered".into();
    assert!(
        host.render("test.render", foreign, EditorContext::default())
            .is_err()
    );
    for invalid in [0., f32::NAN, 20_000.] {
        let mut frame = frame(RenderKind::Preview);
        frame.viewport.size[0] = invalid;
        assert!(
            host.render("test.render", frame, EditorContext::default())
                .is_err()
        );
    }
    assert_eq!(*calls.lock().unwrap(), 2);
    assert_eq!(host.status("test.render"), Some(&Status::Enabled));
}

#[test]
fn invalid_or_unsolicited_drawing_and_reentrant_edits_release_registrations() {
    for invalid in [
        "token",
        "space",
        "radius",
        "coordinate",
        "text",
        "duplicate",
        "command",
        "edit",
    ] {
        let mut host = host(move |r| {
            Ok(match r {
                Request::Activate { .. } => Response {
                    contributions: contribution(),
                    ..Default::default()
                },
                Request::Render { context, .. } | Request::Command { context, .. } => {
                    let mut layers = vec![DrawLayer {
                        space: if invalid == "space" {
                            DrawSpace::Timeline
                        } else {
                            DrawSpace::Normalized
                        },
                        primitives: vec![marker()],
                    }];
                    match invalid {
                        "radius" => {
                            layers[0].primitives = vec![DrawPrimitive::Circle {
                                center: [0., 0.],
                                radius: f32::INFINITY,
                                color: DrawColor::Rgba([255; 4]),
                            }]
                        }
                        "coordinate" => {
                            layers[0].primitives = vec![DrawPrimitive::Line {
                                from: [0., 0.],
                                to: [f64::INFINITY, 1.],
                                width: 1.,
                                color: DrawColor::Rgba([255; 4]),
                            }]
                        }
                        "text" => {
                            layers[0].primitives = vec![DrawPrimitive::Text {
                                position: [0., 0.],
                                text: "x".repeat(16_385),
                                size: 12.,
                                color: DrawColor::Rgba([255; 4]),
                            }]
                        }
                        _ => {}
                    }
                    let token = if let Request::Render { frame, .. } = r {
                        frame.token
                    } else {
                        7
                    };
                    let draw = Effect::Draw {
                        frame: if invalid == "token" { token + 1 } else { token },
                        layers,
                    };
                    let effects = if invalid == "edit" {
                        vec![Effect::Edit {
                            transaction: Transaction {
                                version: context.version,
                                label: "Illegal render edit".into(),
                                operations: vec![],
                            },
                        }]
                    } else if invalid == "duplicate" {
                        vec![draw.clone(), draw]
                    } else {
                        vec![draw]
                    };
                    Response {
                        effects,
                        ..Default::default()
                    }
                }
                _ => Response::default(),
            })
        });
        let result = if invalid == "command" {
            host.command("test.render", "run", Value::Null, EditorContext::default())
        } else {
            host.render(
                "test.render",
                frame(RenderKind::Preview),
                EditorContext::default(),
            )
        };
        assert!(result.is_err(), "{invalid}");
        assert!(host.contributions().next().is_none(), "{invalid}");
    }
}

#[test]
fn static_panel_canvases_validate_spaces_and_size_before_publication() {
    for (height, space, valid) in [
        (80., DrawSpace::Normalized, true),
        (80., DrawSpace::Timeline, false),
        (f32::NAN, DrawSpace::Points, false),
    ] {
        let manifest = manifest();
        let mut host = Host::default();
        host.install(
            manifest.clone(),
            Box::new(Renderer(Box::new(move |r| {
                Ok(match r {
                    Request::Activate { .. } => Response {
                        contributions: contribution(),
                        effects: vec![Effect::Panel {
                            id: "settings".into(),
                            widgets: vec![Widget::Canvas {
                                height,
                                layers: vec![DrawLayer {
                                    space,
                                    primitives: vec![marker()],
                                }],
                            }],
                        }],
                        ..Default::default()
                    },
                    _ => Response::default(),
                })
            }))),
            manifest.capabilities,
        )
        .unwrap();
        assert_eq!(host.enable("test.render").is_ok(), valid);
        assert_eq!(host.contributions().next().is_some(), valid);
    }
}

#[test]
fn render_service_round_trips_keep_background_priority_and_cannot_gain_editing() {
    use std::time::{Duration, Instant};
    fn wait(runtime: &Runtime, id: u64) -> Result<Output, String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(completion) = runtime.poll()? {
                assert_eq!(completion.id, id);
                return completion.result;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let manifest = manifest();
    let builtin = Builtin {
        manifest,
        enabled: true,
        create: Arc::new(|| {
            Box::new(Renderer(Box::new(|r| {
                Ok(match r {
                    Request::Activate { .. } => Response {
                        contributions: contribution(),
                        ..Default::default()
                    },
                    Request::Render { context, .. } => Response {
                        effects: vec![Effect::Call {
                            call: ServiceCall {
                                request_id: "read".into(),
                                service: ServiceRef {
                                    provider: "host".into(),
                                    id: "document".into(),
                                    version: "^1".parse().unwrap(),
                                },
                                method: "read".into(),
                                arguments: json!(DocumentQuery {
                                    version: context.version,
                                    path: "/META/name".into()
                                }),
                            },
                        }],
                        ..Default::default()
                    },
                    Request::ServiceResult { context, .. } => Response {
                        effects: vec![Effect::Edit {
                            transaction: Transaction {
                                version: context.version,
                                label: "Indirect render edit".into(),
                                operations: vec![],
                            },
                        }],
                        ..Default::default()
                    },
                    _ => Response::default(),
                })
            })))
        }),
    };
    let mut runtime = Runtime::spawn(None, vec![builtin]).unwrap();
    wait(&runtime, 0).unwrap();
    let context = EditorContext {
        version: DocumentVersion {
            session: 3,
            revision: 0,
        },
        ..Default::default()
    };
    let id = runtime
        .submit(Action::Render {
            plugin: "test.render".into(),
            frame: frame(RenderKind::Preview),
            context: context.clone(),
        })
        .unwrap();
    let call = wait(&runtime, id).unwrap().calls.remove(0);
    assert!(call.is_background());
    let document = phistudio_core::ChartDocument::empty("Read only rendering", 120.).unwrap();
    let outcome = runtime.document_service(&call, context.version, &document);
    assert!(matches!(outcome, ServiceOutcome::Success { .. }));
    let id = runtime
        .submit(Action::ServiceReply { call, outcome })
        .unwrap();
    assert!(wait(&runtime, id).err().unwrap().contains("not allowed"));
    runtime.shutdown().unwrap();
}
