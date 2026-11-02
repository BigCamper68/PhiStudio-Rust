//! Actual editor/prpr GPU frames after repeated inspector resizing and UI scaling.
use crate::{advanced_ui::RightMode, gui::Gui, workbench::Workbench};
use anyhow::{Result, ensure};
use egui::{Id, Rect, containers::panel::PanelState, pos2, vec2};
use macroquad::prelude::*;
use phistudio_core::{ChartDocument, Language, NoteId};
use phistudio_prpr::Preview;
use std::path::PathBuf;
mod audio;
mod audio_notes;
mod line_tools;
mod multiline;
mod noise;
mod note_properties;
mod properties;
mod versions;

fn paint(gui: &mut Gui, app: &mut Workbench, preview: &mut Preview, width: f32) -> Result<Rect> {
    paint_observed(gui, app, preview, width, |_| {})
}
fn paint_observed(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    width: f32,
    observe: impl FnOnce(&egui::Context),
) -> Result<Rect> {
    clear_background(BLACK);
    gui.set_ui_scale(app.options.ui_scale);
    if app.preview.rect.is_positive() {
        let size = app.preview.rect.size() * app.pixels_per_point;
        preview.fit_display([size.x, size.y], app.preview_aspect());
    }
    app.preview.texture = Some(egui::TextureId::User(preview.texture_id()));
    app.preview.size = preview.render_size();
    let mut panel = Rect::NOTHING;
    gui.run(|ctx, canceled| {
        let screen = ctx.screen_rect();
        ctx.data_mut(|d| {
            d.insert_persisted(
                Id::new("context"),
                PanelState {
                    rect: Rect::from_min_size(
                        pos2(screen.right() - width, 40.),
                        vec2(width, screen.height() - 104.),
                    ),
                },
            );
        });
        app.ui(ctx, canceled);
        observe(ctx);
        panel = PanelState::load(ctx, Id::new("context")).unwrap().rect;
        if app.tools.show {
            let zone = ctx.data_mut(|d| {
                d.get_temp::<Vec<crate::scroll::Zone>>(Id::new("phistudio-scroll-zones"))
                    .unwrap_or_default()
                    .into_iter()
                    .find(|z| z.layer != egui::LayerId::background() && z.axis == 1)
            });
            if let Some(zone) = zone
                && let Some(mut state) = egui::scroll_area::State::load(ctx, zone.id)
            {
                state.offset.y = zone.max_offset;
                state.store(ctx, zone.id);
            }
        }
    });
    preview.set_editor_options(&app.options, app.line)?;
    app.sync_noise_preview(preview, &mut None, true)?;
    let time = if app.noise_preview_override().is_some() {
        app.editor.document().tempo().seconds_at(app.beat)
    } else {
        0.
    };
    preview.render(time, preview.target_viewport());
    gui.draw();
    unsafe { macroquad::miniquad::gl::glFinish() };
    Ok(panel)
}

