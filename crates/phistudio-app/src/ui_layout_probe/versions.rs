use super::*;
use crate::{
    files::FileManager,
    workspace_ui::{View, repository::Action},
};
use phistudio_core::{Beat, Command, Note};
async fn settle(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    files: &mut FileManager,
) -> Result<()> {
    let until = get_time() + 30.;
    loop {
        files.update(app);
        paint(gui, app, preview, 280.)?;
        next_frame().await;
        ensure!(app.error.is_none(), "Repository worker: {:?}", app.error);
        if !app.file_busy && app.native_request.is_none() {
            return Ok(());
        }
        ensure!(get_time() < until, "Repository worker timeout");
    }
}
pub(super) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &std::path::Path,
) -> Result<()> {
    let root = tempfile::tempdir()?;
    let doc = ChartDocument::empty("Version history", 120.)?;
    let loc = phistudio_core::create_project(root.path(), &doc)?;
    let workspace = loc.assets.clone().unwrap();
    app.replace_document(doc, Some(loc.chart.clone()), Some(workspace.clone()));
    app.options.language = Language::Russian;
    let mut files = FileManager::new(Some(root.path().into()));
    files.update(app);
    app.open_workspace(View::Versions(workspace.clone()));
    settle(gui, app, preview, &mut files).await?;
    app.repository_request(
        workspace.clone(),
        Action::CreateBranch("rhythm".into(), None),
    );
    settle(gui, app, preview, &mut files).await?;
    for (beat, message) in [(1, "Основной ритм"), (3, "Добавлены акценты")]
    {
        app.editor.execute(Command::AddNote {
            line: 0,
            note: Note {
                start_time: Beat::new(beat, 0, 1)?,
                end_time: Beat::new(beat, 0, 1)?,
                x: 0.,
                ..Default::default()
            },
        })?;
        app.repository_request(workspace.clone(), Action::Stage);
        settle(gui, app, preview, &mut files).await?;
        app.repository_request(
            workspace.clone(),
            Action::Commit {
                message: message.into(),
                author: phistudio_core::versions::Author {
                    name: "PhiStudio".into(),
                    email: String::new(),
                },
                amend: false,
            },
        );
        settle(gui, app, preview, &mut files).await?;
    }
    app.repository_request(
        workspace.clone(),
        Action::Tag("v1".into(), None, "Первая версия".into()),
    );
    settle(gui, app, preview, &mut files).await?;
    app.repository_request(workspace.clone(), Action::CheckoutBranch("rhythm".into()));
    settle(gui, app, preview, &mut files).await?;
    app.editor.execute(Command::AddNote {
        line: 0,
        note: Note {
            start_time: Beat::new(2, 0, 1)?,
            end_time: Beat::new(2, 0, 1)?,
            x: 100.,
            ..Default::default()
        },
    })?;
    app.repository_request(workspace.clone(), Action::Stage);
    settle(gui, app, preview, &mut files).await?;
    app.repository_request(
        workspace.clone(),
        Action::Commit {
            message: "Альтернативная партия".into(),
            author: Default::default(),
            amend: false,
        },
    );
    settle(gui, app, preview, &mut files).await?;
    app.repository_request(workspace.clone(), Action::CheckoutBranch("main".into()));
    settle(gui, app, preview, &mut files).await?;
    app.repository_request(workspace.clone(), Action::Merge("rhythm".into()));
    settle(gui, app, preview, &mut files).await?;
    ensure!(
        app.editor.document().note_count() == 3 && app.editor.is_dirty(),
        "Merged working copy is incorrect"
    );
    let saved = phistudio_core::open_project(root.path(), &workspace)?;
    ensure!(
        phistudio_core::load_chart(&saved.charts[0].path)?.note_count() == 0,
        "Checkout overwrote the saved chart"
    );
    preview
        .replace_chart(
            app.editor.document(),
            app.asset_root(),
            &app.project_options,
        )
        .await?;
    ensure!(
        preview.note_count() == 3,
        "Version notes did not reach native preview"
    );
    for language in Language::ALL {
        app.options.language = language;
        for tab in 0..5 {
            app.workspace.repository.tab = tab;
            for _ in 0..3 {
                paint(gui, app, preview, 280.)?;
                next_frame().await;
            }
        }
    }
    app.options.language = Language::Russian;
    app.workspace.repository.tab = 0;
    app.workspace.repository.notice.clear();
    for _ in 0..5 {
        paint(gui, app, preview, 280.)?;
        next_frame().await;
    }
    paint(gui, app, preview, 280.)?;
    let image = get_screen_data();
    ensure!(
        image
            .bytes
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0].max(p[1]).max(p[2]) > 80)
            .count()
            > 500,
        "Version UI capture is blank"
    );
    image.export_png(out.join("project-versions.png").to_str().unwrap());
    app.save(app.file.clone().unwrap());
    ensure!(app.error.is_none() && !app.editor.is_dirty(), "Save failed");
    let saved = phistudio_core::open_project(root.path(), &workspace)?;
    ensure!(
        phistudio_core::load_chart(&saved.charts[0].path)?.note_count() == 3,
        "Save did not publish working copy"
    );
    std::fs::write(
        out.join("project-versions.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"native_notes":3,"saved_before":0,"saved_after":3,"tabs":5,"locales":7,"framebuffer":[screen_width(),screen_height()],"pixels_per_point":app.pixels_per_point}),
        )?,
    )?;
    println!(
        "PASS: native version UI, graph, branches, tags, merge, all five tabs and seven locales, deferred Save"
    );
    Ok(())
}
