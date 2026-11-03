use phistudio_core::{ChartDocument, Editor};
use phistudio_plugins::*;
use phistudio_sdk::*;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};
#[path = "services/buffering.rs"]
mod buffering;
#[path = "services/interception.rs"]
mod interception;

type Handler = dyn Fn(&Request) -> Result<Response, String> + Send + Sync;
struct Extension {
    contributions: Vec<Contribution>,
    handler: Arc<Handler>,
}
impl Plugin for Extension {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        if matches!(request, Request::Activate { .. }) {
            return Ok(Response {
                contributions: self.contributions.clone(),
                ..Default::default()
            });
        }
        (self.handler)(request)
    }
}
fn builtin(
    id: &str,
    dependencies: &[&str],
    contributions: Vec<Contribution>,
    handler: impl Fn(&Request) -> Result<Response, String> + Send + Sync + 'static,
) -> Builtin {
    let mut manifest: Manifest = serde_json::from_value(json!({
        "api":1,"id":id,"name":id,"version":"1.0.0","backend":{"kind":"builtin"},
        "capabilities":["services","document_read","document_edit","hooks"]
    }))
    .unwrap();
    for dependency in dependencies {
        manifest
            .dependencies
            .insert((*dependency).into(), "^1".parse().unwrap());
    }
    let handler: Arc<Handler> = Arc::new(handler);
    let mut registered = vec![Contribution::Command {
        id: "run".into(),
        title: "Run".into(),
        shortcut: None,
        snapshot: false,
    }];
    registered.extend(contributions);
    Builtin {
        manifest,
        enabled: true,
        create: Arc::new(move || {
            Box::new(Extension {
                contributions: registered.clone(),
                handler: handler.clone(),
            })
        }),
    }
}
fn call(provider: &str, required: &str) -> ServiceCall {
    ServiceCall {
        request_id: "answer".into(),
        service: ServiceRef {
            provider: provider.into(),
            id: "values".into(),
            version: required.parse().unwrap(),
        },
        method: "echo".into(),
        arguments: json!({"unknown":[1,null,3]}),
    }
}
fn effects(effects: Vec<Effect>) -> Response {
    Response {
        effects,
        ..Default::default()
    }
}
fn command(plugin: &str) -> Action {
    Action::Command {
        plugin: plugin.into(),
        id: "run".into(),
        arguments: Value::Null,
        context: EditorContext {
            version: DocumentVersion {
                session: 42,
                revision: 0,
            },
            beat: 3.,
            ..Default::default()
        },
    }
}
fn wait(runtime: &Runtime, id: u64) -> Result<Output, String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(completion) = runtime.poll()? {
            assert_eq!(completion.id, id);
            return completion.result;
        }
        assert!(
            Instant::now() < deadline,
            "Worker did not finish request {id}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn services_route_targeted_results_validate_versions_and_do_not_broadcast_replies() {
    let replies = Arc::new(Mutex::new(vec![]));
    let reply_log = replies.clone();
    let consumer = builtin("client", &["provider"], vec![], move |r| {
        Ok(match r {
            Request::Command { arguments, .. } => effects(vec![Effect::Call {
                call: call("provider", arguments.as_str().unwrap_or("^1")),
            }]),
            Request::ServiceResult { reply, context } => {
                assert_eq!(
                    context.version,
                    DocumentVersion {
                        session: 42,
                        revision: 0
                    }
                );
                assert_eq!(context.beat, 3.);
                assert!(context.chart.is_none());
                assert_eq!(reply.request_id, "answer");
                reply_log.lock().unwrap().push(reply.outcome.clone());
                effects(vec![Effect::Notify {
                    message: "client received reply".into(),
                }])
            }
            _ => Response::default(),
        })
    });
    let provider = builtin(
        "provider",
        &[],
        vec![Contribution::Service {
            id: "values".into(),
            version: Version::new(1, 2, 0),
        }],
        |r| {
            Ok(match r {
                Request::Service { arguments, .. } => Response {
                    result: arguments.clone(),
                    effects: vec![Effect::Notify {
                        message: "provider ran".into(),
                    }],
                    ..Default::default()
                },
                _ => Response::default(),
            })
        },
    );
    let observer = builtin(
        "observer",
        &[],
        vec![Contribution::Event {
            topic: "host.service.reply".into(),
        }],
        |r| {
            assert!(!matches!(
                r,
                Request::Event { .. } | Request::ServiceResult { .. }
            ));
            Ok(Response::default())
        },
    );
    let mut runtime = Runtime::spawn(None, vec![consumer, provider, observer]).unwrap();
    wait(&runtime, 0).unwrap();
    let id = runtime.submit(command("client")).unwrap();
    let output = wait(&runtime, id).unwrap();
    assert!(output.calls.is_empty());
    assert_eq!(output.effects.len(), 2);
    assert_eq!(output.effects[0].origin.plugin, "provider");
    assert_eq!(output.effects[0].required[0].plugin, "client");
    assert_eq!(output.effects[1].origin.plugin, "client");
    assert_eq!(
        replies.lock().unwrap()[0],
        ServiceOutcome::Success {
            value: json!({"unknown":[1,null,3]})
        }
    );
    let Action::Command {
        plugin,
        id,
        context,
        ..
    } = command("client")
    else {
        panic!()
    };
    let id = runtime
        .submit(Action::Command {
            plugin,
            id,
            context,
            arguments: json!("^2"),
        })
        .unwrap();
    wait(&runtime, id).unwrap();
    assert!(
        matches!(&replies.lock().unwrap()[1],ServiceOutcome::Error {code,..} if code == "service_unavailable")
    );
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

#[test]
fn a_consumer_without_a_dependency_gets_an_error_without_invoking_the_provider() {
    let consumer = builtin("client", &[], vec![], |r| {
        Ok(match r {
            Request::Command { .. } => effects(vec![Effect::Call {
                call: call("provider", "^1"),
            }]),
            Request::ServiceResult { reply, .. } => {
                assert!(
                    matches!(&reply.outcome,ServiceOutcome::Error {code,..} if code == "service_unavailable")
                );
                effects(vec![Effect::Notify {
                    message: "access denied".into(),
                }])
            }
            _ => Response::default(),
        })
    });
    let provider = builtin(
        "provider",
        &[],
        vec![Contribution::Service {
            id: "values".into(),
            version: Version::new(1, 0, 0),
        }],
        |r| {
            assert!(!matches!(r, Request::Service { .. }));
            Ok(Response::default())
        },
    );
    let mut runtime = Runtime::spawn(None, vec![consumer, provider]).unwrap();
    wait(&runtime, 0).unwrap();
    let id = runtime.submit(command("client")).unwrap();
    assert_eq!(wait(&runtime, id).unwrap().effects.len(), 1);
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

#[test]
fn disabling_a_caller_revokes_provider_effects_already_queued_or_in_progress() {
    let (entered, entry) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let gate = Mutex::new(gate);
    let consumer = builtin("client", &["provider"], vec![], |r| {
        Ok(match r {
            Request::Command { .. } => effects(vec![Effect::Call {
                call: call("provider", "^1"),
            }]),
            _ => Response::default(),
        })
    });
    let provider = builtin(
        "provider",
        &[],
        vec![Contribution::Service {
            id: "values".into(),
            version: Version::new(1, 0, 0),
        }],
        move |r| {
            Ok(match r {
                Request::Service { context, .. } => {
                    entered.send(()).unwrap();
                    gate.lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                    effects(vec![Effect::Edit {
                        transaction: Transaction {
                            version: context.version,
                            label: "Service edit".into(),
                            operations: vec![PatchOperation::Replace {
                                path: "/META/name".into(),
                                value: json!("service result"),
                            }],
                        },
                    }])
                }
                _ => Response::default(),
            })
        },
    );
    let mut runtime = Runtime::spawn(None, vec![consumer, provider]).unwrap();
    wait(&runtime, 0).unwrap();
    let first = runtime.submit(command("client")).unwrap();
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    release.send(()).unwrap();
    let effect = wait(&runtime, first).unwrap().effects.remove(0);
    assert!(runtime.is_effect_current(&effect));
    let second = runtime.submit(command("client")).unwrap();
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    let disabled = runtime.submit(Action::Disable("client".into())).unwrap();
    assert!(!runtime.is_effect_current(&effect));
    let mut editor = Editor::new(ChartDocument::empty("Original", 120.).unwrap());
    let Effect::Edit { transaction } = effect.effect else {
        panic!()
    };
    assert!(
        runtime
            .apply_transaction(
                &effect.origin,
                &effect.required,
                42,
                &mut editor,
                transaction
            )
            .is_err()
    );
    release.send(()).unwrap();
    assert!(wait(&runtime, second).unwrap().effects.is_empty());
    wait(&runtime, disabled).unwrap();
    assert_eq!(editor.document().name(), "Original");
    let registry = runtime.registry().unwrap();
    assert_eq!(
        registry
            .plugins
            .iter()
            .find(|p| p.manifest.id == "provider")
            .unwrap()
            .status,
        Status::Enabled
    );
    runtime.shutdown().unwrap();
}

fn reader(read: bool) -> Builtin {
    let mut builtin = builtin("reader", &[], vec![], |r| {
        Ok(match r {
            Request::Command { context, .. } => {
                assert!(context.chart.is_none());
                effects(vec![Effect::Call {
                    call: ServiceCall {
                        request_id: "page".into(),
                        service: ServiceRef {
                            provider: "host".into(),
                            id: "document".into(),
                            version: "^1".parse().unwrap(),
                        },
                        method: "slice".into(),
                        arguments: json!(DocumentPage {
                            version: context.version,
                            path: "/judgeLineList/0/notes".into(),
                            offset: 0,
                            limit: 4096
                        }),
                    },
                }])
            }
            Request::ServiceResult { reply, .. } => effects(vec![Effect::Notify {
                message: serde_json::to_string(&reply.outcome).unwrap(),
            }]),
            _ => Response::default(),
        })
    });
    if !read {
        builtin
            .manifest
            .capabilities
            .remove(&Capability::DocumentRead);
    }
    builtin
}

#[test]
fn host_queries_check_grants_sessions_revisions_and_activation_before_reading() {
    for granted in [false, true] {
        let mut runtime = Runtime::spawn(None, vec![reader(granted)]).unwrap();
        wait(&runtime, 0).unwrap();
        let id = runtime.submit(command("reader")).unwrap();
        let call = wait(&runtime, id).unwrap().calls.remove(0);
        let document = ChartDocument::parse(include_str!(
            "../../../assets/charts/phistudio-demo.rpe.json"
        ))
        .unwrap();
        let version = DocumentVersion {
            session: 42,
            revision: 0,
        };
        let outcome = runtime.document_service(&call, version, &document);
        if granted {
            let ServiceOutcome::Success { value } = &outcome else {
                panic!("{outcome:?}")
            };
            assert_eq!(
                value["items"],
                document.as_json()["judgeLineList"][0]["notes"]
            );
            for stale in [
                DocumentVersion {
                    session: 43,
                    revision: 0,
                },
                DocumentVersion {
                    session: 42,
                    revision: 1,
                },
            ] {
                assert!(
                    matches!(runtime.document_service(&call,stale,&document),ServiceOutcome::Error {code,..} if code == "stale_document")
                );
            }
        } else {
            assert!(
                matches!(&outcome,ServiceOutcome::Error {code,..} if code == "permission_denied")
            );
        }
        let id = runtime
            .submit(Action::ServiceReply { call, outcome })
            .unwrap();
        assert_eq!(wait(&runtime, id).unwrap().effects.len(), 1);
        let id = runtime.submit(command("reader")).unwrap();
        let call = wait(&runtime, id).unwrap().calls.remove(0);
        let disabled = runtime.submit(Action::Disable("reader".into())).unwrap();
        assert!(!runtime.is_call_current(&call));
        assert!(
            matches!(runtime.document_service(&call,version,&document),ServiceOutcome::Error {code,..} if code == "caller_inactive")
        );
        assert!(
            runtime
                .submit(Action::ServiceReply {
                    call,
                    outcome: ServiceOutcome::Success { value: Value::Null }
                })
                .is_err()
        );
        wait(&runtime, disabled).unwrap();
        runtime.shutdown().unwrap();
    }
}

#[test]
fn service_chains_cannot_reenter_document_editing_from_a_hook() {
    let consumer = builtin(
        "client",
        &["provider"],
        vec![Contribution::Hook {
            point: "host.test".into(),
            priority: 0,
        }],
        |r| {
            Ok(match r {
                Request::Hook { .. } => effects(vec![Effect::Call {
                    call: call("provider", "^1"),
                }]),
                _ => Response::default(),
            })
        },
    );
    let provider = builtin(
        "provider",
        &[],
        vec![Contribution::Service {
            id: "values".into(),
            version: Version::new(1, 0, 0),
        }],
        |r| {
            Ok(match r {
                Request::Service { context, .. } => effects(vec![Effect::Edit {
                    transaction: Transaction {
                        version: context.version,
                        label: "Reentrant edit".into(),
                        operations: vec![],
                    },
                }]),
                _ => Response::default(),
            })
        },
    );
    let mut runtime = Runtime::spawn(None, vec![consumer, provider]).unwrap();
    wait(&runtime, 0).unwrap();
    let id = runtime
        .submit(Action::Hook {
            point: "host.test".into(),
            payload: Value::Null,
            context: EditorContext::default(),
        })
        .unwrap();
    assert!(wait(&runtime, id).err().unwrap().contains("not allowed"));
    runtime.shutdown().unwrap();
}

#[test]
fn dispatch_limits_survive_host_service_round_trips() {
    let consumer = builtin("client", &[], vec![], |r| {
        Ok(match r {
            Request::Command { .. } | Request::ServiceResult { .. } => {
                effects(vec![Effect::Call {
                    call: call("host", "^1"),
                }])
            }
            _ => Response::default(),
        })
    });
    let mut runtime = Runtime::spawn(None, vec![consumer]).unwrap();
    wait(&runtime, 0).unwrap();
    let mut id = runtime.submit(command("client")).unwrap();
    let mut rounds = 0;
    loop {
        match wait(&runtime, id) {
            Ok(mut output) => {
                let call = output.calls.remove(0);
                id = runtime
                    .submit(Action::ServiceReply {
                        call,
                        outcome: ServiceOutcome::error("test", "Keep going"),
                    })
                    .unwrap();
                rounds += 1;
                assert!(rounds <= 2048);
            }
            Err(error) => {
                assert!(error.contains("dispatch limit"));
                break;
            }
        }
    }
    assert!(rounds > 1);
    runtime.shutdown().unwrap();
}
