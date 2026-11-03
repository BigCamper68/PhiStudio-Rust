use phistudio_core::ChartDocument;
use phistudio_plugins::*;
use phistudio_sdk::*;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

struct Gate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
struct Guard {
    gate: Option<Arc<Gate>>,
}
impl Plugin for Guard {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![Contribution::Hook {
                    point: FILE_BEFORE_HOOK.into(),
                    priority: 0,
                }],
                ..Default::default()
            },
            Request::Hook { context, .. } => {
                if let Some(gate) = &self.gate {
                    gate.entered.send(()).unwrap();
                    gate.release
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(10))
                        .unwrap();
                }
                Response {
                    effects: vec![
                        Effect::Notify {
                            message: "Hook accepted".into(),
                        },
                        Effect::Call {
                            call: ServiceCall {
                                request_id: "query".into(),
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
                        },
                    ],
                    hook: Some(HookDecision::Continue),
                    ..Default::default()
                }
            }
            Request::ServiceResult { .. } => Response {
                effects: vec![Effect::Notify {
                    message: "Query completed".into(),
                }],
                ..Default::default()
            },
            _ => Response::default(),
        })
    }
}
fn spawn(gate: Option<Arc<Gate>>) -> Runtime {
    let manifest=serde_json::from_value(json!({"api":1,"id":"test.file-guard","name":"File guard","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["hooks","services","document_read"]})).unwrap();
    let runtime = Runtime::spawn(
        None,
        vec![Builtin {
            manifest,
            enabled: true,
            create: Arc::new(move || Box::new(Guard { gate: gate.clone() })),
        }],
    )
    .unwrap();
    wait(&runtime, 0).unwrap();
    runtime
}
fn wait(runtime: &Runtime, id: u64) -> Result<Output, String> {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(completion) = runtime.poll()? {
            assert_eq!(completion.id, id);
            return completion.result;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn context() -> EditorContext {
    EditorContext {
        version: DocumentVersion {
            session: 2,
            revision: 3,
        },
        ..Default::default()
    }
}
fn action(invocation: Invocation) -> Action {
    Action::ScopedHook {
        invocation,
        point: FILE_BEFORE_HOOK.into(),
        payload: json!(FileIntercept {
            action: FileAction::Save,
            content: FileContent::Chart,
            version: context().version,
            name: Some("chart.json".into())
        }),
        context: context(),
    }
}

#[test]
fn scoped_hooks_are_single_use_and_cannot_cross_runtimes_or_document_versions() {
    let mut first = spawn(None);
    let mut second = spawn(None);
    let invocation = first.invocation(context().version);
    assert!(second.submit(action(invocation.clone())).is_err());
    let wrong = first.invocation(DocumentVersion::default());
    assert!(first.submit(action(wrong)).is_err());
    let cancelled = first.invocation(context().version);
    cancelled.cancel();
    assert!(first.submit(action(cancelled)).is_err());
    let id = first.submit(action(invocation.clone())).unwrap();
    let output = wait(&first, id).unwrap();
    assert!(first.is_output_current(&output));
    assert!(!second.is_output_current(&output));
    assert!(first.submit(action(invocation.clone())).is_err());
    invocation.cancel();
    assert!(!first.is_output_current(&output));
    assert!(!first.is_effect_current(&output.effects[0]));
    first.shutdown().unwrap();
    second.shutdown().unwrap();
}

#[test]
fn cancelling_a_running_hook_discards_its_completion_and_keeps_the_plugin_enabled() {
    let (entered, entry) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut runtime = spawn(Some(Arc::new(Gate {
        entered,
        release: Mutex::new(gate),
    })));
    let invocation = runtime.invocation(context().version);
    let id = runtime.submit(action(invocation.clone())).unwrap();
    entry.recv_timeout(Duration::from_secs(10)).unwrap();
    invocation.cancel();
    release.send(()).unwrap();
    assert!(wait(&runtime, id).is_err());
    assert_eq!(
        runtime.registry().unwrap().plugins[0].status,
        Status::Enabled
    );
    runtime.shutdown().unwrap();
}

#[test]
fn cancellation_follows_document_service_roundtrips_and_rejects_foreign_replies() {
    let document = ChartDocument::empty("Guarded document", 120.).unwrap();
    for scenario in ["complete", "cancel", "foreign"] {
        let mut runtime = spawn(None);
        let mut other = spawn(None);
        let invocation = runtime.invocation(context().version);
        let id = runtime.submit(action(invocation.clone())).unwrap();
        let mut output = wait(&runtime, id).unwrap();
        let call = output.calls.pop().unwrap();
        assert_eq!(call.invocation(), Some(&invocation));
        assert!(!other.is_call_current(&call));
        assert!(
            matches!(other.document_service(&call,context().version,&document),ServiceOutcome::Error {code,..} if code=="caller_inactive")
        );
        if scenario == "foreign" {
            assert!(
                other
                    .submit(Action::ServiceReply {
                        call,
                        outcome: ServiceOutcome::Success { value: Value::Null }
                    })
                    .is_err()
            );
        } else {
            if scenario == "cancel" {
                invocation.cancel();
            }
            let outcome = runtime.document_service(&call, context().version, &document);
            if scenario == "cancel" {
                assert!(
                    matches!(&outcome,ServiceOutcome::Error {code,..} if code=="caller_inactive")
                );
                assert!(!runtime.is_call_current(&call));
                assert!(
                    runtime
                        .submit(Action::ServiceReply { call, outcome })
                        .is_err()
                );
            } else {
                let id = runtime
                    .submit(Action::ServiceReply { call, outcome })
                    .unwrap();
                let output = wait(&runtime, id).unwrap();
                assert!(runtime.is_output_current(&output));
                assert!(
                    matches!(&output.effects[0].effect,Effect::Notify {message} if message=="Query completed")
                );
                invocation.cancel();
                assert!(!runtime.is_effect_current(&output.effects[0]));
            }
        }
        runtime.shutdown().unwrap();
        other.shutdown().unwrap();
    }
}

#[test]
fn queue_rejection_leaves_an_invocation_available_for_retry() {
    let (entered, entry) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut runtime = spawn(Some(Arc::new(Gate {
        entered,
        release: Mutex::new(gate),
    })));
    let first = runtime.invocation(context().version);
    let first_id = runtime.submit(action(first.clone())).unwrap();
    entry.recv_timeout(Duration::from_secs(10)).unwrap();
    let mut queued = vec![];
    for _ in 0..32 {
        queued.push(
            runtime
                .submit(Action::Event {
                    topic: "host.test".into(),
                    payload: Value::Null,
                    context: context(),
                })
                .unwrap(),
        );
    }
    let retry = runtime.invocation(context().version);
    assert!(runtime.submit(action(retry.clone())).is_err());
    first.cancel();
    release.send(()).unwrap();
    assert!(wait(&runtime, first_id).is_err());
    for id in queued {
        wait(&runtime, id).unwrap();
    }
    // Use an empty hook point so the gate does not block the retry.
    let id = runtime
        .submit(Action::ScopedHook {
            invocation: retry,
            point: "host.empty".into(),
            payload: Value::Null,
            context: context(),
        })
        .unwrap();
    wait(&runtime, id).unwrap();
    runtime.shutdown().unwrap();
}

#[test]
fn shutdown_revokes_scoped_results_even_when_no_hook_was_registered() {
    let mut runtime = spawn(None);
    let invocation = runtime.invocation(context().version);
    let id = runtime
        .submit(Action::ScopedHook {
            invocation: invocation.clone(),
            point: "host.empty".into(),
            payload: Value::Null,
            context: context(),
        })
        .unwrap();
    let output = wait(&runtime, id).unwrap();
    assert!(output.required.is_empty());
    assert!(runtime.is_output_current(&output));
    runtime.shutdown().unwrap();
    assert!(!runtime.is_output_current(&output));
    assert!(!runtime.is_invocation_current(&invocation, &[]));
}
