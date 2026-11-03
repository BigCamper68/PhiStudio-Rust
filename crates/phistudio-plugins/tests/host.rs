use phistudio_plugins::*;
use phistudio_sdk::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

fn manifest(id: &str) -> Manifest {
    serde_json::from_value(
        json!({"api":1,"id":id,"name":id,"version":"1.0.0","backend":{"kind":"builtin"},
        "capabilities":["document_read","document_edit","hooks","ui","services"]}),
    )
    .unwrap()
}
type Callback = dyn FnMut(&Request) -> Result<Response, String> + Send;
struct TestPlugin(Box<Callback>);
impl Plugin for TestPlugin {
    fn call(&mut self, r: &Request) -> Result<Response, String> {
        (self.0)(r)
    }
}
fn plugin(f: impl FnMut(&Request) -> Result<Response, String> + Send + 'static) -> Box<dyn Plugin> {
    Box::new(TestPlugin(Box::new(f)))
}
fn install(host: &mut Host, m: Manifest, p: Box<dyn Plugin>) {
    let grants = m.capabilities.clone();
    host.install(m, p, grants).unwrap();
}
fn command() -> Contribution {
    Contribution::Command {
        id: "run".into(),
        title: "Run".into(),
        shortcut: None,
        snapshot: false,
    }
}

#[test]
fn menu_entries_only_reference_owned_window_panels() {
    for (target, location, allowed) in [
        ("view", PanelLocation::Window, true),
        ("missing", PanelLocation::Window, false),
        ("view", PanelLocation::Sidebar, false),
    ] {
        let mut host = Host::default();
        install(
            &mut host,
            manifest("example.menu"),
            plugin(move |r| {
                Ok(if matches!(r, Request::Activate { .. }) {
                    Response {
                        contributions: vec![
                            Contribution::Menu {
                                id: "open".into(),
                                title: "Open view".into(),
                                location: EditorMenu::File,
                                panel: target.into(),
                            },
                            Contribution::Panel {
                                id: "view".into(),
                                title: "View".into(),
                                location,
                                snapshot: false,
                            },
                        ],
                        ..Default::default()
                    }
                } else {
                    Response::default()
                })
            }),
        );
        assert_eq!(host.enable("example.menu").is_ok(), allowed);
        if allowed {
            host.disable("example.menu");
            assert_eq!(host.contributions().count(), 0)
        }
    }
}

