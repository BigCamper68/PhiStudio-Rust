//! Real editor integration/GL timing, isolated from Preview rendering and vsync.
//! Run the note-grid-performance example on a private display.
mod reference;
use crate::{
    grid_input::Grid,
    gui::Gui,
    note_index::NoteIndex,
    timeline::{self, NoteBatch},
};
use anyhow::{Result, ensure};
use macroquad::prelude::*;
use phistudio_core::{Beat, ChartDocument, Note, NoteKind, ProjectOptions};
use phistudio_prpr::{EditorSkin, Preview};
use std::{path::PathBuf, time::Instant};

fn notes(count: usize, dense: bool, decorated: bool) -> Vec<(usize, Note)> {
    (0..count)
        .map(|i| {
            let kind = [
                NoteKind::Tap,
                NoteKind::Drag,
                NoteKind::Flick,
                NoteKind::Hold,
            ][i % 4];
            // Deliberately unsorted input, equal times, overlaps and long Holds.
            let start = if dense {
                1. + ((i * 37) % 512) as f64 / 256.
            } else {
                ((i * 7919) % count) as f64 * 0.5
            };
            let start = Beat::from_grid(start, 1024).unwrap();
            let end = if kind == NoteKind::Hold {
                start
                    .checked_add(Beat::new(if dense { 0 } else { 40 }, 1, 4).unwrap())
                    .unwrap()
            } else {
                start
            };
            (
                i,
                Note {
                    kind,
                    start_time: start,
                    end_time: end,
                    x: ((i * 131) % 1100) as f64 - 550.,
                    is_fake: if decorated && i % 7 == 0 { 1 } else { 0 },
                    above: if decorated && i % 11 == 0 { 0 } else { 1 },
                    ..Note::default()
                },
            )
        })
        .collect()
}
fn grid(ctx: &egui::Context, compact: bool, beat: f64) -> Grid {
    Grid {
        rect: egui::Rect::from_min_size(
            egui::pos2(40., 40.),
            if compact {
                egui::vec2(320., 180.)
            } else {
                egui::vec2(800., 620.)
            },
        ),
        event_rect: egui::Rect::NOTHING,
        beat,
        zoom: if compact { 32. } else { 96. },
        division: 4,
        lanes: 11,
        snap_x_enabled: false,
        scroll_speed: 1.,
        note_width: 46.,
        camera_x: 0.,
    }
    .with_screen(ctx.screen_rect())
}
impl Grid {
    fn with_screen(mut self, screen: egui::Rect) -> Self {
        self.rect = self.rect.intersect(screen.shrink(2.));
        self
    }
}
struct View {
    compact: bool,
    beat: f64,
    decorated: bool,
}
fn paint(
    ctx: &egui::Context,
    notes: &[(usize, Note)],
    index: &NoteIndex,
    skin: &EditorSkin,
    optimized: bool,
    view: &View,
) {
    let View {
        compact,
        beat,
        decorated,
    } = *view;

    let grid = grid(ctx, compact, beat);
    let p = ctx
        .layer_painter(egui::LayerId::background())
        .with_clip_rect(grid.rect);
    p.rect_filled(grid.rect, 0., crate::theme::base());
    if optimized {
        let mut batch = NoteBatch::new(&p);
        for i in index.query(
            grid.raw_beat(grid.rect.bottom() + 64.),
            grid.raw_beat(grid.rect.top() - 64.),
        ) {
            let (_, n) = &notes[i];
            let s = if i % 3 == 0 {
                skin.grid
                    .as_ref()
                    .map_or(&skin.highlighted, |s| &s.highlighted)
            } else {
                skin.grid.as_ref().map_or(&skin.normal, |s| &s.normal)
            };
            timeline::draw_note(
                &mut batch,
                &grid,
                n,
                decorated && i % 137 == 0,
                false,
                Some(s),
            );
        }
        batch.flush();
    } else {
        for (i, n) in notes {
            let s = if i % 3 == 0 {
                &skin.highlighted
            } else {
                &skin.normal
            };
            reference::draw_note(&p, &grid, n, decorated && i % 137 == 0, false, Some(s));
        }
    }
}
fn frame(
    gui: &mut Gui,
    notes: &[(usize, Note)],
    index: &NoteIndex,
    skin: &EditorSkin,
    optimized: bool,
    view: &View,
) -> f64 {
    clear_background(Color::from_rgba(24, 23, 25, 255));
    let begin = Instant::now();
    gui.run(|ctx, _| paint(ctx, notes, index, skin, optimized, view));
    gui.draw();
    unsafe {
        macroquad::miniquad::gl::glFinish();
    }
    begin.elapsed().as_secs_f64() * 1000.
}
fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}
pub async fn run() -> Result<()> {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "artifacts/checks/note-grid-performance".into()),
    );
    std::fs::create_dir_all(&out)?;
    let rt = tokio::runtime::Runtime::new()?;
    let _guard = rt.enter();
    let mut preview = Preview::new(
        &ChartDocument::empty("Grid probe", 120.)?,
        None,
        &ProjectOptions::default(),
    )
    .await?;
    let mut gui = Gui::new();
    let mut report = vec![];
    // Optional original custom skin directory, including native repeating Holds.
    let custom = std::env::args().nth(2).map(PathBuf::from);
    for pack in ["official", "custom"] {
        if pack == "custom" {
            if let Some(p) = &custom {
                preview.set_skin(Some(p)).await?;
            } else {
                continue;
            }
        }
        let skin = preview.editor_skin();
        for (name, count, dense, decorated, compact, scale) in [
            ("dense", 8000, true, false, false, 1.),
            ("huge-mesh", 19000, true, false, false, 1.),
            ("decorated", 2000, true, true, false, 1.),
            ("sparse", 100000, false, false, false, 1.),
            ("compact", 8000, true, false, true, 1.),
            ("density-2.75", 8000, true, false, true, 2.75),
        ] {
            let notes = notes(count, dense, decorated);
            let index = NoteIndex::new(&notes);
            gui.set_ui_scale(scale);
            let view = View {
                compact,
                beat: 0.,
                decorated,
            };
            let cpu_ctx = egui::Context::default();
            cpu_ctx.set_fonts(crate::fonts::definitions());
            let mut cpu = vec![];
            for optimized in [false, true] {
                let mut values = vec![];
                let mut shapes = 0;
                let mut jobs = 0;
                for pass in 0..8 {
                    let begin = Instant::now();
                    let out = cpu_ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1280. / scale, 800. / scale),
                            )),
                            pixels_per_point: Some(scale),
                            ..Default::default()
                        },
                        |ctx| paint(ctx, &notes, &index, &skin, optimized, &view),
                    );
                    shapes = out.shapes.len();
                    jobs = cpu_ctx.tessellate(out.shapes).len();
                    if pass >= 2 {
                        values.push(begin.elapsed().as_secs_f64() * 1000.);
                    }
                }
                cpu.push(serde_json::json!({"ms":median(values),"shapes":shapes,"draw_jobs":jobs}));
            }
            let mut before = None;
            let mut timings = [0.; 2];
            let mut compared = 0;
            let mut changed = 0;
            let mut max_error = 0;
            for optimized in [false, true] {
                let mut elapsed = vec![];
                for pass in 0..8 {
                    let ms = frame(&mut gui, &notes, &index, &skin, optimized, &view);
                    if pass >= 2 {
                        elapsed.push(ms);
                    }
                    next_frame().await;
                }
                timings[usize::from(optimized)] = median(elapsed);
                frame(&mut gui, &notes, &index, &skin, optimized, &view);
                let image = get_screen_data();
                image.export_png(
                    &out.join(format!(
                        "{pack}-{name}-{}.png",
                        if optimized { "after" } else { "before" }
                    ))
                    .to_string_lossy(),
                );
                if let Some(reference) = before.take() {
                    let reference: Image = reference;
                    ensure!(reference.width == image.width && reference.height == image.height);
                    // Interior comparison avoids the old painter's early rejection
                    // of partially visible caps, deliberately corrected by the fix.
                    let ppp = scale;
                    let width = if compact { 320. } else { 800. };
                    let height = if compact { 180. } else { 620. };
                    for y in ((40. + 30.) * ppp) as usize
                        ..((40. + height - 30.) * ppp).min(image.height as f32) as usize
                    {
                        for x in (40. * ppp) as usize
                            ..((40. + width) * ppp).min(image.width as f32) as usize
                        {
                            let p = (y * image.width as usize + x) * 4;
                            let err = (0..3)
                                .map(|c| reference.bytes[p + c].abs_diff(image.bytes[p + c]))
                                .max()
                                .unwrap();
                            compared += 1;
                            changed += usize::from(err != 0);
                            max_error = max_error.max(err);
                        }
                    }
                    ensure!(
                        max_error <= 3,
                        "{pack}/{name}: RGB error {max_error}, {changed}/{compared} changed pixels"
                    );
                } else {
                    before = Some(image);
                }
                next_frame().await;
            }
            let entry = serde_json::json!({"pack":pack,"case":name,"notes":count,"before_ms":timings[0],"after_ms":timings[1],"speedup":timings[0]/timings[1],"compared_pixels":compared,"different_pixels":changed,"max_channel_error":max_error,"atlas":skin.grid.is_some(),"cpu_before":cpu[0],"cpu_after":cpu[1]});
            println!("{entry}");
            report.push(entry);
        }
    }
    std::fs::write(out.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    material_check(&mut gui, &out).await?;
    Ok(())
}

