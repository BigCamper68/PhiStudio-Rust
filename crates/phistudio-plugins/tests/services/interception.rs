use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn hook(
    id: &str,
    priority: i32,
    handler: impl Fn(&Request) -> Result<Response, String> + Send + Sync + 'static,
) -> Builtin {
    builtin(
        id,
        &[],
        vec![Contribution::Hook {
            point: SERVICE_BEFORE_HOOK.into(),
            priority,
        }],
        handler,
    )
}
fn client(
    provider: &str,
    dependencies: &[&str],
    replies: Arc<Mutex<Vec<ServiceReply>>>,
) -> Builtin {
    let provider = provider.to_owned();
    builtin("client", dependencies, vec![], move |r| {
        Ok(match r {
            Request::Command { .. } => effects(vec![Effect::Call {
                call: call(&provider, "^1"),
            }]),
            Request::ServiceResult { reply, .. } => {
                replies.lock().unwrap().push(reply.clone());
                effects(vec![Effect::Notify {
                    message: "Consumer reply".into(),
                }])
            }
            _ => Response::default(),
        })
    })
}
fn provider(counter: Arc<AtomicUsize>) -> Builtin {
    builtin(
        "provider",
        &[],
        vec![Contribution::Service {
            id: "values".into(),
            version: Version::new(1, 0, 0),
        }],
        move |r| {
            Ok(match r {
                Request::Service { arguments, .. } => {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Response {
                        result: arguments.clone(),
                        effects: vec![Effect::Notify {
                            message: "Provider reply".into(),
                        }],
                        ..Default::default()
                    }
                }
                _ => Response::default(),
            })
        },
    )
}

#[test]
fn ordered_replacements_preserve_correlation_and_revoke_every_derived_effect() {
    let replies = Arc::new(Mutex::new(vec![]));
    let count = Arc::new(AtomicUsize::new(0));
    let mut builtins = vec![
        client("provider", &["provider"], replies.clone()),
        provider(count.clone()),
    ];
    for id in ["z", "a"] {
        builtins.push(hook(id, -10, move |r| {
            Ok(match r {
                Request::Hook {
                    payload, context, ..
                } => {
                    assert!(context.chart.is_none());
                    assert!(context.selection.is_null());
                    let mut p: ServiceIntercept = serde_json::from_value(payload.clone()).unwrap();
                    let old = p.call.arguments["order"].as_str().unwrap_or("");
                    p.call.arguments["order"] = json!(format!("{old}{id}"));
                    Response {
                        hook: Some(HookDecision::Replace { payload: json!(p) }),
                        effects: vec![Effect::Notify { message: id.into() }],
                        ..Default::default()
                    }
                }
                _ => Response::default(),
            })
        }));
    }
    let mut runtime = Runtime::spawn(None, builtins).unwrap();
    wait(&runtime, 0).unwrap();
    let mut action = command("client");
    if let Action::Command { context, .. } = &mut action {
        context.chart = Some(json!({"large":"source snapshot"}));
        context.selection = json!([1, 2, 3]);
    }
    let id = runtime.submit(action).unwrap();
    let output = wait(&runtime, id).unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let reply = &replies.lock().unwrap()[0];
    assert_eq!(reply.service.provider, "provider");
    assert_eq!(reply.request_id, "answer");
    assert!(matches!(&reply.outcome,ServiceOutcome::Success {value}
        if value["order"]=="az" && value["unknown"]==json!([1,null,3])));
    assert_eq!(output.effects.len(), 4);
    assert!(output.effects.iter().all(|e| runtime.is_effect_current(e)));
    let stop = runtime.submit(Action::Disable("a".into())).unwrap();
    assert!(output.effects.iter().all(|e| !runtime.is_effect_current(e)));
    wait(&runtime, stop).unwrap();
    runtime.shutdown().unwrap();
}