pub async fn run() -> Result<()> {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "artifacts/checks/ui-layout".into()),
    );
    std::fs::create_dir_all(&out)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();
    let document = ChartDocument::from_value(serde_json::from_str(include_str!(
        "../../../assets/charts/phistudio-demo.rpe.json"
    ))?)?;
    let original = document.as_json().clone();
    let mut app = Workbench::new(document, None);
    let mut preview = Preview::new(app.editor.document(), None, &app.project_options).await?;
    app.skin = Some(preview.editor_skin());
    app.note_highlights = preview.highlighted_notes();
    app.select(Some(NoteId { line: 0, index: 0 }));
    let mut gui = Gui::new();
    if std::env::args().nth(2).as_deref() == Some("noise-live") {
        return noise::live(&mut gui, &mut app, &mut preview, &out).await;
    }
    if std::env::args().nth(2).as_deref() == Some("noise-fields") {
        return noise::run(&mut gui, &mut app, &mut preview, &out).await;
    }
    if std::env::args().nth(2).as_deref() == Some("project-versions") {
        return versions::run(&mut gui, &mut app, &mut preview, &out).await;
    }
    if std::env::args().nth(2).as_deref() == Some("audio-notes") {
        return audio_notes::run(&mut gui, &mut app, &mut preview, &out).await;
    }
    if std::env::args().nth(2).as_deref() == Some("note-properties") {
        return note_properties::run(&mut gui, &mut app, &mut preview, &out).await;
    }
    if std::env::args().nth(2).as_deref() == Some("line-tools") {
        return line_tools::run(&mut gui, &mut app, &mut preview, &out).await;
    }
    if std::env::args().nth(2).as_deref() == Some("multiline") {
        return multiline::run(&mut gui, &mut app, &mut preview, &out).await;
    }
    if std::env::args().nth(2).as_deref() == Some("audio") {
        return audio::run(&mut gui, &mut app, &mut preview, &out).await;
    }
    if std::env::args().nth(2).as_deref() == Some("properties") {
        return properties::run(&mut gui, &mut app, &mut preview, &out).await;
    }
    if std::env::args().nth(2).as_deref() == Some("content") {
        return content_gallery(&mut gui, &mut app, &mut preview, &out).await;
    }
    let mut report = vec![];
    for scale in [1., 1.25, 1.5] {
        app.options.ui_scale = scale;
        for language in [Language::Russian, Language::Japanese] {
            app.options.language = language;
            for (mode, label) in [
                (RightMode::Events, "Events"),
                (RightMode::Storyboard, "Storyboard"),
                (RightMode::Shaders, "Shaders"),
            ] {
                app.advanced.right_mode = mode;
                let deadline = get_time() + 0.4;
                while get_time() < deadline {
                    paint(&mut gui, &mut app, &mut preview, 190.)?;
                    next_frame().await;
                }
                paint(&mut gui, &mut app, &mut preview, 190.)?;
                let before = get_screen_data();
                next_frame().await;
                let mut widths = vec![];
                for width in [205., 300., 400., 260., 192., 330., 190.] {
                    for _ in 0..4 {
                        let panel = paint(&mut gui, &mut app, &mut preview, width)?;
                        ensure!(panel.right() <= screen_width() / scale + 0.5);
                        ensure!(app.grid_area.width() >= 200.);
                        widths.push(panel.width());
                        next_frame().await;
                    }
                }
                let deadline = get_time() + 0.4;
                while get_time() < deadline {
                    paint(&mut gui, &mut app, &mut preview, 190.)?;
                    next_frame().await;
                }
                paint(&mut gui, &mut app, &mut preview, 190.)?;
                let after = get_screen_data();
                let name = format!("{language:?}-{scale}-{label}");
                before.export_png(out.join(format!("{name}-before.png")).to_str().unwrap());
                after.export_png(out.join(format!("{name}.png")).to_str().unwrap());
                let changed = before
                    .bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(after.bytes.as_chunks::<4>().0.iter())
                    .filter(|(a, b)| a[..3] != b[..3])
                    .count();
                // Window hover/scrollbar fade may affect a few pixels; persistent
                // mesh corruption or a moving panel affects a substantial area.
                ensure!(
                    changed < before.width as usize * before.height as usize / 1000,
                    "{language:?}/{scale}/{label}: {changed} changed pixels after returning to the same width"
                );
                report.push(serde_json::json!({"case":name,"widths":widths,
                    "grid":[app.grid_area.width(),app.grid_area.height()],
                    "changed_pixels_after_resize":changed}));
                next_frame().await;
            }
        }
    }
    request_new_screen_size(1194., 834.);
    next_frame().await;
    app.options.ui_scale = 1.;
    app.options.language = Language::Russian;
    app.advanced.right_mode = RightMode::Events;
    app.tools.show = true;
    app.tools.page = crate::toolkit::Page::Motion;
    let deadline = get_time() + 0.5;
    while get_time() < deadline {
        paint(&mut gui, &mut app, &mut preview, 260.)?;
        next_frame().await;
    }
    paint(&mut gui, &mut app, &mut preview, 260.)?;
    ensure!(app.error.is_none(), "{:?}", app.error);
    get_screen_data().export_png(out.join("tools-circle.png").to_str().unwrap());
    ensure!(app.editor.document().as_json() == &original && !app.editor.is_dirty());
    std::fs::write(out.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    println!(
        "PASS: native prpr/editor frames, 18 scale/language/workspace cases, 126 resize cycles; chart unchanged"
    );
    Ok(())
}