async fn material_check(gui: &mut Gui, out: &std::path::Path) -> Result<()> {
    let colors = [
        [255, 0, 0, 128],
        [0, 0, 255, 128],
        [255, 255, 0, 128],
        [255, 0, 255, 255],
    ];
    let textures: Vec<_> = colors
        .into_iter()
        .map(|c| Texture2D::from_rgba8(2, 2, &c.repeat(4)))
        .collect();
    let user = |i: usize| {
        egui::TextureId::User(textures[i].raw_miniquad_texture_handle().gl_internal_id() as u64)
    };
    let mut green = None;
    let mut white = None;
    let mut scale = 1.;
    for pass in 0..4 {
        clear_background(BLACK);
        gui.run(|ctx, _| {
            scale = ctx.pixels_per_point();
            let green = green.get_or_insert_with(|| {
                ctx.load_texture(
                    "batch-green",
                    egui::ColorImage::new(
                        [2, 2],
                        egui::Color32::from_rgba_premultiplied(0, 128, 0, 128),
                    ),
                    egui::TextureOptions::LINEAR,
                )
            });
            let white = white.get_or_insert_with(|| {
                ctx.load_texture(
                    "batch-white",
                    egui::ColorImage::new(
                        [2, 2],
                        egui::Color32::from_rgba_premultiplied(128, 128, 128, 128),
                    ),
                    egui::TextureOptions::LINEAR,
                )
            });
            let p = ctx.layer_painter(egui::LayerId::background());
            let rect = egui::Rect::from_min_size(egui::pos2(30., 30.), egui::vec2(100., 100.));
            let uv = egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.));
            for id in [user(0), green.id(), user(1), white.id(), user(2)] {
                p.image(id, rect, uv, egui::Color32::WHITE);
            }
            p.add(egui::Shape::Callback(egui::PaintCallback {
                rect,
                callback: std::sync::Arc::new(egui_miniquad::CallbackFn::new(|_, ctx| {
                    ctx.apply_viewport(0, 0, 1, 1);
                    ctx.apply_scissor_rect(0, 0, 1, 1);
                })),
            }));
            p.with_clip_rect(egui::Rect::from_min_size(
                egui::pos2(160., 30.),
                egui::vec2(50., 100.),
            ))
            .image(
                user(3),
                rect.translate(egui::vec2(130., 0.)),
                uv,
                egui::Color32::WHITE,
            );
            p.rect_filled(
                rect.translate(egui::vec2(270., 0.)),
                0.,
                egui::Color32::WHITE,
            );
        });
        gui.draw();
        unsafe {
            macroquad::miniquad::gl::glFinish();
        }
        if pass < 3 {
            next_frame().await;
        }
    }
    let image = get_screen_data();
    image.export_png(&out.join("material-pipeline.png").to_string_lossy());
    let pixel = |x: f32, y: f32| {
        let p = ((image.height as usize - 1 - (y * scale) as usize) * image.width as usize
            + (x * scale) as usize)
            * 4;
        [image.bytes[p], image.bytes[p + 1], image.bytes[p + 2]]
    };
    for (x, y, expected) in [
        (80., 80., [200, 208, 96]),
        (180., 80., [255, 0, 255]),
        (240., 80., [0, 0, 0]),
        (320., 80., [255, 255, 255]),
    ] {
        let actual = pixel(x, y);
        ensure!(
            actual
                .into_iter()
                .zip(expected)
                .all(|(a, b)| a.abs_diff(b) <= 1),
            "Material/scissor/callback pixel {x}/{y}: {actual:?}, expected {expected:?}"
        );
    }
    println!("Mixed alpha, four texture slots, texture overflow, scissor and callback PASS");
    Ok(())
}

