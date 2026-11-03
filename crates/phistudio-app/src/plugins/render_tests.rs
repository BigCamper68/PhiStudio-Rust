use super::*;
use phistudio_core::{ChartDocument, Command, Note};
use phistudio_plugins::Builtin;
use phistudio_sdk::*;
use std::sync::{Arc, Mutex, mpsc};

const MARKER: egui::Color32 = egui::Color32::from_rgb(231, 17, 99);
struct Gate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
struct Renderer {
    gate: Option<Arc<Gate>>,
    blocked: bool,
    panel: bool,
    seen: Arc<Mutex<Vec<String>>>,
}
impl Plugin for Renderer {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => {
                let mut contributions = vec![
                    Contribution::Preview {
                        id: "overlay".into(),
                    },
                    Contribution::Track {
                        id: "clock".into(),
                        title: "Render clock".into(),
                    },
                ];
                let mut effects = vec![];
                if self.panel {
                    contributions.push(Contribution::Panel {
                        id: "settings".into(),
                        title: "Renderer settings".into(),
                        location: PanelLocation::Window,
                        snapshot: false,
                    });
                    effects.push(Effect::Panel {
                        id: "settings".into(),
                        widgets: vec![Widget::Button {
                            id: "action".into(),
                            text: "Renderer action".into(),
                            enabled: true,
                        }],
                    });
                }
                Response {
                    contributions,
                    effects,
                    ..Default::default()
                }
            }
            Request::Render { frame, context } => {
                assert!(context.chart.is_none());
                assert!(context.selection.is_null());
                if !self.blocked
                    && let Some(gate) = &self.gate
                {
                    gate.entered.send(()).unwrap();
                    gate.release
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                    self.blocked = true;
                }
                self.seen.lock().unwrap().push(frame.instance.clone());
                let (space, center) = if frame.target.kind == RenderKind::Track {
                    (DrawSpace::Timeline, [0., 4.])
                } else {
                    (DrawSpace::Normalized, [0.25, 0.75])
                };
                Response {
                    effects: vec![Effect::Draw {
                        frame: frame.token,
                        layers: vec![DrawLayer {
                            space,
                            primitives: vec![DrawPrimitive::Circle {
                                center,
                                radius: 6.,
                                color: DrawColor::Rgba([231, 17, 99, 255]),
                            }],
                        }],
                    }],
                    ..Default::default()
                }
            }
            Request::Ui { .. } => Response {
                effects: vec![Effect::Notify {
                    message: "Renderer action accepted".into(),
                }],
                ..Default::default()
            },
            _ => Response::default(),
        })
    }
}
fn app(gate: Option<Arc<Gate>>, panel: bool, seen: Arc<Mutex<Vec<String>>>) -> Workbench {
    let manifest:Manifest=serde_json::from_value(json!({"api":1,"id":"test.renderer","name":"Renderer","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["preview","timeline","ui"]})).unwrap();
    let mut app = Workbench::new(ChartDocument::empty("Rendering", 120.).unwrap(), None);
    app.plugins.runtime = Some(
        Runtime::spawn(
            None,
            vec![Builtin {
                manifest,
                enabled: true,
                create: Arc::new(move || {
                    Box::new(Renderer {
                        gate: gate.clone(),
                        blocked: false,
                        panel,
                        seen: seen.clone(),
                    })
                }),
            }],
        )
        .unwrap(),
    );
    app.plugins.pending.insert(0, None);
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.plugins.busy() {
        app.poll_plugins();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(app.error.is_none(), "{:?}", app.error);
    let origin = app.plugins.registry.plugins[0].origin.clone().unwrap();
    app.open_plugin_panel(&origin, "settings");
    app.plugins.rendering.tracks.insert(
        (origin.plugin.clone(), "clock".into()),
        rendering::TrackWindow { origin, open: true },
    );
    app
}
fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    ctx
}
fn rect() -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(100., 80.), egui::vec2(400., 180.))
}
fn frame(
    app: &mut Workbench,
    ctx: &egui::Context,
    rect: egui::Rect,
    events: Vec<egui::Event>,
    windows: bool,
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
            egui::CentralPanel::default().show(ctx, |ui| app.paint_preview(ui, rect));
            if windows {
                app.plugin_windows(ctx);
            }
        },
    )
}
fn marker_centers(output: &egui::FullOutput) -> Vec<(egui::Rect, egui::Pos2)> {
    output
        .shapes
        .iter()
        .filter_map(|s| match &s.1 {
            egui::Shape::Circle(c) if c.fill == MARKER => Some((s.0, c.center)),
            _ => None,
        })
        .collect()
}
fn wait_marker(
    app: &mut Workbench,
    ctx: &egui::Context,
    rect: egui::Rect,
    windows: bool,
) -> egui::FullOutput {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let output = frame(app, ctx, rect, vec![], windows);
        assert!(app.error.is_none(), "{:?}", app.error);
        if !marker_centers(&output).is_empty() {
            return output;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn cached_preview_layers_clip_scale_follow_revision_and_disappear_on_disable() {
    let mut app = app(None, false, Arc::default());
    let ctx = context();
    let area = rect();
    let output = wait_marker(&mut app, &ctx, area, false);
    let (clip, center) = marker_centers(&output)[0];
    assert_eq!(clip, area);
    assert_eq!(center, area.min + egui::vec2(100., 135.));
    assert!(!app.editor.is_dirty());
    assert!(!app.editor.can_undo());
    let larger = egui::Rect::from_min_size(area.min, egui::vec2(600., 240.));
    assert!(
        marker_centers(&frame(&mut app, &ctx, larger, vec![], false)).is_empty(),
        "Old layout must not paint at the new size"
    );
    let output = wait_marker(&mut app, &ctx, larger, false);
    assert_eq!(
        marker_centers(&output)[0].1,
        larger.min + egui::vec2(150., 180.)
    );
    app.execute(Command::AddNote {
        line: 0,
        note: Note::default(),
    });
    assert!(
        marker_centers(&frame(&mut app, &ctx, larger, vec![], false)).is_empty(),
        "Old document must not paint after an edit"
    );
    wait_marker(&mut app, &ctx, larger, false);
    app.plugin_action(Action::Disable("test.renderer".into()));
    assert!(marker_centers(&frame(&mut app, &ctx, larger, vec![], false)).is_empty());
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.plugins.working() {
        app.poll_plugins();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(app.plugins.rendering.views.is_empty());
    app.plugins.shutdown().unwrap();
}

#[test]
fn pending_frames_coalesce_latest_geometry_and_leave_panel_buttons_enabled() {
    let (entered, entry) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut app = app(
        Some(Arc::new(Gate {
            entered,
            release: Mutex::new(gate),
        })),
        true,
        Arc::default(),
    );
    let ctx = context();
    let area = rect();
    frame(&mut app, &ctx, area, vec![], false);
    frame(&mut app, &ctx, area, vec![], false);
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(app.plugins.working());
    assert!(!app.plugins.busy());
    let changed = egui::Rect::from_min_size(area.min, egui::vec2(520., 200.));
    for _ in 0..8 {
        frame(&mut app, &ctx, changed, vec![], false);
    }
    assert_eq!(
        app.plugins.rendering.jobs.len(),
        1,
        "Only the latest desired geometry should be retained while a frame runs"
    );
    let mut output = frame(&mut app, &ctx, changed, vec![], true);
    for _ in 0..2 {
        output = frame(&mut app, &ctx, changed, vec![], true);
    }
    let point = output
        .shapes
        .iter()
        .find_map(|s| match &s.1 {
            egui::Shape::Text(t) if t.galley.job.text == "Renderer action" => {
                Some(t.pos + t.galley.rect.center().to_vec2())
            }
            _ => None,
        })
        .unwrap();
    for pressed in [true, false] {
        frame(
            &mut app,
            &ctx,
            changed,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            true,
        );
    }
    let queued = app.plugins.busy();
    release.send(()).unwrap();
    assert!(
        queued,
        "A real enabled button should queue its action while rendering is in progress"
    );
    let output = wait_marker(&mut app, &ctx, changed, false);
    assert_eq!(
        marker_centers(&output)[0].1,
        changed.min + egui::vec2(130., 150.)
    );
    assert_eq!(app.status, "Renderer action accepted");
    app.plugins.shutdown().unwrap();
}

#[test]
fn track_windows_share_the_editor_clock_and_back_only_closes_the_window() {
    let mut app = app(None, false, Arc::default());
    let ctx = context();
    app.transport.beat = 4.;
    let deadline = Instant::now() + Duration::from_secs(5);
    let before = loop {
        let output = frame(&mut app, &ctx, rect(), vec![], true);
        if marker_centers(&output).len() >= 2 {
            break marker_centers(&output)
                .into_iter()
                .find(|(clip, _)| *clip != rect())
                .unwrap()
                .1;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    };
    let key = app
        .plugins
        .rendering
        .views
        .keys()
        .find(|k| k.target.kind == RenderKind::Track)
        .unwrap()
        .clone();
    let viewport = &app.plugins.rendering.views[&key].viewport;
    let t = viewport.timeline.as_ref().unwrap();
    assert_eq!(t.beat, 4.);
    assert_eq!(t.pixels_per_beat, app.timeline.zoom);
    app.transport.beat = 3.5;
    let output = frame(&mut app, &ctx, rect(), vec![], true);
    // Existing absolute-beat geometry is reprojected immediately as time moves.
    assert!(marker_centers(&output).len() >= 2);
    let after = marker_centers(&output)
        .into_iter()
        .find(|(clip, _)| *clip != rect())
        .unwrap()
        .1;
    assert!((after.y - (before.y - 0.5 * app.timeline.zoom)).abs() < 0.01);
    app.back();
    assert!(!app.close_requested);
    assert!(app.plugins.rendering.tracks.values().all(|t| !t.open));
    app.plugins.shutdown().unwrap();
}

#[test]
fn visible_view_scheduling_services_new_views_before_repeating_old_ones() {
    let seen = Arc::new(Mutex::new(vec![]));
    let mut app = app(None, false, seen.clone());
    let ctx = context();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(873., 393.),
                )),
                ..Default::default()
            },
            |ctx| {
                app.poll_plugins();
                egui::CentralPanel::default().show(ctx, |ui| {
                    for i in 0..20 {
                        app.plugin_preview_overlay(ui.painter(), rect(), &format!("view-{i}"));
                    }
                });
            },
        );
        let unique: std::collections::BTreeSet<_> = seen.lock().unwrap().iter().cloned().collect();
        if unique.len() == 20 {
            break;
        }
        assert!(app.plugins.rendering.jobs.len() <= 8);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    app.plugins.shutdown().unwrap();
}

