use super::*;
fn run(files: &mut crate::files::FileManager, app: &mut Workbench, job: WorkspaceJob) {
    app.workspace_request(job);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    files.update(app);
    while app.file_busy || app.workspace.loading {
        assert!(
            std::time::Instant::now() < deadline,
            "file worker timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        files.update(app);
    }
    assert!(app.error.is_none(), "{:?}", app.error);
}
#[test]
fn worker_integration_stages_commits_checks_out_unsaved_work_and_publishes_only_on_save() {
    let root = tempfile::tempdir().unwrap();
    let doc = ChartDocument::empty("Worker", 120.).unwrap();
    let loc = core::create_project(root.path(), &doc).unwrap();
    let ws = loc.assets.clone().unwrap();
    let mut app = Workbench::new(doc, Some(loc.chart.clone()));
    let mut files = crate::files::FileManager::new(Some(root.path().into()));
    files.update(&mut app);
    app.editor
        .execute(core::Command::AddNote {
            line: 0,
            note: core::Note::default(),
        })
        .unwrap();
    app.set_object_selection(
        core::tools::Selection {
            notes: vec![core::NoteId { line: 0, index: 0 }],
            events: vec![],
        },
        vec![],
    );
    let raw = app.editor.document().as_json().clone();
    let clip = ObjectClipboard::copy(app.editor.document(), &app.selection(), &[]).unwrap();
    run(
        &mut files,
        &mut app,
        WorkspaceJob::SavePattern {
            name: "Reusable".into(),
            clipboard: Arc::new(clip),
        },
    );
    assert_eq!(app.workspace.patterns.len(), 1);
    let id = app.workspace.patterns[0].id.clone();
    run(
        &mut files,
        &mut app,
        WorkspaceJob::InsertPattern {
            id,
            settings: Insertion {
                line: 1,
                layer: 0,
                beat: Beat::new(4, 0, 1).unwrap(),
                scale: Beat::ONE,
                offset: [0.; 2],
                mirrored: false,
            },
        },
    );
    assert_eq!(app.editor.document().line_count(), 2);
    app.editor.undo();
    assert_eq!(app.editor.document().as_json(), &raw);
    assert!(app.editor.is_dirty());
    use super::repository::Action;
    app.workspace.view = Some(View::Versions(ws.clone()));
    let job = |action| WorkspaceJob::Repository {
        workspace: ws.clone(),
        revision: None,
        action,
    };
    run(&mut files, &mut app, job(Action::Refresh));
    let initial = app
        .workspace
        .repository
        .status
        .as_ref()
        .unwrap()
        .state
        .head_id()
        .unwrap()
        .to_owned();
    app.editor
        .execute(core::Command::AddNote {
            line: 0,
            note: core::Note::default(),
        })
        .unwrap();
    run(&mut files, &mut app, job(Action::Stage));
    run(
        &mut files,
        &mut app,
        job(Action::Commit {
            message: "Two notes".into(),
            author: Default::default(),
            amend: false,
        }),
    );
    assert_eq!(core::load_chart(&loc.chart).unwrap().note_count(), 0);
    assert!(app.editor.is_dirty() && app.editor.can_undo());
    let session = app.session;
    run(&mut files, &mut app, job(Action::CheckoutCommit(initial)));
    assert_eq!(app.editor.document().as_json(), &raw);
    assert!(app.editor.is_dirty() && !app.editor.can_undo());
    assert!(app.session > session);
    assert_eq!(app.workspace.view, Some(View::Versions(ws.clone())));
    let project = core::open_project(root.path(), &ws).unwrap();
    assert_eq!(
        core::load_chart(&project.charts[0].path)
            .unwrap()
            .note_count(),
        0
    );
    let file = app.file.clone().unwrap();
    app.save(file);
    assert!(app.error.is_none() && !app.editor.is_dirty());
    let project = core::open_project(root.path(), &ws).unwrap();
    assert_eq!(
        core::load_chart(&project.charts[0].path)
            .unwrap()
            .note_count(),
        1
    );
    assert_eq!(app.workspace.patterns.len(), 1);
}
#[test]
fn library_windows_fit_phone_and_tablet_and_back_closes_them() {
    for size in [egui::vec2(873., 393.), egui::vec2(1194., 834.)] {
        let mut app = Workbench::new(ChartDocument::empty("Window", 120.).unwrap(), None);
        let ctx = egui::Context::default();
        for view in [
            View::Patterns,
            View::Resources,
            View::Versions("test".into()),
            View::Project("test".into()),
        ] {
            app.workspace.view = Some(view);
            for _ in 0..3 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| app.ui(ctx, false),
                );
            }
            let area = app.workspace.rect;
            assert!(
                area.left() >= 0.
                    && area.top() >= 0.
                    && area.right() <= size.x + 1.
                    && area.bottom() <= size.y + 1.,
                "{size:?}: {area:?}"
            );
            assert!(app.modal_open());
            app.back();
            assert!(app.workspace.view.is_none());
        }
    }
}