/// Full editor frames using the real text generator, including its automatic
/// multiselection, Fake markers, properties and native Preview.
pub async fn run_text() -> Result<()> {
    use crate::workbench::Workbench;
    use phistudio_core::{
        text_composition::{self, Mode, Spec},
        tools::Selection,
    };
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "artifacts/checks/text-note-performance".into()),
    );
    std::fs::create_dir_all(&out)?;
    let reference = std::env::args().nth(2).map(PathBuf::from);
    let rt = tokio::runtime::Runtime::new()?;
    let _guard = rt.enter();
    let mut preview = Preview::new(
        &ChartDocument::empty("Text notes", 120.)?,
        None,
        &ProjectOptions::default(),
    )
    .await?;
    let skin = preview.editor_skin();
    let mut gui = Gui::new();
    let mut report = vec![];
    for (mode, target) in [
        (Mode::Tap, 200),
        (Mode::Drag, 200),
        (Mode::Flick, 200),
        (Mode::Tap, 1000),
    ] {
        let mut app = Workbench::new(ChartDocument::empty("Generated text notes", 120.)?, None);
        let mut low = 1.;
        let mut high = 100.;
        let mut best = None;
        for _ in 0..16 {
            let spacing = (low + high) / 2.;
            let plan = text_composition::prepare(
                Spec {
                    mode,
                    spacing,
                    ..Spec::default()
                },
                phistudio_prpr::TEXT_FONT_BYTES,
                app.editor.document().tempo(),
            )?;
            let count = plan.notes;
            if best.as_ref().is_none_or(|p: &text_composition::Plan| {
                p.notes.abs_diff(target) > count.abs_diff(target)
            }) {
                best = Some(plan);
            }
            if count > target {
                low = spacing;
            } else {
                high = spacing;
            }
        }
        let plan = best.unwrap();
        let count = plan.notes;
        ensure!(
            count.abs_diff(target) <= 20,
            "Could not prepare {target}-note fixture: {count}"
        );
        let selection = plan.apply(&mut app.editor)?;
        app.line = selection.notes[0].line;
        app.transport.beat = 2.;
        app.skin = Some(skin.clone());
        preview
            .replace_chart(app.editor.document(), None, &app.project_options)
            .await?;
        preview.set_editor_options(&app.options, app.line)?;
        app.note_highlights = preview.highlighted_notes();
        app.preview.texture = Some(egui::TextureId::User(preview.texture_id()));
        std::fs::write(
            out.join(format!("{mode:?}-{count}.rpe.json")),
            serde_json::to_vec_pretty(app.editor.document().as_json())?,
        )?;
        let original = app.editor.document().as_json().clone();
        let revision = app.editor.revision();
        for selected in [true, false] {
            app.set_selection(if selected {
                selection.clone()
            } else {
                Selection::default()
            });
            let mut ui_times = vec![];
            let mut gpu_times = vec![];
            let mut preview_times = vec![];
            let mut changed_pixels = 0usize;
            let mut max_channel_error = 0u8;
            for pass in 0..12 {
                clear_background(Color::from_rgba(20, 27, 42, 255));
                gui.set_ui_scale(1.);
                let begin = Instant::now();
                gui.run(|ctx, canceled| app.ui(ctx, canceled));
                let ui_ms = begin.elapsed().as_secs_f64() * 1000.;
                let begin = Instant::now();
                preview.render(
                    app.editor.document().tempo().seconds_at(app.transport.beat),
                    preview.target_viewport(),
                );
                unsafe {
                    macroquad::miniquad::gl::glFinish();
                }
                let preview_ms = begin.elapsed().as_secs_f64() * 1000.;
                let begin = Instant::now();
                gui.draw();
                unsafe {
                    macroquad::miniquad::gl::glFinish();
                }
                let gpu_ms = begin.elapsed().as_secs_f64() * 1000.;
                if pass >= 4 {
                    ui_times.push(ui_ms);
                    gpu_times.push(gpu_ms);
                    preview_times.push(preview_ms);
                }
                if pass == 11 {
                    let image = get_screen_data();
                    let name = format!("{mode:?}-{count}-{selected}.png");
                    image.export_png(&out.join(&name).to_string_lossy());
                    if let Some(reference) = &reference {
                        let before = Image::from_file_with_format(
                            &std::fs::read(reference.join(&name))?,
                            None,
                        );
                        ensure!(
                            before.width == image.width && before.height == image.height,
                            "Screenshot dimensions changed"
                        );
                        let after =
                            Image::from_file_with_format(&std::fs::read(out.join(&name))?, None);
                        for (a, b) in before
                            .bytes
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .zip(after.bytes.as_chunks::<4>().0.iter())
                        {
                            let error = (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap();
                            max_channel_error = max_channel_error.max(error);
                            changed_pixels += usize::from(error != 0);
                        }
                        ensure!(
                            max_channel_error <= 1,
                            "{name}: {changed_pixels} changed pixels, RGB error {max_channel_error}"
                        );
                    }
                }
                next_frame().await;
            }
            let stats = gui.paint_stats();
            let entry = serde_json::json!({"mode":format!("{mode:?}"),"notes":count,"selected":selected,"pixels_per_point":app.pixels_per_point,"ui_ms":median(ui_times),"gui_gpu_ms":median(gpu_times),"preview_ms":median(preview_times),"draw_calls":stats.draw_calls,"buffer_uploads":stats.buffer_uploads,"vertices":stats.vertices,"changed_pixels":changed_pixels,"max_channel_error":max_channel_error});
            println!("{entry}");
            report.push(entry);
            ensure!(
                *app.editor.document().as_json() == original && app.editor.revision() == revision,
                "Profiling edited the generated chart"
            );
        }
    }
    std::fs::write(out.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    material_check(&mut gui, &out).await?;
    Ok(())
}