#[test]
fn pending_frames_from_old_projects_or_expired_requests_are_discarded() {
    for replace in [true, false] {
        let (entered, entry) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let mut app = app(
            Some(Arc::new(Gate {
                entered,
                release: Mutex::new(gate),
            })),
            false,
            Arc::default(),
        );
        let ctx = context();
        frame(&mut app, &ctx, rect(), vec![], false);
        frame(&mut app, &ctx, rect(), vec![], false);
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        let job = *app.plugins.rendering.jobs.keys().next().unwrap();
        if replace {
            app.replace_document(
                ChartDocument::empty("Replacement", 160.).unwrap(),
                None,
                None,
            );
        } else {
            app.plugins.rendering.jobs.get_mut(&job).unwrap().requested =
                Instant::now() - Duration::from_secs(1);
        }
        let version = app.plugin_version();
        assert!(marker_centers(&frame(&mut app, &ctx, rect(), vec![], false)).is_empty());
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.plugins.rendering.jobs.contains_key(&job) {
            app.poll_plugins();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            app.plugins
                .rendering
                .views
                .values()
                .all(|v| v.cache.is_none())
        );
        wait_marker(&mut app, &ctx, rect(), false);
        assert!(
            app.plugins
                .rendering
                .views
                .values()
                .all(|v| v.cache.as_ref().is_none_or(|c| c.version == version))
        );
        assert!(!app.editor.is_dirty());
        assert!(!app.editor.can_undo());
        app.plugins.shutdown().unwrap();
    }
}

