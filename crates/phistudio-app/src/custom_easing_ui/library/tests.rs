use super::*;
use std::time::Instant;

fn wait(state: &mut State, root: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        state.update(root);
        if state.pending.is_none() && state.sender.is_some() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Library worker timed out: {:?}",
            state.error
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn loaded(root: &Path) -> State {
    let mut state = State::default();
    state.request_load();
    wait(&mut state, root);
    assert!(state.ready, "{:?}", state.error);
    state
}
#[test]
fn private_worker_serializes_rename_replace_import_export_and_persists_independently() {
    let root = tempfile::tempdir().unwrap();
    let mut state = loaded(root.path());
    assert!(!root.path().join("custom-easing/templates.json").exists());
    let curve = Curve::default().insert(0.4).unwrap().0;
    state.send(Action::Save {
        name: "Motion".into(),
        curve: curve.clone(),
        replace: None,
    });
    state.flush().unwrap();
    wait(&mut state, root.path());
    assert!(state.error.is_none());
    let id = state.selected.clone().unwrap();
    state.send(Action::Rename {
        id: id.clone(),
        name: "Named motion".into(),
    });
    wait(&mut state, root.path());
    assert_eq!(state.name, "Named motion");
    let changed = curve.move_nodes(&[1], 0., 0.5).unwrap();
    state.send(Action::Save {
        name: state.name.clone(),
        curve: changed.clone(),
        replace: Some(id.clone()),
    });
    wait(&mut state, root.path());
    state.send(Action::Export(Some(id.clone())));
    wait(&mut state, root.path());
    let export = state.exported.take().unwrap();
    let bytes = std::fs::read(export.path()).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["format"],
        "rpen-animation-curves"
    );
    let parsed = CurveLibrary::parse(&bytes).unwrap();
    assert_eq!(parsed.templates.len(), 1);
    assert_eq!(parsed.templates[0].curve.nodes, changed.nodes);
    let copy = root.path().join("picker-owned");
    std::fs::write(&copy, &bytes).unwrap();
    state
        .import(MediaFile::new(
            copy.clone(),
            "import.rpencurve".into(),
            true,
        ))
        .unwrap();
    state.flush().unwrap();
    wait(&mut state, root.path());
    assert!(!copy.exists(), "The imported picker snapshot leaked");
    assert_eq!(state.prepared.as_ref().unwrap().library.templates.len(), 2);
    assert_eq!(state.name, "Named motion (2)");
    let reopened = loaded(root.path());
    assert_eq!(
        state.prepared.as_ref().unwrap().library,
        reopened.prepared.as_ref().unwrap().library
    );
    state.send(Action::Remove(id));
    wait(&mut state, root.path());
    assert_eq!(state.prepared.as_ref().unwrap().library.templates.len(), 1);
    assert!(state.selected.is_none());
}

#[test]
fn failed_import_and_external_changes_never_overwrite_the_saved_library() {
    let root = tempfile::tempdir().unwrap();
    let mut state = loaded(root.path());
    state.send(Action::Save {
        name: "Keep".into(),
        curve: Curve::default(),
        replace: None,
    });
    wait(&mut state, root.path());
    let path = root.path().join("custom-easing/templates.json");
    let before = std::fs::read(&path).unwrap();
    let source = root.path().join("invalid");
    std::fs::write(&source, b"invalid").unwrap();
    state
        .import(MediaFile::new(source.clone(), "bad.json".into(), true))
        .unwrap();
    wait(&mut state, root.path());
    assert!(state.error.is_some());
    assert!(state.ready);
    assert!(!source.exists());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::write(&path, b"external content").unwrap();
    state.send(Action::Remove(state.selected.clone().unwrap()));
    wait(&mut state, root.path());
    assert!(state.error.as_ref().unwrap().contains("changed on disk"));
    assert_eq!(state.prepared.as_ref().unwrap().library.templates.len(), 1);
    assert_eq!(std::fs::read(&path).unwrap(), b"external content");
    state.send(Action::Reload);
    wait(&mut state, root.path());
    assert!(!state.ready);
    state.send(Action::Save {
        name: "Must not overwrite".into(),
        curve: Curve::default(),
        replace: None,
    });
    wait(&mut state, root.path());
    assert_eq!(std::fs::read(&path).unwrap(), b"external content");
    std::fs::write(&path, before).unwrap();
    state.send(Action::Reload);
    wait(&mut state, root.path());
    assert!(state.ready);
}

