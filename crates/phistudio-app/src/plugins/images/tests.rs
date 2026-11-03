use super::*;
use crate::{media::MediaFile, plugins::Action, workbench::Workbench};
use phistudio_core::{ChartDocument, Command, Note};
use phistudio_sdk::*;
use std::time::{Duration, Instant};

#[test]
fn image_geometry_clips_destination_and_uv_coordinates_in_each_space() {
    let ctx = egui::Context::default();
    let rect = egui::Rect::from_min_size(egui::pos2(100., 80.), egui::vec2(400., 200.));
    let texture = egui::TextureId::Managed(17);
    for (space, min, max) in [
        (DrawSpace::Points, [-100., -50.], [500., 250.]),
        (DrawSpace::Normalized, [-0.25, -0.25], [1.25, 1.25]),
        (DrawSpace::Timeline, [-150., -1.], [150., 2.]),
    ] {
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx
                .layer_painter(egui::LayerId::background())
                .with_clip_rect(rect);
            crate::plugins::drawing::paint(
                &painter,
                rect,
                &Viewport {
                    size: [400., 200.],
                    timeline: Some(TimelineViewport {
                        beat: 0.,
                        pixels_per_beat: 100.,
                        baseline: 150.,
                        x_min: -100.,
                        x_max: 100.,
                        division: 4,
                        lanes: 11,
                    }),
                },
                &[DrawLayer {
                    space,
                    primitives: vec![DrawPrimitive::Image {
                        file: 5,
                        min,
                        max,
                        tint: DrawColor::Rgba([255; 4]),
                    }],
                }],
                &BTreeMap::from([(5, texture)]),
            );
        });
        let mesh = output
            .shapes
            .iter()
            .find_map(|s| match &s.1 {
                egui::Shape::Mesh(m) if m.texture_id == texture => Some(m),
                _ => None,
            })
            .unwrap();
        assert_eq!(mesh.vertices.len(), 4);
        for v in &mesh.vertices {
            assert!(rect.contains(v.pos), "{space:?}: {v:?}");
            assert!(v.uv.x > 0. && v.uv.x < 1. && v.uv.y > 0. && v.uv.y < 1.);
        }
    }
}