#[test]
fn cancellation_stops_the_operation_without_failing_the_consumer() {
    let replies = Arc::new(Mutex::new(vec![]));
    let count = Arc::new(AtomicUsize::new(0));
    let guard = hook("guard", 0, |r| {
        Ok(match r {
            Request::Hook { .. } => Response {
                hook: Some(HookDecision::Cancel {
                    reason: "Blocked by guard".into(),
                }),
                ..Default::default()
            },
            _ => Response::default(),
        })
    });
    let mut runtime = Runtime::spawn(
        None,
        vec![
            client("provider", &["provider"], replies.clone()),
            provider(count.clone()),
            guard,
        ],
    )
    .unwrap();
    wait(&runtime, 0).unwrap();
    let id = runtime.submit(command("client")).unwrap();
    wait(&runtime, id).unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert!(
        matches!(&replies.lock().unwrap()[0].outcome,ServiceOutcome::Error {code,..} if code=="hook_cancelled")
    );
    assert!(
        runtime
            .registry()
            .unwrap()
            .plugins
            .iter()
            .all(|p| p.status == Status::Enabled)
    );
    let stop = runtime.submit(Action::Disable("guard".into())).unwrap();
    wait(&runtime, stop).unwrap();
    let id = runtime.submit(command("client")).unwrap();
    wait(&runtime, id).unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    runtime.shutdown().unwrap();
}

#[test]
fn replacement_providers_use_the_replacing_hooks_permissions_and_dependencies() {
    for mode in 0..5 {
        let replies = Arc::new(Mutex::new(vec![]));
        let replacement = provider(Arc::default());
        let mut contributions = vec![Contribution::Hook {
            point: SERVICE_BEFORE_HOOK.into(),
            priority: 0,
        }];
        if mode != 3 {
            contributions.push(Contribution::Service {
                id: "values".into(),
                version: Version::new(1, 0, 0),
            });
        }
        let mut proxy = builtin(
            "proxy",
            if mode >= 2 { &["provider"] } else { &[] },
            contributions,
            move |r| {
                Ok(match r {
                    Request::Hook { payload, .. } => {
                        let mut p: ServiceIntercept =
                            serde_json::from_value(payload.clone()).unwrap();
                        p.call.service.provider =
                            if mode == 0 { "proxy" } else { "provider" }.into();
                        if mode == 4 {
                            p.call.service.version = "^2".parse().unwrap();
                        }
                        Response {
                            hook: Some(HookDecision::Replace { payload: json!(p) }),
                            ..Default::default()
                        }
                    }
                    Request::Service { .. } => Response {
                        result: json!({"proxy":true}),
                        ..Default::default()
                    },
                    _ => Response::default(),
                })
            },
        );
        if mode == 0 {
            proxy
                .manifest
                .capabilities
                .remove(&Capability::DocumentRead);
        }
        if mode == 3 {
            proxy.manifest.capabilities.remove(&Capability::Services);
        }
        let mut runtime = Runtime::spawn(
            None,
            vec![client("host", &[], replies.clone()), replacement, proxy],
        )
        .unwrap();
        wait(&runtime, 0).unwrap();
        let id = runtime.submit(command("client")).unwrap();
        let output = wait(&runtime, id).unwrap();
        assert!(output.calls.is_empty());
        let reply = &replies.lock().unwrap()[0];
        assert_eq!(reply.service.provider, "host");
        if matches!(mode, 1 | 3 | 4) {
            assert!(
                matches!(&reply.outcome,ServiceOutcome::Error {code,..} if code=="service_unavailable")
            );
        } else {
            assert!(matches!(reply.outcome, ServiceOutcome::Success { .. }));
        }
        runtime.shutdown().unwrap();
    }
}

#[test]
fn host_queries_keep_the_original_service_identity_and_hook_scope_across_round_trips() {
    let replies = Arc::new(Mutex::new(vec![]));
    let alias = hook("alias", 0, |r| {
        Ok(match r {
            Request::Hook {
                payload, context, ..
            } => {
                let mut p: ServiceIntercept = serde_json::from_value(payload.clone()).unwrap();
                p.call.service.id = "document".into();
                p.call.method = "read".into();
                p.call.arguments = json!(DocumentQuery {
                    version: context.version,
                    path: "/META/name".into()
                });
                Response {
                    hook: Some(HookDecision::Replace { payload: json!(p) }),
                    ..Default::default()
                }
            }
            _ => Response::default(),
        })
    });
    let mut runtime =
        Runtime::spawn(None, vec![client("host", &[], replies.clone()), alias]).unwrap();
    wait(&runtime, 0).unwrap();
    let document = ChartDocument::empty("Resolved by the original host", 120.).unwrap();
    let id = runtime.submit(command("client")).unwrap();
    let mut output = wait(&runtime, id).unwrap();
    let call = output.calls.pop().unwrap();
    assert_eq!(call.request.service.id, "document");
    let outcome = runtime.document_service(&call, call.context.version, &document);
    let id = runtime
        .submit(Action::ServiceReply { call, outcome })
        .unwrap();
    let output = wait(&runtime, id).unwrap();
    assert_eq!(replies.lock().unwrap()[0].service.id, "values");
    assert!(
        matches!(&replies.lock().unwrap()[0].outcome,ServiceOutcome::Success {value}
        if value["value"]=="Resolved by the original host")
    );
    let id = runtime.submit(command("client")).unwrap();
    let mut pending = wait(&runtime, id).unwrap();
    let call = pending.calls.pop().unwrap();
    let stop = runtime.submit(Action::Disable("alias".into())).unwrap();
    assert!(!runtime.is_call_current(&call));
    assert!(output.effects.iter().all(|e| !runtime.is_effect_current(e)));
    let outcome = runtime.document_service(&call, call.context.version, &document);
    assert!(matches!(outcome, ServiceOutcome::Error { .. }));
    assert!(
        runtime
            .submit(Action::ServiceReply { call, outcome })
            .is_err()
    );
    wait(&runtime, stop).unwrap();
    runtime.shutdown().unwrap();
}

