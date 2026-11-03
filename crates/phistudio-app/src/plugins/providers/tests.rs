use super::*;
use crate::files::FileManager;
use phistudio_core::ChartDocument;
use std::time::{Duration, Instant};

#[path = "tests/lifecycle.rs"]
mod lifecycle;
#[path = "tests/plugin_data.rs"]
mod plugin_data;
#[path = "tests/project.rs"]
mod project;

fn frame(
    app: &mut Workbench,
    files: &mut FileManager,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    files.update(app);
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100., 720.),
            )),
            events,
            ..Default::default()
        },
        |ctx| app.ui(ctx, false),
    )
}
fn settle(app: &mut Workbench, files: &mut FileManager, ctx: &egui::Context) {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        frame(app, files, ctx, vec![]);
        if !app.plugins.working()
            && !app.files.busy
            && app.files.request.is_none()
            && app.plugins.providers.active.is_none()
        {
            break;
        }
        assert!(
            Instant::now() < end,
            "Provider workflow did not finish: {:?}",
            app.error
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn click(app: &mut Workbench, files: &mut FileManager, ctx: &egui::Context, label: &str) {
    if matches!(
        label,
        "Import with plugin" | "Export with plugin" | "Convert resource"
    ) && !app.plugin_provider_modal()
    {
        app.show_projects = false;
        app.plugins.open = false;
        let title = app
            .plugins
            .registry
            .plugins
            .iter()
            .find(|p| p.origin.is_some())
            .unwrap()
            .manifest
            .name
            .clone();
        click(app, files, ctx, "File");
        let title = format!("{} · {}", title, crate::i18n::tr("More…"));
        for _ in 0..12 {
            frame(app, files, ctx, vec![]);
        }
        if control(&frame(app, files, ctx, vec![]), label).is_none() {
            click(app, files, ctx, &title);
            for _ in 0..12 {
                frame(app, files, ctx, vec![]);
            }
        }
    }
    frame(app, files, ctx, vec![]);
    let out = frame(app, files, ctx, vec![]);
    let p = control(&out, label).unwrap_or_else(|| panic!("Missing provider control: {label}"));
    for pressed in [true, false] {
        frame(
            app,
            files,
            ctx,
            vec![
                egui::Event::PointerMoved(p),
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}
fn control(out: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
    out.shapes.iter().rev().find_map(|s| match &s.1 {
        egui::Shape::Text(t) if t.galley.job.text == label => {
            let p = t.pos + t.galley.rect.center().to_vec2();
            s.0.contains(p).then_some(p)
        }
        _ => None,
    })
}

#[test]
#[ignore = "requires the separately built chart-envelope Wasm example"]
fn compiled_wasm_envelope_exports_and_imports_from_the_editor_menu_and_keeps_unknown_chart_data() {
    let root = tempfile::tempdir().unwrap();
    let mut value = ChartDocument::empty("Envelope test", 120.)
        .unwrap()
        .as_json()
        .clone();
    value["pluginData"] = json!({"future":[null,{"text":"Keep source data"}]});
    value["judgeLineList"][0]["unknownLine"] = json!({"keep":true});
    let mut app = Workbench::new(ChartDocument::from_value(value.clone()).unwrap(), None);
    let mut files = FileManager::new(Some(root.path().into()));
    app.start_plugins(Some(root.path().into()));
    let ctx = egui::Context::default();
    settle(&mut app, &mut files, &ctx);
    let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/plugins/chart-envelope");
    app.install_plugin(MediaFile::new(package, "Chart envelope".into(), false));
    settle(&mut app, &mut files, &ctx);
    app.plugin_action(Action::Enable("example.chart-envelope".into()));
    settle(&mut app, &mut files, &ctx);
    app.plugins.open = true;
    let target = root.path().join("chart.pschart");
    click(&mut app, &mut files, &ctx, "Export with plugin");
    assert!(app.plugin_provider_modal());
    app.plugins.providers.dialog.as_mut().unwrap().destination =
        target.to_string_lossy().into_owned();
    click(&mut app, &mut files, &ctx, "Export with plugin");
    settle(&mut app, &mut files, &ctx);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.editor.document().as_json(), &value);
    assert!(!app.editor.is_dirty());
    let bytes = std::fs::read(&target).unwrap();
    let header = b"PhiStudio chart 1\n";
    assert!(bytes.starts_with(header));
    let encoded: Value = serde_json::from_slice(&bytes[header.len()..]).unwrap();
    assert_eq!(encoded["pluginData"], value["pluginData"]);
    app.show_projects = true;
    click(&mut app, &mut files, &ctx, "Import with plugin");
    assert!(app.plugin_provider_modal());
    app.plugins.providers.dialog.as_mut().unwrap().source = target.to_string_lossy().into_owned();
    click(&mut app, &mut files, &ctx, "Import with plugin");
    settle(&mut app, &mut files, &ctx);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(
        app.editor.document().as_json()["pluginData"],
        value["pluginData"]
    );
    assert_eq!(
        app.editor.document().as_json()["judgeLineList"][0]["unknownLine"],
        value["judgeLineList"][0]["unknownLine"]
    );
    assert!(app.file.as_ref().unwrap().is_file());
    assert!(!app.editor.can_undo());
    app.plugin_action(Action::Disable("example.chart-envelope".into()));
    settle(&mut app, &mut files, &ctx);
    assert_eq!(app.editor.document().name(), "Envelope test");
    let saved = root.path().join("without-plugin.rpe.json");
    app.save(saved.clone());
    assert_eq!(
        phistudio_core::load_chart(&saved).unwrap().as_json()["pluginData"],
        value["pluginData"]
    );
    app.plugins.shutdown().unwrap();
}

#[test]
#[ignore = "requires the separately built pixel-grid Wasm example"]
fn compiled_wasm_pixel_grid_imports_a_valid_image_exports_it_and_keeps_resource_undo_after_disable()
{
    let root = tempfile::tempdir().unwrap();
    let document = ChartDocument::empty("Resource provider", 120.).unwrap();
    let location = phistudio_core::create_project(root.path(), &document).unwrap();
    let assets = location.assets.unwrap();
    let mut app = Workbench::new(document, Some(location.chart));
    let original = app.editor.document().as_json().clone();
    let mut files = FileManager::new(Some(root.path().into()));
    app.start_plugins(Some(root.path().into()));
    let ctx = egui::Context::default();
    settle(&mut app, &mut files, &ctx);
    let package =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/pixel-grid");
    app.install_plugin(MediaFile::new(package, "Pixel grid".into(), false));
    settle(&mut app, &mut files, &ctx);
    app.plugin_action(Action::Enable("example.pixel-grid".into()));
    settle(&mut app, &mut files, &ctx);
    app.plugins.open = true;
    let source = root.path().join("texture.pgrid");
    std::fs::write(&source, b"PSTILE 2 2\nff0000 00ff00\n0000ff ffffff\n").unwrap();
    click(&mut app, &mut files, &ctx, "Convert resource");
    app.plugins.providers.dialog.as_mut().unwrap().source = source.to_string_lossy().into_owned();
    click(&mut app, &mut files, &ctx, "Import with plugin");
    settle(&mut app, &mut files, &ctx);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(!app.editor.is_dirty());
    let change = app.workspace.resource_undo.take().unwrap();
    let name = change.name().to_owned();
    let converted = assets.join(&name);
    let bytes = std::fs::read(&converted).unwrap();
    let image = macroquad::texture::Image::from_file_with_format(&bytes, None);
    assert_eq!((image.width, image.height), (2, 2));
    assert_eq!(&image.bytes[..4], &[255, 0, 0, 255]);
    assert_eq!(&image.bytes[12..16], &[255, 255, 255, 255]);
    let package = phistudio_core::prepare_project_export(
        app.editor.document(),
        &app.project_options,
        Some(&assets),
        app.file.as_deref(),
    )
    .unwrap();
    let imported = phistudio_core::import_project(
        package.path(),
        &root.path().join("roundtrip"),
        "Resource export",
        true,
    )
    .unwrap();
    assert_eq!(std::fs::read(imported.assets.join(&name)).unwrap(), bytes);
    app.plugin_action(Action::Disable("example.pixel-grid".into()));
    settle(&mut app, &mut files, &ctx);
    assert!(converted.exists());
    app.workspace_request(crate::workspace_ui::WorkspaceJob::UndoResource(change));
    settle(&mut app, &mut files, &ctx);
    assert!(!converted.exists());
    assert_eq!(app.editor.document().as_json(), &original);
    app.plugins.shutdown().unwrap();
}
