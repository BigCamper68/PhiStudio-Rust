use super::*;
use crate::{
    files::{FileManager, FileRequest},
    media::MediaKind,
};
use phistudio_core::{EditorSoundPack, Language};
use std::time::{Duration, Instant};
fn frame(
    ctx: &egui::Context,
    app: &mut Workbench,
    events: Vec<egui::Event>,
    width: f32,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 800.),
            )),
            events,
            ..Default::default()
        },
        |ctx| app.ui(ctx, false),
    )
}
fn click(ctx: &egui::Context, app: &mut Workbench, label: &str) {
    let mut output = frame(ctx, app, Vec::new(), 1280.);
    for _ in 0..2 {
        output = frame(ctx, app, Vec::new(), 1280.);
    }
    let pos = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.1 {
            egui::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing visible control: {label}"));
    for pressed in [true, false] {
        frame(
            ctx,
            app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
            1280.,
        );
    }
}
#[test]
fn manager_buttons_create_import_export_and_persist_a_portable_pack_without_chart_edits() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app();
    let original = app.editor.document().as_json().clone();
    let mut files = FileManager::new(Some(root.path().to_owned()));
    files.update(&mut app);
    app.sounds.open = true;
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::fonts::definitions());
    click(&ctx, &mut app, "Manage sound packs");
    click(&ctx, &mut app, "New sound pack");
    assert_eq!(app.options.editor_sounds.packs.len(), 1);
    click(&ctx, &mut app, "Duplicate sound pack");
    assert_eq!(app.options.editor_sounds.packs.len(), 2);
    let source = archive::export(EditorSoundPack {
        name: "Imported portable pack".into(),
        ..Default::default()
    })
    .unwrap();
    click(&ctx, &mut app, "Import sound pack…");
    assert_eq!(app.files.media, Some(MediaKind::SoundPack));
    app.files.media_path = source.path().to_string_lossy().into_owned();
    click(&ctx, &mut app, "Use file");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        files.update(&mut app);
        if app.options.editor_sounds.packs.len() == 3 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Pack import did not complete: {:?}",
            app.error
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        app.options.editor_sounds.pack().name,
        "Imported portable pack"
    );
    click(&ctx, &mut app, "Export sound pack…");
    while app.files.export_package.is_none() {
        frame(&ctx, &mut app, Vec::new(), 1280.);
        assert!(
            Instant::now() < deadline,
            "Pack export did not complete: {:?}",
            app.error
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let destination = root.path().join("shared.phisounds.json");
    app.files.request = Some(FileRequest::WriteExport(destination.clone()));
    while !destination.exists() || app.files.busy {
        files.update(&mut app);
        assert!(
            Instant::now() < deadline,
            "Export was not written: {:?}",
            app.error
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let imported = archive::import(&destination, &root.path().join("other-device")).unwrap();
    assert_eq!(imported.name, "Imported portable pack");
    files.checkpoint(&app, false).unwrap();
    assert_eq!(
        phistudio_core::load_editor_preferences(root.path())
            .unwrap()
            .options
            .editor_sounds,
        app.options.editor_sounds
    );
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(!app.editor.is_dirty());
    assert!(!app.editor.can_undo());
}
#[test]
fn a_long_pack_name_and_localized_manager_fit_a_phone_without_changing_preferences() {
    for language in [Language::English, Language::Russian] {
        let mut app = app();
        app.options.language = language;
        app.options.editor_sounds.add_pack(EditorSoundPack {
            name: "A very long sound pack name for a custom collection of editor sounds".into(),
            ..Default::default()
        });
        app.sounds.open = true;
        let original = app.preferences();
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::definitions());
        for _ in 0..4 {
            frame(&ctx, &mut app, Vec::new(), 320.);
        }
        let rect =
            crate::window::visible_rect(&ctx, egui::Id::new("Editor Sounds Manager")).unwrap();
        assert!(rect.right() <= 321., "{language:?}: {rect:?}");
        assert_eq!(
            app.preferences(),
            original,
            "Viewing a sound pack must not create rule overrides"
        );
    }
}