fn edit(context: &EditorContext) -> Effect {
    Effect::Edit {
        transaction: Transaction {
            version: context.version,
            label: "Forbidden hook edit".into(),
            operations: vec![PatchOperation::Replace {
                path: "/META/name".into(),
                value: json!("Must not commit"),
            }],
        },
    }
}

#[test]
fn hook_side_effects_do_not_reenter_interceptors_or_gain_editing_through_a_service() {
    let seen = Arc::new(AtomicUsize::new(0));
    let counter = seen.clone();
    let mut guard = hook("guard", 0, move |r| {
        Ok(match r {
            Request::Hook { .. } => {
                counter.fetch_add(1, Ordering::SeqCst);
                effects(vec![Effect::Call {
                    call: call("mutator", "^1"),
                }])
            }
            _ => Response::default(),
        })
    });
    guard
        .manifest
        .dependencies
        .insert("mutator".into(), "^1".parse().unwrap());
    let mutator = builtin(
        "mutator",
        &[],
        vec![Contribution::Service {
            id: "values".into(),
            version: Version::new(1, 0, 0),
        }],
        |r| {
            Ok(match r {
                Request::Service { context, .. } => effects(vec![edit(context)]),
                _ => Response::default(),
            })
        },
    );
    let replies = Arc::new(Mutex::new(vec![]));
    let original = Arc::new(AtomicUsize::new(0));
    let mut runtime = Runtime::spawn(
        None,
        vec![
            client("provider", &["provider"], replies.clone()),
            provider(original.clone()),
            guard,
            mutator,
        ],
    )
    .unwrap();
    wait(&runtime, 0).unwrap();
    let id = runtime.submit(command("client")).unwrap();
    let output = wait(&runtime, id).unwrap();
    assert_eq!(seen.load(Ordering::SeqCst), 1);
    assert_eq!(original.load(Ordering::SeqCst), 0);
    assert!(
        output
            .effects
            .iter()
            .all(|e| !matches!(e.effect, Effect::Edit { .. }))
    );
    assert!(
        matches!(&replies.lock().unwrap()[0].outcome,ServiceOutcome::Error {code,..} if code=="hook_failed")
    );
    runtime.shutdown().unwrap();
}