#[test]
fn failed_writes_do_not_publish_a_template_or_change_the_previous_file() {
    let root = tempfile::tempdir().unwrap();
    let mut state = loaded(root.path());
    state.send(Action::Save {
        name: "Keep".into(),
        curve: Curve::default(),
        replace: None,
    });
    wait(&mut state, root.path());
    let directory = root.path().join("custom-easing");
    let preserved = root.path().join("preserved-library");
    let before = std::fs::read(directory.join("templates.json")).unwrap();
    std::fs::rename(&directory, &preserved).unwrap();
    std::fs::write(&directory, b"blocked directory").unwrap();
    state.send(Action::Save {
        name: "Retry".into(),
        curve: Curve::default(),
        replace: None,
    });
    wait(&mut state, root.path());
    assert!(state.error.is_some());
    assert_eq!(state.prepared.as_ref().unwrap().library.templates.len(), 1);
    assert_eq!(
        std::fs::read(preserved.join("templates.json")).unwrap(),
        before
    );
    assert_eq!(std::fs::read(&directory).unwrap(), b"blocked directory");
    std::fs::remove_file(&directory).unwrap();
    std::fs::rename(preserved, &directory).unwrap();
    state.send(Action::Save {
        name: "Retry".into(),
        curve: Curve::default(),
        replace: None,
    });
    wait(&mut state, root.path());
    assert!(state.error.is_none());
    assert_eq!(state.prepared.as_ref().unwrap().library.templates.len(), 2);
}

#[test]
fn a_changed_storage_root_rejects_results_from_the_previous_worker() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let mut state = loaded(first.path());
    state.send(Action::Save {
        name: "First storage only".into(),
        curve: Curve::default(),
        replace: None,
    });
    state.flush().unwrap();
    state.update(second.path());
    wait(&mut state, second.path());
    assert!(
        state
            .prepared
            .as_ref()
            .unwrap()
            .library
            .templates
            .is_empty()
    );
    assert!(state.selected.is_none());
    assert!(!second.path().join("custom-easing/templates.json").exists());
}

#[test]
fn thumbnails_retain_hold_jumps_and_narrow_extrema() {
    use phistudio_core::custom_easing::{Mode, Node};
    let narrow = Curve {
        nodes: vec![
            Node::new(0., 0., Mode::Linear),
            Node::new(0.500001, 20., Mode::Hold),
            Node::new(0.500002, -10., Mode::Linear),
            Node::new(1., 1., Mode::Linear),
        ],
        ..Default::default()
    };
    let mut library = CurveLibrary::default();
    library.save("Narrow", narrow, None).unwrap();
    let prepared = Prepared::new(library).unwrap();
    let preview = &prepared.previews[0];
    assert!(preview.range[0] <= -10. && preview.range[1] >= 20.);
    assert!(
        preview
            .points
            .windows(2)
            .any(|p| p[0][0] == p[1][0] && p[0][1] != p[1][1])
    );
}