#[test]
fn dependencies_order_activation_conflicts_versions_and_cascading_cleanup() {
    let log = Arc::new(Mutex::new(vec![]));
    let mut host = Host::default();
    for id in ["example.child", "example.base"] {
        let mut m = manifest(id);
        if id.ends_with("child") {
            m.dependencies
                .insert("example.base".into(), "^1".parse().unwrap());
        }
        let log = log.clone();
        let name = id.to_owned();
        install(
            &mut host,
            m,
            plugin(move |r| {
                let event = if matches!(r, Request::Activate { .. }) {
                    "start"
                } else {
                    "stop"
                };
                log.lock().unwrap().push(format!("{name}:{event}"));
                Ok(Response {
                    contributions: if event == "start" {
                        vec![command()]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                })
            }),
        );
    }
    host.enable("example.child").unwrap();
    assert_eq!(
        *log.lock().unwrap(),
        ["example.base:start", "example.child:start"]
    );
    assert_eq!(host.contributions().count(), 2);
    let mut conflict = manifest("example.conflict");
    conflict.conflicts.insert("example.base".into());
    install(&mut host, conflict, plugin(|_| Ok(Response::default())));
    assert!(host.enable("example.conflict").is_err());
    assert_eq!(host.status("example.conflict"), Some(&Status::Disabled));
    assert_eq!(
        host.disable("example.base"),
        ["example.child", "example.base"]
    );
    assert_eq!(
        &log.lock().unwrap()[2..],
        ["example.child:stop", "example.base:stop"]
    );
    assert_eq!(host.contributions().count(), 0);
    let mut incompatible = manifest("example.incompatible");
    incompatible
        .dependencies
        .insert("example.base".into(), "^2".parse().unwrap());
    install(&mut host, incompatible, plugin(|_| Ok(Response::default())));
    assert!(host.enable("example.incompatible").is_err());
}

#[test]
fn cycles_missing_dependencies_and_failed_activation_do_not_leave_partial_registrations() {
    let mut host = Host::default();
    for (id, dependency) in [("a", "b"), ("b", "a"), ("c", "missing")] {
        let mut m = manifest(id);
        m.dependencies
            .insert(dependency.into(), "*".parse().unwrap());
        install(&mut host, m, plugin(|_| Ok(Response::default())));
    }
    assert!(host.enable("a").unwrap_err().message.contains("cycle"));
    assert!(host.enable("c").unwrap_err().message.contains("Missing"));
    install(
        &mut host,
        manifest("base"),
        plugin(|r| {
            Ok(Response {
                contributions: if matches!(r, Request::Activate { .. }) {
                    vec![command()]
                } else {
                    vec![]
                },
                ..Default::default()
            })
        }),
    );
    let mut broken = manifest("broken");
    broken
        .dependencies
        .insert("base".into(), "*".parse().unwrap());
    install(&mut host, broken, plugin(|_| Err("Cannot activate".into())));
    assert!(host.enable("broken").is_err());
    assert_eq!(host.status("base"), Some(&Status::Disabled));
    assert!(matches!(host.status("broken"), Some(Status::Failed(_))));
    assert_eq!(host.contributions().count(), 0);
}

#[test]
fn undeclared_access_and_foreign_panel_effects_fail_atomically() {
    let mut host = Host::default();
    host.install(
        manifest("denied"),
        plugin(|_| {
            Ok(Response {
                contributions: vec![Contribution::Panel {
                    id: "panel".into(),
                    title: "Panel".into(),
                    location: PanelLocation::Window,
                    snapshot: false,
                }],
                ..Default::default()
            })
        }),
        BTreeSet::new(),
    )
    .unwrap();
    assert!(host.enable("denied").is_err());
    install(
        &mut host,
        manifest("owner"),
        plugin(|r| {
            Ok(if matches!(r, Request::Activate { .. }) {
                Response {
                    contributions: vec![command()],
                    ..Default::default()
                }
            } else {
                Response {
                    effects: vec![
                        Effect::Notify {
                            message: "Must not escape".into(),
                        },
                        Effect::Panel {
                            id: "someone.else".into(),
                            widgets: vec![],
                        },
                    ],
                    ..Default::default()
                }
            })
        }),
    );
    host.enable("owner").unwrap();
    assert!(
        host.command("owner", "run", Value::Null, EditorContext::default())
            .is_err()
    );
    assert!(matches!(host.status("owner"), Some(Status::Failed(_))));
    assert_eq!(host.contributions().count(), 0);
}

#[test]
fn disabled_generation_cannot_apply_queued_edits_and_history_survives_uninstall() {
    let mut host = Host::default();
    install(
        &mut host,
        manifest("edit"),
        plugin(|r| {
            Ok(match r {
                Request::Activate { .. } => Response {
                    contributions: vec![command()],
                    ..Default::default()
                },
                Request::Command { context, .. } => Response {
                    effects: vec![Effect::Edit {
                        transaction: Transaction {
                            version: context.version,
                            label: "Plugin rename".into(),
                            operations: vec![PatchOperation::Replace {
                                path: "/META/name".into(),
                                value: json!("From plugin"),
                            }],
                        },
                    }],
                    ..Default::default()
                },
                _ => Response::default(),
            })
        }),
    );
    let mut editor = phistudio_core::Editor::new(
        phistudio_core::ChartDocument::empty("Original", 120.).unwrap(),
    );
    let context = EditorContext {
        version: DocumentVersion {
            session: 7,
            revision: 0,
        },
        ..Default::default()
    };
    host.enable("edit").unwrap();
    let result = host
        .command("edit", "run", Value::Null, context.clone())
        .unwrap();
    let effect = result.effects[0].clone();
    let Effect::Edit { transaction } = effect.effect else {
        panic!()
    };
    assert!(
        host.apply_transaction(&effect.origin, 8, &mut editor, transaction.clone())
            .is_err()
    );
    host.disable("edit");
    host.enable("edit").unwrap();
    assert!(
        host.apply_transaction(&effect.origin, 7, &mut editor, transaction.clone())
            .is_err()
    );
    let fresh = host.command("edit", "run", Value::Null, context).unwrap();
    host.apply_transaction(&fresh.effects[0].origin, 7, &mut editor, transaction)
        .unwrap();
    assert_eq!(editor.document().name(), "From plugin");
    host.uninstall("edit");
    assert!(editor.undo());
    assert_eq!(editor.document().name(), "Original");
    assert!(editor.redo());
    assert_eq!(editor.document().name(), "From plugin");
}

#[test]
fn hooks_replace_in_order_cancel_and_cannot_reenter_document_editing() {
    let mut host = Host::default();
    for (id, priority, decision) in [
        ("z", -1, HookDecision::Replace { payload: json!(2) }),
        (
            "a",
            0,
            HookDecision::Cancel {
                reason: "Stop".into(),
            },
        ),
    ] {
        install(
            &mut host,
            manifest(id),
            plugin(move |r| {
                Ok(match r {
                    Request::Activate { .. } => Response {
                        contributions: vec![Contribution::Hook {
                            point: "host.before-commit".into(),
                            priority,
                        }],
                        ..Default::default()
                    },
                    Request::Hook { payload, .. } => {
                        if id == "a" {
                            assert_eq!(payload, &json!(2));
                        }
                        Response {
                            hook: Some(decision.clone()),
                            ..Default::default()
                        }
                    }
                    _ => Response::default(),
                })
            }),
        );
    }
    host.enable("a").unwrap();
    host.enable("z").unwrap();
    let result = host
        .hook("host.before-commit", json!(1), EditorContext::default())
        .unwrap();
    assert_eq!(result.payload, json!(2));
    assert_eq!(result.cancelled, Some("Stop".into()));
}

#[test]
fn read_grants_redact_snapshots_and_failed_listeners_do_not_suppress_others() {
    let observed = Arc::new(Mutex::new(false));
    let mut host = Host::default();
    for id in ["broken", "reader"] {
        let observed = observed.clone();
        let p = plugin(move |r| {
            Ok(match r {
                Request::Activate { .. } => Response {
                    contributions: vec![Contribution::Event {
                        topic: "host.changed".into(),
                    }],
                    ..Default::default()
                },
                Request::Event { context, .. } => {
                    if id == "broken" {
                        return Err("Bad callback".into());
                    }
                    assert!(context.chart.is_none());
                    *observed.lock().unwrap() = true;
                    Response::default()
                }
                _ => Response::default(),
            })
        });
        host.install(manifest(id), p, BTreeSet::new()).unwrap();
        host.enable(id).unwrap();
    }
    let (effects, errors) = host.event(
        "host.changed",
        json!({}),
        EditorContext {
            chart: Some(json!({"secret":1})),
            ..Default::default()
        },
    );
    assert!(effects.is_empty());
    assert_eq!(errors.len(), 1);
    assert!(*observed.lock().unwrap());
}

#[test]
fn settings_are_validated_and_service_access_requires_dependency_and_version() {
    let mut host = Host::default();
    install(
        &mut host,
        manifest("provider"),
        plugin(|r| {
            Ok(match r {
                Request::Activate { .. } => Response {
                    contributions: vec![Contribution::Service {
                        id: "analysis".into(),
                        version: "1.2.0".parse().unwrap(),
                    }],
                    ..Default::default()
                },
                Request::Service { arguments, .. } => Response {
                    result: arguments.clone(),
                    ..Default::default()
                },
                _ => Response::default(),
            })
        }),
    );
    let mut consumer = manifest("consumer");
    consumer
        .dependencies
        .insert("provider".into(), "^1".parse().unwrap());
    consumer.settings.insert(
        "count".into(),
        Setting::Number {
            title: "Count".into(),
            default: 1.,
            min: 0.,
            max: 5.,
        },
    );
    install(&mut host, consumer, plugin(|_| Ok(Response::default())));
    host.enable("consumer").unwrap();
    host.set_setting("consumer", "count", json!(3)).unwrap();
    assert!(host.set_setting("consumer", "count", json!(6)).is_err());
    assert_eq!(host.settings("consumer").unwrap()["count"], json!(3));
    let required = "^1".parse().unwrap();
    assert_eq!(
        host.service(
            "consumer",
            "provider",
            &required,
            Request::Service {
                id: "analysis".into(),
                method: "run".into(),
                arguments: json!([1, 2]),
                context: EditorContext::default()
            }
        )
        .unwrap()
        .result,
        json!([1, 2])
    );
    assert!(
        host.service(
            "consumer",
            "provider",
            &"^2".parse().unwrap(),
            Request::Service {
                id: "analysis".into(),
                method: "run".into(),
                arguments: Value::Null,
                context: EditorContext::default()
            }
        )
        .is_err()
    );
    install(
        &mut host,
        manifest("stranger"),
        plugin(|_| Ok(Response::default())),
    );
    host.enable("stranger").unwrap();
    assert!(
        host.service(
            "stranger",
            "provider",
            &required,
            Request::Service {
                id: "analysis".into(),
                method: "run".into(),
                arguments: Value::Null,
                context: EditorContext::default()
            }
        )
        .is_err()
    );
}

#[test]
fn panics_and_deactivation_errors_still_release_every_registration() {
    let mut host = Host::default();
    install(
        &mut host,
        manifest("panic"),
        plugin(|r| match r {
            Request::Activate { .. } => Ok(Response {
                contributions: vec![command()],
                ..Default::default()
            }),
            Request::Deactivate => Err("Broken cleanup".into()),
            _ => panic!("Broken callback"),
        }),
    );
    host.enable("panic").unwrap();
    assert!(
        host.command("panic", "run", Value::Null, EditorContext::default())
            .is_err()
    );
    assert_eq!(host.contributions().count(), 0);
    assert!(matches!(host.status("panic"), Some(Status::Failed(_))));
}

#[test]
fn reinstalling_the_same_id_cannot_revalidate_work_from_the_previous_instance() {
    let mut host = Host::default();
    let callback = |_: &Request| {
        Ok(Response {
            effects: vec![Effect::Notify {
                message: "Activated".into(),
            }],
            ..Default::default()
        })
    };
    install(&mut host, manifest("reload"), plugin(callback));
    let old = host.enable("reload").unwrap().remove(0).origin;
    assert!(host.is_current(&old));
    host.uninstall("reload");
    install(&mut host, manifest("reload"), plugin(callback));
    let new = host.enable("reload").unwrap().remove(0).origin;
    assert!(host.is_current(&new));
    assert!(!host.is_current(&old));
    assert_ne!(old, new);
}

fn project_data_plugin(request: &Request) -> Result<Response, String> {
    Ok(match request {
        Request::Activate { .. } => Response {
            contributions: vec![command()],
            ..Default::default()
        },
        Request::Command {
            arguments, context, ..
        } => Response {
            effects: vec![Effect::Edit {
                transaction: Transaction {
                    version: context.version,
                    label: "Project data".into(),
                    operations: vec![PatchOperation::PluginData {
                        plugin: arguments.as_str().unwrap_or("example.data").into(),
                        entry: Some(PluginData::new(
                            1,
                            PluginDataPolicy::EditorOnly,
                            json!({"kept":42}),
                        )),
                    }],
                },
            }],
            ..Default::default()
        },
        _ => Response::default(),
    })
}

#[test]
fn project_data_history_outlives_its_plugin_and_typed_foreign_writes_are_rejected() {
    let mut host = Host::default();
    install(
        &mut host,
        manifest("example.data"),
        plugin(project_data_plugin),
    );
    host.enable("example.data").unwrap();
    let mut editor = phistudio_core::Editor::new(
        phistudio_core::ChartDocument::empty("Data lifetime", 120.).unwrap(),
    );
    let original = editor.document().as_json().clone();
    let effect = host
        .command("example.data", "run", Value::Null, EditorContext::default())
        .unwrap()
        .effects
        .remove(0);
    let Effect::Edit { transaction } = effect.effect else {
        panic!()
    };
    host.apply_transaction(&effect.origin, 0, &mut editor, transaction)
        .unwrap();
    let changed = editor.document().as_json().clone();
    assert_eq!(changed[PLUGIN_DATA_KEY]["example.data"]["data"]["kept"], 42);
    host.uninstall("example.data");
    assert!(editor.undo());
    assert_eq!(editor.document().as_json(), &original);
    assert!(editor.redo());
    assert_eq!(editor.document().as_json(), &changed);
    install(
        &mut host,
        manifest("example.data"),
        plugin(project_data_plugin),
    );
    host.enable("example.data").unwrap();
    let failure = host
        .command(
            "example.data",
            "run",
            json!("example.other"),
            EditorContext::default(),
        )
        .unwrap_err();
    assert!(failure.message.contains("producing plugin"));
    assert_eq!(editor.document().as_json(), &changed);
}

#[test]
fn runtime_rechecks_project_data_ownership_at_the_document_boundary() {
    let mut runtime = Runtime::spawn(
        None,
        vec![Builtin {
            manifest: manifest("example.data"),
            enabled: true,
            create: Arc::new(|| plugin(project_data_plugin)),
        }],
    )
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(completion) = runtime.poll().unwrap() {
            completion.result.unwrap();
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let origin = runtime.registry().unwrap().plugins[0]
        .origin
        .clone()
        .unwrap();
    let mut editor =
        phistudio_core::Editor::new(phistudio_core::ChartDocument::empty("Guard", 120.).unwrap());
    let transaction = Transaction {
        version: DocumentVersion::default(),
        label: "Foreign data".into(),
        operations: vec![PatchOperation::PluginData {
            plugin: "example.other".into(),
            entry: Some(PluginData::new(1, PluginDataPolicy::Runtime, Value::Null)),
        }],
    };
    assert!(
        runtime
            .apply_transaction(&origin, &[], 0, &mut editor, transaction)
            .is_err()
    );
    assert!(!editor.is_dirty() && !editor.can_undo());
    runtime.shutdown().unwrap();
}

#[test]
fn preview_effects_require_registered_correlated_readonly_render_frames() {
    for case in ["valid", "token", "draw", "edit", "driver"] {
        let mut host = Host::default();
        let mut descriptor = manifest("example.fx");
        descriptor.capabilities.insert(Capability::Preview);
        install(
            &mut host,
            descriptor,
            plugin(move |request| {
                Ok(match request {
                    Request::Activate { .. } => Response {
                        contributions: vec![Contribution::PreviewEffect {
                            id: "grade".into(),
                            priority: 0,
                        }],
                        ..Default::default()
                    },
                    Request::Render { frame, context } => {
                        let effect = match case {
                            "draw" => Effect::Draw {
                                frame: frame.token,
                                layers: vec![],
                            },
                            "edit" => Effect::Edit {
                                transaction: Transaction {
                                    version: context.version,
                                    label: "Forbidden".into(),
                                    operations: vec![],
                                },
                            },
                            _ => Effect::PreviewEffects {
                                frame: if case == "token" {
                                    frame.token + 1
                                } else {
                                    frame.token
                                },
                                passes: vec![PreviewPass {
                                    id: "tone".into(),
                                    shader: if case == "driver" {
                                        PreviewShader::Fragment {
                                            source: "invalid GLSL for the driver to report".into(),
                                        }
                                    } else {
                                        PreviewShader::Preset {
                                            name: "grayscale".into(),
                                        }
                                    },
                                    uniforms: Default::default(),
                                }],
                            },
                        };
                        Response {
                            effects: vec![effect],
                            ..Default::default()
                        }
                    }
                    _ => Response::default(),
                })
            }),
        );
        host.enable("example.fx").unwrap();
        let mut frame = RenderFrame {
            token: 1,
            instance: "preview-effects".into(),
            target: RenderTarget {
                kind: RenderKind::Preview,
                id: "grade".into(),
            },
            viewport: Viewport {
                size: [1280., 720.],
                timeline: None,
            },
        };
        assert!(
            host.render("example.fx", frame.clone(), EditorContext::default())
                .is_err()
        );
        frame.target.kind = RenderKind::PreviewEffect;
        let result = host.render("example.fx", frame, EditorContext::default());
        assert_eq!(result.is_ok(), matches!(case, "valid" | "driver"), "{case}");
        if case == "driver" {
            assert_eq!(host.status("example.fx"), Some(&Status::Enabled));
        }
    }
}