#[test]
fn asynchronous_hook_queries_keep_readonly_permissions_after_returning_to_the_worker() {
    let guard = hook("guard", 0, |r| {
        Ok(match r {
            Request::Hook { context, .. } => effects(vec![Effect::Call {
                call: ServiceCall {
                    request_id: "hook-query".into(),
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
            }]),
            Request::ServiceResult { context, .. } => effects(vec![edit(context)]),
            _ => Response::default(),
        })
    });
    let mut runtime = Runtime::spawn(
        None,
        vec![
            client("provider", &["provider"], Arc::default()),
            provider(Arc::default()),
            guard,
        ],
    )
    .unwrap();
    wait(&runtime, 0).unwrap();
    let id = runtime.submit(command("client")).unwrap();
    let mut output = wait(&runtime, id).unwrap();
    let call = output.calls.pop().unwrap();
    assert_eq!(call.origin.plugin, "guard");
    let document = ChartDocument::empty("Read-only hook", 120.).unwrap();
    let outcome = runtime.document_service(&call, call.context.version, &document);
    let id = runtime
        .submit(Action::ServiceReply { call, outcome })
        .unwrap();
    assert!(matches!(wait(&runtime, id),Err(error) if error.contains("not allowed")));
    runtime.shutdown().unwrap();
}

#[test]
fn forged_caller_or_correlation_fails_the_hook_and_preserves_the_consumer() {
    for field in ["consumer", "request_id", "schema"] {
        let guard = hook("guard", 0, move |r| {
            Ok(match r {
                Request::Hook { payload, .. } => {
                    let mut p = payload.clone();
                    match field {
                        "consumer" => p["consumer"] = json!("another-plugin"),
                        "request_id" => p["call"]["request_id"] = json!("another-request"),
                        _ => p["unknown"] = json!(true),
                    }
                    Response {
                        hook: Some(HookDecision::Replace { payload: p }),
                        ..Default::default()
                    }
                }
                _ => Response::default(),
            })
        });
        let replies = Arc::new(Mutex::new(vec![]));
        let count = Arc::new(AtomicUsize::new(0));
        let mut runtime = Runtime::spawn(
            None,
            vec![
                client("provider", &["provider"], replies.clone()),
                provider(count.clone()),
                guard,
            ],
        )
        .unwrap();
        wait(&runtime, 0).unwrap();
        let id = runtime.submit(command("client")).unwrap();
        wait(&runtime, id).unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 0);
        let registry = runtime.registry().unwrap();
        assert!(matches!(
            registry
                .plugins
                .iter()
                .find(|p| p.manifest.id == "guard")
                .unwrap()
                .status,
            Status::Failed(_)
        ));
        assert_eq!(
            registry
                .plugins
                .iter()
                .find(|p| p.manifest.id == "client")
                .unwrap()
                .status,
            Status::Enabled
        );
        assert!(
            matches!(&replies.lock().unwrap()[0].outcome,ServiceOutcome::Error {code,..} if code=="hook_failed")
        );
        runtime.shutdown().unwrap();
    }
}

#[test]
fn hook_payloads_with_no_effects_still_retain_a_revocable_generation() {
    let guard = builtin(
        "guard",
        &[],
        vec![Contribution::Hook {
            point: "host.before-export".into(),
            priority: 0,
        }],
        |r| {
            Ok(match r {
                Request::Hook { .. } => Response {
                    hook: Some(HookDecision::Replace {
                        payload: json!({"flag":true}),
                    }),
                    ..Default::default()
                },
                Request::Command { .. } => Response {
                    result: json!({"command":true}),
                    ..Default::default()
                },
                _ => Response::default(),
            })
        },
    );
    let mut runtime = Runtime::spawn(None, vec![guard]).unwrap();
    wait(&runtime, 0).unwrap();
    let id = runtime
        .submit(Action::Hook {
            point: "host.before-export".into(),
            payload: json!({}),
            context: EditorContext::default(),
        })
        .unwrap();
    let output = wait(&runtime, id).unwrap();
    assert_eq!(output.result, json!({"flag":true}));
    assert!(runtime.is_output_current(&output));
    let id = runtime.submit(command("guard")).unwrap();
    let command_output = wait(&runtime, id).unwrap();
    assert_eq!(command_output.result, json!({"command":true}));
    assert!(runtime.is_output_current(&command_output));
    let stop = runtime.submit(Action::Disable("guard".into())).unwrap();
    assert!(!runtime.is_output_current(&output));
    assert!(!runtime.is_output_current(&command_output));
    wait(&runtime, stop).unwrap();
    runtime.shutdown().unwrap();
}

#[test]
fn individually_valid_hooks_cannot_accumulate_unbounded_effect_batches() {
    for large in [false, true] {
        let mut builtins = vec![];
        for id in ["a", "z"] {
            builtins.push(hook(id, 0, move |r| {
                Ok(match r {
                    Request::Hook { .. } => {
                        if large {
                            effects(vec![Effect::Emit {
                                topic: "bulk".into(),
                                payload: json!("x".repeat(MAX_MESSAGE_BYTES / 2 + 1024)),
                            }])
                        } else {
                            effects(vec![
                                Effect::Notify {
                                    message: "small".into()
                                };
                                MAX_EFFECTS
                            ])
                        }
                    }
                    _ => Response::default(),
                })
            }));
        }
        let mut runtime = Runtime::spawn(None, builtins).unwrap();
        wait(&runtime, 0).unwrap();
        let id = runtime
            .submit(Action::Hook {
                point: SERVICE_BEFORE_HOOK.into(),
                payload: json!(ServiceIntercept {
                    consumer: "client".into(),
                    call: call("host", "^1")
                }),
                context: EditorContext::default(),
            })
            .unwrap();
        let expected = if large {
            "effect byte limit"
        } else {
            "effect count limit"
        };
        assert!(matches!(wait(&runtime, id),Err(error) if error.contains(expected)));
        assert!(
            runtime
                .registry()
                .unwrap()
                .plugins
                .iter()
                .all(|p| p.status == Status::Enabled)
        );
        runtime.shutdown().unwrap();
    }
}