#[test]
fn disabling_during_a_frame_closes_tracks_before_the_worker_reaches_deactivation() {
    let (entered, entry) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut app = app(
        Some(Arc::new(Gate {
            entered,
            release: Mutex::new(gate),
        })),
        false,
        Arc::default(),
    );
    let ctx = context();
    frame(&mut app, &ctx, rect(), vec![], true);
    frame(&mut app, &ctx, rect(), vec![], true);
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(!app.plugins.rendering.tracks.is_empty());
    app.plugin_action(Action::Disable("test.renderer".into()));
    let output = frame(&mut app, &ctx, rect(), vec![], true);
    let hidden = app.plugins.rendering.tracks.is_empty()
        && app.plugins.rendering.views.is_empty()
        && marker_centers(&output).is_empty();
    release.send(()).unwrap();
    assert!(
        hidden,
        "Revoked tracks must not be recreated from a cached registry"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.plugins.working() {
        app.poll_plugins();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    app.plugins.shutdown().unwrap();
}

fn guide_cross(output: &egui::FullOutput, area: egui::Rect, color: egui::Color32) -> bool {
    output.shapes.iter().any(|shape|matches!(&shape.1,egui::Shape::LineSegment {points,stroke}
        if stroke.color==color && points[0]==area.center_top() && points[1]==area.center_bottom() && shape.0==area))
}

#[test]
#[ignore = "requires the separately built editor-guides Wasm example"]
fn compiled_wasm_guides_paint_preview_track_and_canvas_and_accept_their_real_checkbox() {
    let root = tempfile::tempdir().unwrap();
    let mut app = Workbench::new(ChartDocument::empty("Compiled guides", 120.).unwrap(), None);
    let original = app.editor.document().as_json().clone();
    app.workspace.root = Some(root.path().into());
    app.start_plugins(Some(root.path().into()));
    let settle = |app: &mut Workbench| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.plugins.busy() {
            app.poll_plugins();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(app.error.is_none(), "{:?}", app.error);
    };
    settle(&mut app);
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/plugins/editor-guides");
    app.install_plugin(MediaFile::new(source, "Editor guides".into(), false));
    settle(&mut app);
    app.plugin_action(Action::Enable("example.editor-guides".into()));
    settle(&mut app);
    let origin = app.plugins.registry.plugins[0].origin.clone().unwrap();
    app.open_plugin_panel(&origin, "settings");
    app.plugins.rendering.tracks.insert(
        (origin.plugin.clone(), "beats".into()),
        rendering::TrackWindow { origin, open: true },
    );
    let ctx = context();
    let area = rect();
    let deadline = Instant::now() + Duration::from_secs(10);
    let output = loop {
        let output = frame(&mut app, &ctx, area, vec![], true);
        assert!(app.error.is_none(), "{:?}", app.error);
        let track = app.plugins.rendering.views.iter().any(|(k, v)| {
            k.target.kind == RenderKind::Track
                && v.cache.as_ref().is_some_and(|c| !c.layers.is_empty())
        });
        if guide_cross(&output, area, crate::theme::accent()) && track {
            break output;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    };
    assert!(output.shapes.iter().any(|s|matches!(&s.1,egui::Shape::Text(t)
        if t.galley.job.text=="Plugin beat guide" && t.galley.job.sections[0].format.font_id.family==egui::FontFamily::Monospace)));
    let canvas =
        &app.plugins.panels[&("example.editor-guides".into(), "settings".into())].widgets[1];
    assert!(matches!(canvas,Widget::Canvas {layers,..} if layers[0].primitives.len()==80));
    let mut theme = app.options.theme;
    theme.colors[6] = [18, 207, 143, 255];
    crate::theme::apply(&ctx, &theme);
    let output = frame(&mut app, &ctx, area, vec![], true);
    assert!(
        guide_cross(&output, area, crate::theme::accent()),
        "Theme tokens must resolve while painting cached layers"
    );
    let point = output
        .shapes
        .iter()
        .find_map(|s| match &s.1 {
            egui::Shape::Text(t) if t.galley.job.text == "Show plugin guides" => {
                Some(t.pos + t.galley.rect.center().to_vec2())
            }
            _ => None,
        })
        .unwrap();
    for pressed in [true, false] {
        frame(
            &mut app,
            &ctx,
            area,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            true,
        );
    }
    loop {
        let output = frame(&mut app, &ctx, area, vec![], true);
        assert!(app.error.is_none(), "{:?}", app.error);
        let clear = app.plugins.rendering.views.iter().any(|(k, v)| {
            k.target.kind == RenderKind::Preview
                && v.cache.as_ref().is_some_and(|c| c.layers.is_empty())
        });
        if clear {
            assert!(!guide_cross(&output, area, crate::theme::accent()));
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(!app.editor.is_dirty());
    assert!(!app.editor.can_undo());
    app.plugin_action(Action::Disable("example.editor-guides".into()));
    settle(&mut app);
    assert!(!guide_cross(
        &frame(&mut app, &ctx, area, vec![], true),
        area,
        crate::theme::accent()
    ));
    app.plugins.shutdown().unwrap();
}
