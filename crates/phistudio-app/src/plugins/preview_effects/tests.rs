use super::*;
use phistudio_core::{ChartDocument, Command, Note};
use phistudio_plugins::Builtin;
use phistudio_sdk::*;
use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

struct Gate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
struct Filter {
    priority: i32,
    gate: Option<Arc<Gate>>,
    blocked: bool,
}
impl Plugin for Filter {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![Contribution::PreviewEffect {
                    id: "grade".into(),
                    priority: self.priority,
                }],
                ..Default::default()
            },
            Request::Render { frame, context } => {
                assert_eq!(frame.target.kind, RenderKind::PreviewEffect);
                assert_eq!(frame.instance, "preview-effects");
                assert!(context.chart.is_none() && context.selection.is_null());
                if !self.blocked
                    && let Some(gate) = &self.gate
                {
                    self.blocked = true;
                    gate.entered.send(()).unwrap();
                    gate.release
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(10))
                        .unwrap();
                }
                Response {
                    effects: vec![Effect::PreviewEffects {
                        frame: frame.token,
                        passes: vec![PreviewPass {
                            id: "tone".into(),
                            shader: PreviewShader::Preset {
                                name: "grayscale".into(),
                            },
                            uniforms: BTreeMap::from([(
                                "factor".into(),
                                ShaderUniform::Float(0.5),
                            )]),
                        }],
                    }],
                    ..Default::default()
                }
            }
            _ => Response::default(),
        })
    }
}
fn builtin(id: &str, priority: i32, gate: Option<Arc<Gate>>) -> Builtin {
    let manifest = serde_json::from_value(json!({"api":1,"id":id,"name":id,"version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["preview"]})).unwrap();
    Builtin {
        manifest,
        enabled: true,
        create: Arc::new(move || {
            Box::new(Filter {
                priority,
                gate: gate.clone(),
                blocked: false,
            })
        }),
    }
}
fn app(builtins: Vec<Builtin>) -> Workbench {
    let mut app = Workbench::new(ChartDocument::empty("Preview passes", 120.).unwrap(), None);
    app.options.preview_shaders = true;
    app.plugins.runtime = Some(Runtime::spawn(None, builtins).unwrap());
    app.plugins.pending.insert(0, None);
    app
}
fn wait(app: &mut Workbench, ready: impl Fn(&Workbench) -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        app.poll_plugins();
        assert!(app.error.is_none(), "{:?}", app.error);
        if ready(app) {
            break;
        }
        assert!(Instant::now() < end, "Preview callback did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn preview_passes_are_ordered_versioned_resized_and_revoked_without_changing_the_chart() {
    let mut app = app(vec![
        builtin("test.last", 20, None),
        builtin("test.first", -10, None),
    ]);
    let original = app.editor.document().as_json().clone();
    wait(&mut app, |a| a.plugin_preview_effects().len() == 2);
    assert_eq!(
        app.plugin_preview_effects()
            .iter()
            .map(|p| p.plugin.as_str())
            .collect::<Vec<_>>(),
        ["test.first", "test.last"]
    );
    assert!(!app.plugins.busy());
    app.preview.size = [1920, 1080];
    assert!(app.plugin_preview_effects().is_empty());
    wait(&mut app, |a| a.plugin_preview_effects().len() == 2);
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(!app.editor.is_dirty());
    app.editor
        .execute(Command::AddNote {
            line: 0,
            note: Note::default(),
        })
        .unwrap();
    assert!(app.plugin_preview_effects().is_empty());
    wait(&mut app, |a| a.plugin_preview_effects().len() == 2);
    app.options.preview_shaders = false;
    assert!(app.plugin_preview_effects().is_empty());
    app.poll_plugins();
    app.options.preview_shaders = true;
    wait(&mut app, |a| a.plugin_preview_effects().len() == 2);
    app.plugin_action(Action::Disable("test.first".into()));
    assert_eq!(app.plugin_preview_effects().len(), 1);
    wait(&mut app, |a| !a.plugins.working());
    app.replace_document(
        ChartDocument::empty("Replacement", 120.).unwrap(),
        None,
        None,
    );
    assert!(app.plugin_preview_effects().is_empty());
    wait(&mut app, |a| a.plugin_preview_effects().len() == 1);
    app.plugins.shutdown().unwrap();
}
#[test]
fn in_progress_effects_remain_background_work_and_cannot_survive_disable() {
    let (entered, entry) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut app = app(vec![builtin(
        "test.blocked",
        0,
        Some(Arc::new(Gate {
            entered,
            release: Mutex::new(gate),
        })),
    )]);
    wait(&mut app, |a| a.plugins.background.len() == 1);
    entry.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(!app.plugins.busy());
    app.plugin_action(Action::Disable("test.blocked".into()));
    assert!(app.plugin_preview_effects().is_empty());
    release.send(()).unwrap();
    wait(&mut app, |a| !a.plugins.working());
    assert!(app.plugin_preview_effects().is_empty());
    assert!(!app.editor.is_dirty() && !app.editor.can_undo());
    app.plugins.shutdown().unwrap();
}

fn ui_frame(
    app: &mut Workbench,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(873., 393.),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            app.poll_plugins();
            app.plugin_windows(ctx);
        },
    )
}
fn click(app: &mut Workbench, ctx: &egui::Context, label: &str) {
    let views: Vec<_> = app
        .plugins
        .panels
        .iter()
        .map(|((_, id), p)| (p.origin.clone(), id.clone()))
        .collect();
    for (origin, id) in views {
        app.open_plugin_panel(&origin, &id);
    }
    ui_frame(app, ctx, vec![]);
    let output = ui_frame(app, ctx, vec![]);
    let point = output
        .shapes
        .iter()
        .rev()
        .find_map(|s| match &s.1 {
            egui::Shape::Text(t) if t.galley.job.text == label => {
                let point = t.pos + t.galley.rect.center().to_vec2();
                s.0.contains(point).then_some(point)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing preview control: {label}"));
    for pressed in [true, false] {
        ui_frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}
#[test]
#[ignore = "requires the separately built preview-fx Wasm example"]
fn compiled_wasm_preview_fx_accepts_real_controls_and_preserves_chart_history_on_disable() {
    let root = tempfile::tempdir().unwrap();
    let mut app = Workbench::new(
        ChartDocument::empty("Compiled effects", 120.).unwrap(),
        None,
    );
    app.options.preview_shaders = true;
    app.workspace.root = Some(root.path().into());
    app.start_plugins(Some(root.path().into()));
    wait(&mut app, |a| !a.plugins.working());
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/preview-fx");
    app.install_plugin(crate::media::MediaFile::new(
        source,
        "Preview FX".into(),
        false,
    ));
    wait(&mut app, |a| !a.plugins.working());
    app.plugin_action(Action::Enable("example.preview-fx".into()));
    wait(&mut app, |a| a.plugin_preview_effects().len() == 1);
    let original = app.editor.document().as_json().clone();
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    assert!(matches!(
        app.plugin_preview_effects()[0].pass.shader,
        PreviewShader::Preset { .. }
    ));
    click(&mut app, &ctx, "Use custom GLSL");
    wait(&mut app, |a| {
        a.plugin_preview_effects()
            .first()
            .is_some_and(|p| matches!(p.pass.shader, PreviewShader::Fragment { .. }))
    });
    assert_eq!(app.plugin_preview_effects()[0].pass.uniforms.len(), 3);
    let pass = app.plugin_preview_effects().remove(0);
    app.plugin_preview_effect_failures(vec![phistudio_prpr::PreviewFilterFailure {
        key: phistudio_prpr::PreviewFilterKey {
            plugin: pass.plugin,
            generation: pass.generation,
            contribution: pass.contribution,
            pass: pass.pass.id,
        },
        message: "Simulated driver compile failure".into(),
    }]);
    app.show_projects = true;
    app.plugins.open = true;
    ui_frame(&mut app, &ctx, vec![]);
    let output = ui_frame(&mut app, &ctx, vec![]);
    assert!(output.shapes.iter().any(|s|matches!(&s.1,egui::Shape::Text(t) if t.galley.job.text.contains("Simulated driver compile failure"))));
    app.plugins.open = false;
    app.show_projects = false;
    click(&mut app, &ctx, "Enable plugin preview effect");
    wait(&mut app, |a| a.plugin_preview_effects().is_empty());
    app.plugin_preview_effect_failures(vec![]);
    click(&mut app, &ctx, "Enable plugin preview effect");
    wait(&mut app, |a| a.plugin_preview_effects().len() == 1);
    app.plugin_action(Action::Disable("example.preview-fx".into()));
    assert!(app.plugin_preview_effects().is_empty());
    wait(&mut app, |a| !a.plugins.working());
    assert!(app.plugins.effect_errors.is_empty());
    assert!(app.plugins.panels.is_empty());
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(!app.editor.is_dirty() && !app.editor.can_undo());
    app.plugins.shutdown().unwrap();
}