#[test]
fn disabling_a_hook_during_the_hook_or_its_provider_cancels_the_old_operation() {
    for block_hook in [false, true] {
        let (entered, entry) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let gate = Arc::new(Mutex::new(gate));
        let hook_gate = gate.clone();
        let hook_entered = entered.clone();
        let guard = hook("guard", 0, move |r| {
            Ok(match r {
                Request::Hook { .. } => {
                    if block_hook {
                        hook_entered.send(()).unwrap();
                        hook_gate
                            .lock()
                            .unwrap()
                            .recv_timeout(Duration::from_secs(10))
                            .unwrap();
                    }
                    Response::default()
                }
                _ => Response::default(),
            })
        });
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        let provider = builtin(
            "provider",
            &[],
            vec![Contribution::Service {
                id: "values".into(),
                version: Version::new(1, 0, 0),
            }],
            move |r| {
                Ok(match r {
                    Request::Service { .. } => {
                        calls.fetch_add(1, Ordering::SeqCst);
                        if !block_hook {
                            entered.send(()).unwrap();
                            gate.lock()
                                .unwrap()
                                .recv_timeout(Duration::from_secs(10))
                                .unwrap();
                        }
                        effects(vec![Effect::Notify {
                            message: "Old provider result".into(),
                        }])
                    }
                    _ => Response::default(),
                })
            },
        );
        let replies = Arc::new(Mutex::new(vec![]));
        let mut runtime = Runtime::spawn(
            None,
            vec![
                client("provider", &["provider"], replies.clone()),
                provider,
                guard,
            ],
        )
        .unwrap();
        wait(&runtime, 0).unwrap();
        let id = runtime.submit(command("client")).unwrap();
        entry.recv_timeout(Duration::from_secs(10)).unwrap();
        let stop = runtime.submit(Action::Disable("guard".into())).unwrap();
        release.send(()).unwrap();
        let output = wait(&runtime, id).unwrap();
        if block_hook {
            assert_eq!(count.load(Ordering::SeqCst), 0);
            assert!(
                matches!(&replies.lock().unwrap()[0].outcome,ServiceOutcome::Error {code,..} if code=="hook_failed")
            );
        } else {
            assert!(output.effects.is_empty());
            assert!(replies.lock().unwrap().is_empty());
        }
        wait(&runtime, stop).unwrap();
        runtime.shutdown().unwrap();
    }
}

#[test]
fn interception_can_run_during_activation_before_the_initial_registry_is_published() {
    struct BootClient(Arc<AtomicUsize>);
    impl Plugin for BootClient {
        fn call(&mut self, request: &Request) -> Result<Response, String> {
            Ok(match request {
                Request::Activate { .. } => effects(vec![Effect::Call {
                    call: call("host", "^1"),
                }]),
                Request::ServiceResult { reply, .. } => {
                    assert!(matches!(reply.outcome, ServiceOutcome::Success { .. }));
                    self.0.fetch_add(1, Ordering::SeqCst);
                    Response::default()
                }
                _ => Response::default(),
            })
        }
    }
    let count = Arc::new(AtomicUsize::new(0));
    let record = count.clone();
    let mut client = client("host", &[], Arc::default());
    client.create = Arc::new(move || Box::new(BootClient(record.clone())));
    let proxy = builtin(
        "proxy",
        &[],
        vec![
            Contribution::Hook {
                point: SERVICE_BEFORE_HOOK.into(),
                priority: 0,
            },
            Contribution::Service {
                id: "values".into(),
                version: Version::new(1, 0, 0),
            },
        ],
        |r| {
            Ok(match r {
                Request::Hook { payload, .. } => {
                    let mut p: ServiceIntercept = serde_json::from_value(payload.clone()).unwrap();
                    p.call.service.provider = "proxy".into();
                    Response {
                        hook: Some(HookDecision::Replace { payload: json!(p) }),
                        ..Default::default()
                    }
                }
                _ => Response::default(),
            })
        },
    );
    let mut runtime = Runtime::spawn(None, vec![client, proxy]).unwrap();
    wait(&runtime, 0).unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    runtime.shutdown().unwrap();
}
