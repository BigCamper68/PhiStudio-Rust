use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn a_branching_event_cascade_hits_live_queue_limits_before_running_thousands_of_callbacks() {
    for large in [false, true] {
        let seen = Arc::new(AtomicUsize::new(0));
        let counter = seen.clone();
        let emitter = builtin(
            "emitter",
            &[],
            vec![Contribution::Event {
                topic: "emitter.grow".into(),
            }],
            move |r| {
                Ok(match r {
                    Request::Command { .. } | Request::Event { .. } => {
                        counter.fetch_add(1, Ordering::SeqCst);
                        effects(vec![
                            Effect::Emit {
                                topic: "grow".into(),
                                payload: if large {
                                    json!("x".repeat(600_000))
                                } else {
                                    Value::Null
                                }
                            };
                            if large { 8 } else { 512 }
                        ])
                    }
                    _ => Response::default(),
                })
            },
        );
        let mut runtime = Runtime::spawn(None, vec![emitter]).unwrap();
        wait(&runtime, 0).unwrap();
        let id = runtime.submit(command("emitter")).unwrap();
        let expected = if large {
            "cascade queue exceeds its byte limit"
        } else {
            "cascade queue exceeds its item limit"
        };
        assert!(matches!(wait(&runtime, id),Err(error) if error.contains(expected)));
        assert!(seen.load(Ordering::SeqCst) < 16);
        runtime.shutdown().unwrap();
    }
}

#[test]
fn completion_limits_drop_the_entire_unpublished_edit_batch() {
    let producer = builtin(
        "producer",
        &[],
        vec![Contribution::Event {
            topic: "producer.burst".into(),
        }],
        |r| {
            Ok(match r {
                Request::Command { context, .. } => effects(vec![
                    Effect::Edit {
                        transaction: Transaction {
                            version: context.version,
                            label: "Unpublished edit".into(),
                            operations: vec![PatchOperation::Replace {
                                path: "/META/name".into(),
                                value: json!("Must not apply"),
                            }],
                        },
                    },
                    Effect::Emit {
                        topic: "burst".into(),
                        payload: Value::Null,
                    },
                ]),
                Request::Event { .. } => effects(vec![
                    Effect::Notify {
                        message: "Burst".into()
                    };
                    MAX_EFFECTS
                ]),
                _ => Response::default(),
            })
        },
    );
    let mut runtime = Runtime::spawn(None, vec![producer]).unwrap();
    wait(&runtime, 0).unwrap();
    let id = runtime.submit(command("producer")).unwrap();
    assert!(
        matches!(wait(&runtime, id),Err(error) if error.contains("completion exceeds its item limit"))
    );
    runtime.shutdown().unwrap();
}

#[test]
fn completions_bound_retained_bytes_even_when_each_callback_and_live_queue_are_small_enough() {
    let mut producer = builtin(
        "producer",
        &[],
        vec![
            Contribution::Service {
                id: "values".into(),
                version: Version::new(1, 0, 0),
            },
            Contribution::Panel {
                id: "large".into(),
                title: "Large".into(),
                location: PanelLocation::Window,
                snapshot: false,
            },
        ],
        |r| {
            Ok(match r {
                Request::Command { .. } => effects(vec![
                    Effect::Call {
                        call: call("producer", "^1")
                    };
                    2
                ]),
                Request::Service { .. } => effects(vec![Effect::Panel {
                    id: "large".into(),
                    widgets: vec![Widget::Label {
                        text: "x".repeat(MAX_MESSAGE_BYTES / 2 + 1024),
                    }],
                }]),
                _ => Response::default(),
            })
        },
    );
    producer.manifest.capabilities.insert(Capability::Ui);
    let mut runtime = Runtime::spawn(None, vec![producer]).unwrap();
    wait(&runtime, 0).unwrap();
    let id = runtime.submit(command("producer")).unwrap();
    assert!(
        matches!(wait(&runtime, id),Err(error) if error.contains("completion exceeds its byte limit"))
    );
    runtime.shutdown().unwrap();
}

#[test]
fn aggregate_event_fanout_stops_before_returning_an_unbounded_effect_list() {
    let mut host = Host::default();
    for id in ["a", "z"] {
        let plugin = builtin(
            id,
            &[],
            vec![Contribution::Event {
                topic: "host.burst".into(),
            }],
            |r| {
                Ok(match r {
                    Request::Event { .. } => effects(vec![
                        Effect::Notify {
                            message: "Bounded".into()
                        };
                        MAX_EFFECTS
                    ]),
                    _ => Response::default(),
                })
            },
        );
        let grants = plugin.manifest.capabilities.clone();
        host.install(plugin.manifest, (plugin.create)(), grants)
            .unwrap();
        host.enable(id).unwrap();
    }
    let (effects, errors) = host.event("host.burst", Value::Null, EditorContext::default());
    assert_eq!(effects.len(), MAX_EFFECTS);
    assert!(errors[0].message.contains("aggregate effect limit"));
    assert_eq!(host.status("z"), Some(&Status::Enabled));
}