fn area() -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(650., 450.), egui::vec2(400., 200.))
}
fn frame(app: &mut Workbench, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100., 720.),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            app.poll_plugins();
            let painter = ctx.layer_painter(egui::LayerId::background());
            app.plugin_preview_overlay(&painter, area(), "asset-test");
            app.plugin_windows(ctx);
        },
    )
}
fn settle(app: &mut Workbench, ctx: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        frame(app, ctx, vec![]);
        assert!(app.error.is_none(), "{:?}", app.error);
        if !app.plugins.working() {
            break;
        }
        assert!(Instant::now() < deadline, "Asset view did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
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
    frame(app, ctx, vec![]);
    let output = frame(app, ctx, vec![]);
    let point = output
        .shapes
        .iter()
        .rev()
        .find_map(|s| match &s.1 {
            egui::Shape::Text(t) if t.galley.job.text == label => {
                let p = t.pos + t.galley.rect.center().to_vec2();
                s.0.contains(p).then_some(p)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing asset control: {label}"));
    assert!(!app.modal_open(), "Asset controls are covered by a modal");
    assert!(!app.plugins.busy(), "Asset controls are busy");
    frame(app, ctx, vec![egui::Event::PointerMoved(point)]);
    for pressed in [true, false] {
        frame(
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
    assert!(
        app.plugins.busy(),
        "Asset control was not activated: {label} at {point:?}, layer {:?}",
        ctx.layer_id_at(point)
    );
    settle(app, ctx);
}
fn displayed(app: &Workbench) -> u64 {
    let panel = &app.plugins.panels[&("example.asset-view".into(), "assets".into())];
    panel
        .widgets
        .iter()
        .find_map(|w| match w {
            Widget::Canvas { layers, .. } => {
                layers.iter().flat_map(|l| &l.primitives).find_map(|p| {
                    if let DrawPrimitive::Image { file, .. } = p {
                        Some(*file)
                    } else {
                        None
                    }
                })
            }
            _ => None,
        })
        .expect("Asset canvas must display an image")
}
fn meshes(output: &egui::FullOutput, texture: egui::TextureId) -> Vec<egui::Rect> {
    output
        .shapes
        .iter()
        .filter_map(|s| match &s.1 {
            egui::Shape::Mesh(m) if m.texture_id == texture => {
                Some(m.vertices.iter().fold(egui::Rect::NOTHING, |r, v| {
                    r.union(egui::Rect::from_min_max(v.pos, v.pos))
                }))
            }
            _ => None,
        })
        .collect()
}

#[test]
#[ignore = "requires the separately built asset-view Wasm example"]
fn compiled_wasm_asset_view_reads_packages_and_projects_and_paints_revocable_images_through_real_controls()
 {
    let root = tempfile::tempdir().unwrap();
    let assets = root.path().join("assets");
    std::fs::create_dir(&assets).unwrap();
    std::fs::write(
        assets.join("cover.png"),
        include_bytes!("../../../../../assets/skins/phigros/click.png"),
    )
    .unwrap();
    let mut app = Workbench::new(ChartDocument::empty("Asset test", 120.).unwrap(), None);
    app.workspace.root = Some(root.path().into());
    app.replace_document(app.editor.document().clone(), None, Some(assets.clone()));
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    app.start_plugins(Some(root.path().into()));
    settle(&mut app, &ctx);
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/asset-view");
    app.install_plugin(MediaFile::new(source, "Asset view".into(), false));
    settle(&mut app, &ctx);
    app.plugin_action(Action::Enable("example.asset-view".into()));
    settle(&mut app, &ctx);
    let bundled = displayed(&app);
    let origin = app.plugins.registry.plugins[0].origin.clone().unwrap();
    app.open_plugin_panel(&origin, "assets");
    app.plugins.rendering.tracks.insert(
        (origin.plugin.clone(), "images".into()),
        crate::plugins::rendering::TrackWindow {
            origin: origin.clone(),
            open: true,
        },
    );
    settle(&mut app, &ctx);
    assert_eq!(
        app.plugins
            .runtime
            .as_ref()
            .unwrap()
            .image(&origin, bundled)
            .unwrap()
            .size(),
        [16, 16]
    );
    let texture =
        app.plugins.images.textures[&(origin.plugin.clone(), origin.generation, bundled)].id();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let output = frame(&mut app, &ctx, vec![]);
        let rects = meshes(&output, texture);
        if rects.len() >= 3 && rects.iter().any(|r| area().contains_rect(*r)) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Canvas, track and preview must paint the bundled image: {rects:?}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!app.editor.is_dirty() && !app.editor.can_undo());
    click(&mut app, &ctx, "Read project resources");
    assert!(
        app.plugins.panels[&("example.asset-view".into(), "assets".into())]
            .widgets
            .iter()
            .any(|w| matches!(w, Widget::Label { text } if text.starts_with("cover.png ·"))),
        "Asset panel after list: {:?}",
        app.plugins.panels[&("example.asset-view".into(), "assets".into())].widgets
    );
    click(&mut app, &ctx, "Show project image");
    let project = displayed(&app);
    assert_ne!(project, bundled);
    assert!(
        app.plugins
            .runtime
            .as_ref()
            .unwrap()
            .image(&origin, project)
            .is_some()
    );
    app.editor
        .execute(Command::AddNote {
            line: 0,
            note: Note::default(),
        })
        .unwrap();
    settle(&mut app, &ctx);
    assert_eq!(displayed(&app), project);
    assert!(
        app.plugins
            .runtime
            .as_ref()
            .unwrap()
            .image(&origin, project)
            .is_some()
    );
    app.workspace_result(
        app.session,
        crate::workspace_ui::WorkspaceResult::Resources {
            entries: vec![],
            undo: None,
            changed: true,
        },
    );
    assert!(
        app.plugins
            .runtime
            .as_ref()
            .unwrap()
            .image(&origin, project)
            .is_none()
    );
    settle(&mut app, &ctx);
    assert_eq!(displayed(&app), bundled);
    click(&mut app, &ctx, "Show project image");
    let reopened = displayed(&app);
    assert_ne!(reopened, project);
    app.replace_document(
        ChartDocument::empty("Replacement", 120.).unwrap(),
        None,
        Some(assets),
    );
    assert!(
        app.plugins
            .runtime
            .as_ref()
            .unwrap()
            .image(&origin, reopened)
            .is_none(),
        "Project replacement revokes before the next poll"
    );
    settle(&mut app, &ctx);
    assert_eq!(displayed(&app), bundled);
    app.plugin_action(Action::Reload);
    settle(&mut app, &ctx);
    let reloaded = displayed(&app);
    assert_ne!(reloaded, bundled);
    assert!(
        app.plugins
            .runtime
            .as_ref()
            .unwrap()
            .image(&origin, bundled)
            .is_none()
    );
    let origin = app.plugins.registry.plugins[0].origin.clone().unwrap();
    assert!(
        app.plugins
            .runtime
            .as_ref()
            .unwrap()
            .image(&origin, reloaded)
            .is_some()
    );
    let texture =
        app.plugins.images.textures[&(origin.plugin.clone(), origin.generation, reloaded)].id();
    let disabled = app
        .plugins
        .submit(Action::Disable("example.asset-view".into()), None)
        .unwrap();
    assert!(app.plugins.pending.contains_key(&disabled));
    assert!(
        app.plugins
            .runtime
            .as_ref()
            .unwrap()
            .image(&origin, reloaded)
            .is_none()
    );
    assert!(meshes(&frame(&mut app, &ctx, vec![]), texture).is_empty());
    settle(&mut app, &ctx);
    assert!(app.plugins.images.textures.is_empty());
    assert!(app.plugins.panels.is_empty());
    assert_eq!(app.editor.document().name(), "Replacement");
    assert!(!app.editor.is_dirty() && !app.editor.can_undo());
    app.plugins.shutdown().unwrap();
}