#[test]
fn repository_forms_keep_text_focus_when_status_changes_and_fit_all_locales() {
    use phistudio_core::vcs::{Repository, Working};
    let root = tempfile::tempdir().unwrap();
    let doc = ChartDocument::empty("Forms", 120.).unwrap();
    let loc = core::create_project(root.path(), &doc).unwrap();
    let ws = loc.assets.clone().unwrap();
    let repo = Repository::open(
        root.path(),
        &ws,
        &Working {
            document: &doc,
            location: &loc,
            options: &Default::default(),
        },
    )
    .unwrap();
    let status = repo
        .status(&Working {
            document: &doc,
            location: &loc,
            options: &Default::default(),
        })
        .unwrap();
    let mut app = Workbench::new(doc, Some(loc.chart));
    app.workspace.root = Some(root.path().into());
    app.workspace.view = Some(View::Versions(ws));
    app.accept_repository(Box::new(status));
    app.workspace.repository.tab = 1;
    app.workspace.repository.message = "Draft".into();
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::fonts::definitions());
    let render = |app: &mut Workbench, events: Vec<egui::Event>, size: egui::Vec2| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| app.ui(ctx, false),
        )
    };
    let size = egui::vec2(873., 393.);
    for _ in 0..4 {
        render(&mut app, vec![], size);
    }
    assert!(ctx.fonts(|f| f.has_glyph(&egui::FontId::proportional(16.), '↻')));
    let out = render(&mut app, vec![], size);
    let at = out
        .shapes
        .iter()
        .find_map(|s| match &s.1 {
            egui::Shape::Text(t) if t.galley.job.text == "Draft" => {
                Some(t.pos + t.galley.size() * 0.5)
            }
            _ => None,
        })
        .expect("Message text missing");
    let pointer = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    render(
        &mut app,
        vec![egui::Event::PointerMoved(at), pointer(true)],
        size,
    );
    render(&mut app, vec![pointer(false)], size);
    let focused = ctx
        .memory(|m| m.focus())
        .expect("Text field did not take focus");
    render(&mut app, vec![egui::Event::Text("1".into())], size);
    let first = app.workspace.repository.message.clone();
    assert!(first.contains('1'));
    app.workspace.repository.notice = "Commit created; Save writes the project separately".into();
    app.error = Some("Repository changed; refresh before retrying".into());
    render(&mut app, vec![egui::Event::Text("2".into())], size);
    assert_eq!(ctx.memory(|m| m.focus()), Some(focused));
    assert!(app.workspace.repository.message.contains("12"));
    app.error = None;
    for language in core::Language::ALL {
        app.options.language = language;
        for size in [
            egui::vec2(873., 393.),
            egui::vec2(582., 262.),
            egui::vec2(1280., 800.),
        ] {
            for tab in 0..5 {
                app.workspace.repository.tab = tab;
                for _ in 0..3 {
                    render(&mut app, vec![], size);
                }
                let rect = app.workspace.rect;
                assert!(
                    rect.left() >= 0.
                        && rect.right() <= size.x + 1.
                        && rect.top() >= 0.
                        && rect.bottom() <= size.y + 1.,
                    "{} tab {tab}: {rect:?} in {size:?}",
                    language.code()
                );
            }
        }
    }
}
#[test]
fn checking_out_another_projects_version_keeps_the_existing_save_discard_decision() {
    use super::repository::Action;
    let root = tempfile::tempdir().unwrap();
    let doc = ChartDocument::empty("Current", 120.).unwrap();
    let loc = core::create_project(root.path(), &doc).unwrap();
    let other =
        core::create_project(root.path(), &ChartDocument::empty("Other", 120.).unwrap()).unwrap();
    let ws = other.assets.clone().unwrap();
    let mut app = Workbench::new(doc, Some(loc.chart.clone()));
    let mut files = crate::files::FileManager::new(Some(root.path().into()));
    files.update(&mut app);
    app.editor
        .execute(core::Command::AddNote {
            line: 0,
            note: core::Note::default(),
        })
        .unwrap();
    let before = app.editor.document().as_json().clone();
    app.workspace.view = Some(View::Versions(ws.clone()));
    let job = |action| WorkspaceJob::Repository {
        workspace: ws.clone(),
        revision: None,
        action,
    };
    run(&mut files, &mut app, job(Action::Refresh));
    let initial = app
        .workspace
        .repository
        .status
        .as_ref()
        .unwrap()
        .state
        .head_id()
        .unwrap()
        .to_owned();
    run(&mut files, &mut app, job(Action::CheckoutCommit(initial)));
    assert_eq!(app.file.as_ref(), Some(&loc.chart));
    assert_eq!(app.editor.document().as_json(), &before);
    assert!(app.editor.is_dirty());
    let pending = app.pending_open.clone().expect("Save/Discard was bypassed");
    app.discard_and_open(pending);
    assert_eq!(app.editor.document().name(), "Other");
    assert!(app.editor.is_dirty());
}