async fn content_gallery(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &std::path::Path,
) -> Result<()> {
    use phistudio_core::{EventId, EventType, ProjectLibrary, ProjectSummary};
    let original = app.editor.document().as_json().clone();
    let mut cases = vec![];
    let id = EventId {
        line: 0,
        layer: 0,
        kind: EventType::MoveX,
        index: 0,
    };
    for language in Language::ALL {
        app.options.language = language;
        for scale in [0.75, 0.9, 1., 1.1, 1.25, 1.5] {
            app.options.ui_scale = scale;
            app.show_projects = false;
            for width in [190., 350.] {
                app.select_event(Some(id));
                for _ in 0..6 {
                    paint(gui, app, preview, width)?;
                    next_frame().await;
                }
                paint(gui, app, preview, width)?;
                let name = format!("{}-{scale}-{width}-event", language.code());
                if language == Language::Russian || scale == 1. {
                    get_screen_data().export_png(out.join(format!("{name}.png")).to_str().unwrap());
                }
                cases.push(serde_json::json!({"case":name,"logical":[screen_width()/app.pixels_per_point,screen_height()/app.pixels_per_point]}));
                next_frame().await;
            }
            if language != Language::Russian {
                continue;
            }
            app.select_event(None);
            gui.probe_input(
                vec![
                    egui::Event::PointerMoved(pos2(
                        screen_width() / app.pixels_per_point - 30.,
                        100.,
                    )),
                    egui::Event::Scroll(vec2(0., 10000.)),
                ],
                false,
            );
            for _ in 0..6 {
                paint(gui, app, preview, 350.)?;
                next_frame().await;
            }
            paint(gui, app, preview, 350.)?;
            get_screen_data().export_png(
                out.join(format!("ru-{scale}-sidebar.png"))
                    .to_str()
                    .unwrap(),
            );
            next_frame().await;
            gui.probe_input(
                vec![
                    egui::Event::PointerMoved(pos2(
                        screen_width() / app.pixels_per_point - 30.,
                        100.,
                    )),
                    egui::Event::Scroll(vec2(0., -app.preview.rect.height() - 50.)),
                ],
                false,
            );
            for _ in 0..4 {
                paint(gui, app, preview, 350.)?;
                next_frame().await;
            }
            paint(gui, app, preview, 350.)?;
            get_screen_data()
                .export_png(out.join(format!("ru-{scale}-lines.png")).to_str().unwrap());
            next_frame().await;
            app.show_projects = true;
            app.library = Some(ProjectLibrary {
                projects: (0..12)
                    .map(|i| ProjectSummary {
                        workspace: out.join(format!("project-{i}")),
                        name: format!("Project {i} · 中文 / Пример"),
                        source_name: format!("Chart {i}.pez"),
                        notes: 1000 + i,
                        last_opened_ms: 0,
                        cover: None,
                    })
                    .collect(),
                issues: vec![],
            });
            for _ in 0..6 {
                paint(gui, app, preview, 350.)?;
                next_frame().await;
            }
            paint(gui, app, preview, 350.)?;
            get_screen_data().export_png(
                out.join(format!("ru-{scale}-projects.png"))
                    .to_str()
                    .unwrap(),
            );
            next_frame().await;
        }
    }
    app.show_projects = false;
    app.options.language = Language::Russian;
    app.options.ui_scale = 1.5;
    for _ in 0..4 {
        paint(gui, app, preview, 260.)?;
        next_frame().await;
    }
    let at = pos2(85., 20.);
    gui.probe_input(
        vec![
            egui::Event::PointerMoved(at),
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ],
        false,
    );
    for _ in 0..4 {
        paint(gui, app, preview, 260.)?;
        next_frame().await;
    }
    gui.probe_input(
        vec![
            egui::Event::PointerMoved(pos2(100., 120.)),
            egui::Event::Scroll(vec2(0., -10000.)),
        ],
        false,
    );
    for _ in 0..4 {
        paint(gui, app, preview, 260.)?;
        next_frame().await;
    }
    gui.probe_input(
        vec![
            egui::Event::PointerMoved(pos2(100., 120.)),
            egui::Event::Scroll(vec2(0., 50.)),
        ],
        false,
    );
    for _ in 0..4 {
        paint(gui, app, preview, 260.)?;
        next_frame().await;
    }
    paint(gui, app, preview, 260.)?;
    get_screen_data().export_png(out.join("ru-1.5-windows.png").to_str().unwrap());
    ensure!(app.editor.document().as_json() == &original && !app.editor.is_dirty());
    std::fs::write(
        out.join("content-cases.json"),
        serde_json::to_vec_pretty(&cases)?,
    )?;
    println!(
        "PASS: {} native event property layouts, Projects/Preview at all six scales; source unchanged",
        cases.len()
    );
    Ok(())
}