fn app() -> Workbench {
    use phistudio_core::{ChartDocument, EventType};
    let mut app = Workbench::new(ChartDocument::empty("Template UI", 120.).unwrap(), None);
    let id = EventId {
        line: 0,
        layer: 0,
        kind: EventType::MoveX,
        index: 0,
    };
    app.select_event(Some(id));
    let event = app.editor.document().event(id).unwrap();
    app.open_custom_easing(DraftKey {
        session: app.session,
        revision: app.editor.revision(),
        id,
        times: [event.start_time.to_string(), event.end_time.to_string()],
        event,
    });
    app.custom_easing.insert(0.4);
    app
}
fn frame(
    ctx: &egui::Context,
    app: &mut Workbench,
    events: Vec<egui::Event>,
    size: egui::Vec2,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ctx| app.ui(ctx, false),
    )
}
fn click(ctx: &egui::Context, app: &mut Workbench, label: &str) {
    let size = egui::vec2(1280., 800.);
    let mut output = frame(ctx, app, vec![], size);
    for _ in 0..3 {
        output = frame(ctx, app, vec![], size);
    }
    let pos = output
        .shapes
        .iter()
        .find_map(|shape| {
            if let egui::Shape::Text(text) = &shape.1
                && text.galley.job.text == label
            {
                let center = text.pos + text.galley.size() * 0.5;
                return shape.0.contains(center).then_some(center);
            }
            None
        })
        .unwrap_or_else(|| panic!("Missing visible control {label}"));
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
            size,
        );
    }
}
fn until(
    files: &mut crate::files::FileManager,
    app: &mut Workbench,
    done: impl Fn(&Workbench) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        files.update(app);
        if done(app) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "File/library operation timed out: {:?}, {:?}",
            app.error,
            app.curve_templates.error
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn manager_uses_the_real_file_workflow_and_keeps_chart_history_and_drafts_separate() {
    use crate::{
        files::{FileManager, FileRequest},
        media::MediaKind,
    };
    let root = tempfile::tempdir().unwrap();
    let mut app = app();
    let original = app.editor.document().as_json().clone();
    let source_curve = app.custom_easing.draft.curve.clone();
    let mut files = FileManager::new(Some(root.path().to_owned()));
    files.update(&mut app);
    let ctx = egui::Context::default();
    ctx.set_style(crate::theme::style());
    ctx.set_fonts(crate::fonts::definitions());
    click(&ctx, &mut app, "Templates");
    until(&mut files, &mut app, |a| {
        a.curve_templates.ready && a.curve_templates.pending.is_none()
    });
    app.curve_templates.name = "Stored motion".into();
    click(&ctx, &mut app, "Save current curve");
    until(&mut files, &mut app, |a| {
        a.curve_templates.pending.is_none()
    });
    assert_eq!(
        app.curve_templates
            .prepared
            .as_ref()
            .unwrap()
            .library
            .templates
            .len(),
        1
    );
    click(&ctx, &mut app, "Back to curve");
    let altered = app
        .custom_easing
        .draft
        .curve
        .move_nodes(&[1], 0., 0.4)
        .unwrap();
    app.custom_easing.edit(Ok(altered.clone()));
    click(&ctx, &mut app, "Templates");
    click(&ctx, &mut app, "Use template");
    assert!(!app.custom_easing.library_view);
    assert_eq!(app.custom_easing.draft.curve.nodes, source_curve.nodes);
    assert!(!app.custom_easing.draft.preserve_domain);
    app.custom_easing.history(false);
    assert_eq!(app.custom_easing.draft.curve, altered);
    click(&ctx, &mut app, "Templates");
    click(&ctx, &mut app, "Template actions");
    click(&ctx, &mut app, "Export library…");
    until(&mut files, &mut app, |a| a.files.export_package.is_some());
    let destination = root.path().join("shared.rpencurve");
    app.files.request = Some(FileRequest::WriteExport(destination.clone()));
    until(&mut files, &mut app, |a| {
        destination.exists() && !a.files.busy
    });
    assert_eq!(
        CurveLibrary::parse(&std::fs::read(&destination).unwrap())
            .unwrap()
            .templates
            .len(),
        1
    );
    assert_eq!(app.status, "Curve templates exported");
    click(&ctx, &mut app, "Import templates…");
    assert_eq!(app.files.media, Some(MediaKind::CurveTemplates));
    app.files.media_path = destination.to_string_lossy().into_owned();
    click(&ctx, &mut app, "Use file");
    until(&mut files, &mut app, |a| {
        a.curve_templates
            .prepared
            .as_ref()
            .unwrap()
            .library
            .templates
            .len()
            == 2
            && a.curve_templates.pending.is_none()
    });
    assert_eq!(app.curve_templates.name, "Stored motion (2)");
    assert!(destination.exists());
    app.curve_templates.name = "Renamed import".into();
    click(&ctx, &mut app, "Rename template");
    until(&mut files, &mut app, |a| {
        a.curve_templates.pending.is_none()
    });
    assert_eq!(app.curve_templates.name, "Renamed import");
    click(&ctx, &mut app, "Delete template");
    assert!(app.curve_templates.confirm.is_some());
    click(&ctx, &mut app, "Confirm");
    until(&mut files, &mut app, |a| {
        a.curve_templates.pending.is_none()
    });
    assert_eq!(
        app.curve_templates
            .prepared
            .as_ref()
            .unwrap()
            .library
            .templates
            .len(),
        1
    );
    files.checkpoint(&app, false).unwrap();
    assert_eq!(app.editor.document().as_json(), &original);
    assert!(!app.editor.can_undo());
    assert!(!app.editor.is_dirty());
    assert!(
        !serde_json::to_string(&app.preferences())
            .unwrap()
            .contains("Stored motion")
    );
    let reopened = loaded(root.path());
    assert_eq!(
        reopened.prepared.as_ref().unwrap().library.templates[0].name,
        "Stored motion"
    );
}

#[test]
fn a_project_switch_keeps_global_templates_without_applying_an_old_curve_to_the_new_chart() {
    use phistudio_core::{ChartDocument, EventType};
    let root = tempfile::tempdir().unwrap();
    let mut app = app();
    let mut files = crate::files::FileManager::new(Some(root.path().to_owned()));
    app.curve_templates.request_load();
    until(&mut files, &mut app, |a| a.curve_templates.ready);
    let curve = app.custom_easing.draft.curve.clone();
    app.curve_templates.send(Action::Save {
        name: "Shared shape".into(),
        curve: curve.clone(),
        replace: None,
    });
    let mut raw = ChartDocument::empty("Second project", 120.)
        .unwrap()
        .as_json()
        .clone();
    raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0]["end"] = serde_json::json!(200.);
    app.replace_document(ChartDocument::from_value(raw).unwrap(), None, None);
    let before = app.editor.document().as_json().clone();
    until(&mut files, &mut app, |a| {
        a.curve_templates.pending.is_none()
    });
    assert_eq!(
        app.curve_templates
            .prepared
            .as_ref()
            .unwrap()
            .library
            .templates
            .len(),
        1
    );
    assert_eq!(app.editor.document().as_json(), &before);
    assert!(!app.editor.can_undo());
    let id = EventId {
        line: 0,
        layer: 0,
        kind: EventType::MoveX,
        index: 0,
    };
    let event = app.editor.document().event(id).unwrap();
    let key = DraftKey {
        session: app.session,
        revision: app.editor.revision(),
        id,
        times: [event.start_time.to_string(), event.end_time.to_string()],
        event,
    };
    app.open_custom_easing(key.clone());
    app.custom_easing
        .load_template(app.curve_templates.selected_curve().unwrap());
    app.custom_easing.prepare();
    for _ in 0..500 {
        app.custom_easing.poll();
        if !app.custom_easing.open {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let event = app.custom_easing.take_completed(&key).unwrap();
    assert_eq!([event.start, event.end], [0., 200.]);
    assert_eq!(event.custom_easing.unwrap().curve.nodes, curve.nodes);
    assert_eq!(app.editor.document().as_json(), &before);
}

#[test]
fn localized_library_with_long_template_names_keeps_its_actions_inside_phone_viewports() {
    use phistudio_core::Language;
    let mut source = CurveLibrary::default();
    source
        .save(
            &"A long curve template name ".repeat(6),
            Curve::default(),
            None,
        )
        .unwrap();
    let prepared = Arc::new(Prepared::new(source).unwrap());
    for size in [
        egui::vec2(240., 520.),
        egui::vec2(582., 262.),
        egui::vec2(360., 780.),
        egui::vec2(873., 393.),
    ] {
        for language in Language::ALL {
            let mut app = app();
            app.options.language = language;
            app.custom_easing.library_view = true;
            app.curve_templates.prepared = Some(prepared.clone());
            app.curve_templates.ready = true;
            app.curve_templates.selected = Some(prepared.library.templates[0].id.clone());
            let ctx = egui::Context::default();
            ctx.set_style(crate::theme::style());
            ctx.set_fonts(crate::fonts::definitions());
            for _ in 0..5 {
                frame(&ctx, &mut app, vec![], size);
            }
            let window =
                crate::window::visible_rect(&ctx, egui::Id::new("Custom easing editor")).unwrap();
            let use_button = ctx
                .data_mut(|d| d.get_temp::<egui::Rect>(egui::Id::new("custom-easing-use")))
                .unwrap();
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            assert!(
                viewport.expand(2.).contains_rect(window),
                "{size:?} {language:?}: {window:?}"
            );
            assert!(
                viewport.contains_rect(use_button),
                "{size:?} {language:?}: {use_button:?}"
            );
        }
    }
}

#[test]
fn back_dismisses_library_confirmation_without_deleting_or_loading_a_template() {
    let mut app = app();
    let mut library = CurveLibrary::default();
    let id = library.save("Keep", Curve::default(), None).unwrap();
    app.curve_templates.prepared = Some(Arc::new(Prepared::new(library.clone()).unwrap()));
    app.curve_templates.selected = Some(id.clone());
    app.curve_templates.ready = true;
    app.custom_easing.library_view = true;
    app.curve_templates.confirm = Some(Confirm::Remove {
        id,
        name: "Keep".into(),
    });
    assert!(!app.curve_templates.can_use());
    let before = app.custom_easing.draft.curve.clone();
    app.back();
    assert!(app.custom_easing.open && !app.custom_easing.library_view);
    assert!(app.curve_templates.confirm.is_none());
    assert_eq!(
        app.curve_templates.prepared.as_ref().unwrap().library,
        library
    );
    assert_eq!(app.custom_easing.draft.curve, before);
    app.back();
    assert!(!app.custom_easing.open);
}
