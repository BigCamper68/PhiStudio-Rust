//! Actual Noise Field workspace, animation inspector and shared palette frames.
use super::*;
use phistudio_core::{
    Beat, Command,
    noise_fields::{self as noise, Action, Format, Track},
};
use serde_json::json;
use std::path::Path;
pub(super) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &Path,
) -> Result<()> {
    let source = app.editor.document().as_json().clone();
    let start = Beat::new(-4, 0, 1)?;
    let end = Beat::new(16, 0, 1)?;
    let mut native = noise::create(
        start,
        end,
        Format::Native,
        false,
        app.editor.document().tempo(),
    );
    native["phiStudioName"] = json!("Native field");
    native[noise::COLOR] = json!([40, 180, 255]);
    native["scaleEvents"] = json!([{"time":0.,"scale":{"x":1.,"y":1.},"anchor":{"x":0.5,"y":0.5},"easeTypeX":1,"easeTypeY":2},{"time":3.,"scale":{"x":0.8,"y":0.4},"anchor":{"x":0.5,"y":0.5}}]);
    let mut next = noise::create(
        start,
        end,
        Format::Next,
        false,
        app.editor.document().tempo(),
    );
    next["Name"] = json!("rpe-next field");
    next["activeIntervals"] = json!([{"startTime":[4,0,1],"endTime":[12,0,1],"readyFlash":0.75}]);
    next["moveXEvents"] =
        json!([{"startTime":[0,0,1],"endTime":[8,0,1],"start":-300.,"end":200.,"easingType":2}]);
    next["scaleXEvents"] =
        json!([{"startTime":[0,0,1],"endTime":[8,0,1],"start":0.5,"end":1.5,"easingType":5}]);
    next[noise::COLOR_EVENTS] = json!([{"startTime":[0,0,1],"endTime":[8,0,1],"start":[255,0,0],"end":[0,128,255],"easingType":1}]);
    next["rotateEvents"] = json!([{"startTime":[0,0,1],"endTime":[8,0,1],"start":0.,"end":90.,"easingType":29,"anchor":{"x":100.,"y":0.}}]);
    let mut distant = noise::create(
        Beat::new(322, 0, 1)?,
        Beat::new(334, 0, 1)?,
        Format::Next,
        false,
        app.editor.document().tempo(),
    );
    distant["Name"] = json!("Distant field");
    distant["editorPositionX"] = json!(20000.);
    let mut dense = noise::create(
        Beat::ZERO,
        Beat::new(8, 0, 1)?,
        Format::Native,
        false,
        app.editor.document().tempo(),
    );
    dense["phiStudioName"] = json!("Dense native scale");
    let times = std::iter::once(0.)
        .chain((0..=20).map(|n| 0.5 + n as f64 * 0.01))
        .chain([1.5, 2., 3.]);
    dense["scaleEvents"] = times
        .enumerate()
        .map(|(n, time)| {
            json!({"time":time,
        "scale":{"x":n,"y":n},"anchor":{"x":0.5,"y":0.5},"easeTypeX":1,"easeTypeY":2})
        })
        .collect();
    app.editor.execute_batch(
        "Noise gallery",
        vec![
            Command::EditNoise(Action::Insert {
                index: 0,
                value: native,
            }),
            Command::EditNoise(Action::Insert {
                index: 1,
                value: next,
            }),
            Command::EditNoise(Action::Insert {
                index: 2,
                value: distant,
            }),
            Command::EditNoise(Action::Insert {
                index: 3,
                value: dense,
            }),
            Command::EditNoise(Action::Insert {
                index: 4,
                value: noise::create(
                    Beat::ZERO,
                    Beat::ONE,
                    Format::Native,
                    false,
                    app.editor.document().tempo(),
                ),
            }),
        ],
    )?;
    app.advanced.right_mode = RightMode::NoiseFields;
    app.windows.sidebar = true;
    preview
        .replace_chart(app.editor.document(), None, &app.project_options)
        .await?;
    let mut report = vec![];
    for language in [
        Language::English,
        Language::Russian,
        Language::SimplifiedChinese,
    ] {
        app.options.language = language;
        for (scene, field, animation, track, pan) in [
            ("fields", 0, false, Track::Scale, false),
            ("native-scale", 0, true, Track::Scale, false),
            ("dense-native-scale", 3, true, Track::Scale, false),
            ("next-activation", 1, true, Track::Active, false),
            ("next-rotation", 1, true, Track::Rotate, true),
            ("next-color", 1, true, Track::Color, true),
        ] {
            app.noise.animation = false;
            app.select_noise(Some(field), false);
            app.noise.animation = animation;
            app.noise.track = track;
            if animation {
                app.select_noise(Some(0), false);
            }
            for width in [220., 330.] {
                app.noise.offset = if pan { 1. } else { 0. };
                for _ in 0..5 {
                    paint(gui, app, preview, width)?;
                    next_frame().await;
                }
                // next_frame clears the new framebuffer; capture a freshly painted frame.
                paint(gui, app, preview, width)?;
                let name = format!("{}-{scene}-{}", language.code(), width);
                get_screen_data().export_png(out.join(format!("{name}.png")).to_str().unwrap());
                report.push(name);
            }
        }
    }
    app.noise.animation = false;
    app.select_noise(Some(0), false);
    app.open_noise_color(None, [40, 180, 255]);
    for _ in 0..5 {
        paint(gui, app, preview, 330.)?;
        next_frame().await;
    }
    paint(gui, app, preview, 330.)?;
    get_screen_data().export_png(out.join("palette.png").to_str().unwrap());
    app.palette = None;
    app.options.language = Language::Russian;
    let grid = crate::grid_input::geometry(app.timeline.area, app);
    ensure!(
        app.jump_noise_field(2, grid),
        "Failed to reveal the selected Noise Field"
    );
    ensure!(
        (app.transport.beat - 321.).abs() < 0.001,
        "Field list did not seek to the selected lifetime"
    );
    for _ in 0..5 {
        paint(gui, app, preview, 330.)?;
        next_frame().await;
    }
    paint(gui, app, preview, 330.)?;
    get_screen_data().export_png(out.join("field-list-jump.png").to_str().unwrap());
    app.noise.animation = false;
    app.select_noise(Some(3), false);
    app.noise.animation = true;
    app.noise.track = Track::Scale;
    app.select_noise(Some(1), false);
    app.set_grid_beat(0.);
    app.timeline.lanes = 11;
    app.noise.offset = 0.;
    for _ in 0..5 {
        paint(gui, app, preview, 330.)?;
        next_frame().await;
    }
    paint(gui, app, preview, 330.)?;
    get_screen_data().export_png(out.join("vertical-grid-11.png").to_str().unwrap());
    app.timeline.lanes = 5;
    app.noise.offset = 0.;
    for _ in 0..5 {
        paint(gui, app, preview, 330.)?;
        next_frame().await;
    }
    paint(gui, app, preview, 330.)?;
    get_screen_data().export_png(out.join("vertical-grid-5.png").to_str().unwrap());
    let mut preferences = app.preferences();
    preferences.grid_window = true;
    app.apply_preferences(&preferences);
    for _ in 0..5 {
        paint(gui, app, preview, 330.)?;
        next_frame().await;
    }
    paint(gui, app, preview, 330.)?;
    get_screen_data().export_png(out.join("grid-settings.png").to_str().unwrap());
    preferences.grid_window = false;
    app.apply_preferences(&preferences);
    app.options.ui_scale = 1.5;
    app.noise.offset = 8.19;
    for _ in 0..5 {
        paint(gui, app, preview, 220.)?;
        next_frame().await;
    }
    paint(gui, app, preview, 220.)?;
    get_screen_data().export_png(out.join("navigation-150-percent.png").to_str().unwrap());

    app.options.ui_scale = 1.;
    app.noise.animation = false;
    app.select_noise(Some(4), false);
    app.noise.offset = 0.;
    app.set_grid_beat(0.);
    app.set_tool(crate::workbench::Tool::Select);
    for _ in 0..5 {
        paint(gui, app, preview, 330.)?;
        next_frame().await;
    }
    let grid = crate::grid_input::geometry(app.timeline.area, app);
    let x = grid.rect.left() + 4.5 * 52.;
    let press = |p, pressed| egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let a = pos2(x, grid.y(1.));
    let b = pos2(x, grid.y(2.));
    crate::grid_input::handle(
        app,
        &[press(a, true), egui::Event::PointerMoved(b)],
        grid,
        grid.full_rect(),
        |_| true,
        false,
    );
    paint(gui, app, preview, 330.)?;
    ensure!(
        app.timeline.drag.is_some(),
        "Resize ghost was lost before release"
    );
    get_screen_data().export_png(out.join("field-upper-resize-ghost.png").to_str().unwrap());
    crate::grid_input::handle(
        app,
        &[press(b, false)],
        grid,
        grid.full_rect(),
        |_| true,
        false,
    );
    app.noise.animation = true;
    app.noise.track = Track::Move;
    app.set_grid_beat(0.5);
    app.add_noise();
    app.set_grid_beat(0.);
    for _ in 0..5 {
        paint(gui, app, preview, 330.)?;
        next_frame().await;
    }
    paint(gui, app, preview, 330.)?;
    get_screen_data().export_png(out.join("new-native-keyframe.png").to_str().unwrap());
    let grid = crate::grid_input::geometry(app.timeline.area, app);
    let x = grid.event_rect.left() + grid.event_rect.width() / 8.;
    let a = pos2(x, grid.y(0.75));
    let b = pos2(x, grid.y(1.25));
    crate::grid_input::handle(
        app,
        &[press(a, true), egui::Event::PointerMoved(b)],
        grid,
        grid.full_rect(),
        |_| true,
        false,
    );
    paint(gui, app, preview, 330.)?;
    ensure!(
        app.timeline.drag.is_some(),
        "Native key endpoint resize was lost"
    );
    get_screen_data().export_png(out.join("native-upper-resize-ghost.png").to_str().unwrap());
    crate::grid_input::handle(
        app,
        &[press(b, false)],
        grid,
        grid.full_rect(),
        |_| true,
        false,
    );

    app.noise.animation = false;
    app.set_grid_beat(0.);
    while app.noise.selected.is_some()
        && app.editor.document().as_json()["blockAreaList"]
            .as_array()
            .map_or(0, Vec::len)
            < 8
    {
        app.noise.offset = 10000.;
        app.add_noise();
    }
    ensure!(
        app.noise.selected == Some(7),
        "Cursor field creation did not select field #8"
    );
    for _ in 0..5 {
        paint(gui, app, preview, 330.)?;
        next_frame().await;
    }
    paint(gui, app, preview, 330.)?;
    get_screen_data().export_png(out.join("cursor-field-8-reveal.png").to_str().unwrap());

    app.noise.animation = false;
    app.select_noise(Some(0), false);
    app.open_noise_preview_editor();
    for _ in 0..5 {
        paint(gui, app, preview, 330.)?;
        next_frame().await;
    }
    paint(gui, app, preview, 330.)?;
    get_screen_data().export_png(out.join("fullscreen-field-grid.png").to_str().unwrap());
    app.options.noise_preview_grid = false;
    paint(gui, app, preview, 330.)?;
    get_screen_data().export_png(out.join("fullscreen-field-no-grid.png").to_str().unwrap());
    app.close_fullscreen_preview();
    app.options.noise_preview_grid = true;

    for (field, track, target, name) in [
        (
            0,
            Track::Move,
            crate::noise_ui::SampleTarget::Position,
            "native-position-grid",
        ),
        (
            0,
            Track::Scale,
            crate::noise_ui::SampleTarget::Anchor,
            "native-anchor-grid",
        ),
        (
            1,
            Track::Rotate,
            crate::noise_ui::SampleTarget::Anchor,
            "next-anchor-grid",
        ),
    ] {
        app.noise.animation = false;
        app.select_noise(Some(field), false);
        app.noise.animation = true;
        app.noise.track = track;
        app.select_noise(Some(0), false);
        if track == Track::Move {
            app.add_noise();
        }
        app.open_noise_sample(target);
        for _ in 0..5 {
            paint(gui, app, preview, 330.)?;
            next_frame().await;
        }
        paint(gui, app, preview, 330.)?;
        get_screen_data().export_png(out.join(format!("{name}.png")).to_str().unwrap());
        app.close_fullscreen_preview();
    }

    ensure!(
        app.editor.document().as_json()["judgeLineList"] == source["judgeLineList"],
        "Noise UI changed judge lines"
    );
    std::fs::write(
        out.join("noise-ui.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "PASS: Noise Field workspace, native/Next animation, shared palette; {} frames",
        report.len()
    );
    Ok(())
}

/// Compare the draft's actual GPU texture with rendering the same saved data.
pub(super) async fn live(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &Path,
) -> Result<()> {
    let mut report = vec![];
    for format in [Format::Native, Format::Next] {
        *app = Workbench::new(ChartDocument::empty("Subtract preview", 120.)?, None);
        app.advanced.right_mode = RightMode::NoiseFields;
        app.options.preview_hud = false;
        app.options.preview_particles = false;
        app.options.snap_x = false;
        app.options.snap_time = false;
        let mut normal = noise::create(
            Beat::ZERO,
            Beat::new(8, 0, 1)?,
            format,
            false,
            app.editor.document().tempo(),
        );
        if format == Format::Next {
            normal["activeIntervals"] = json!([{"startTime":[0,0,1],"endTime":[8,0,1]}]);
        }
        let mut subtract = normal.clone();
        let (flag, names, x) = if format == Format::Native {
            (
                "isSubtract",
                ["bottomLeftPercentage", "topRightPercentage"],
                [0.4, 0.6],
            )
        } else {
            ("isInvert", ["bottomLeft", "topRight"], [-135., 135.])
        };
        subtract[flag] = json!(true);
        for (key, x) in names.into_iter().zip(x) {
            subtract[key]["x"] = json!(x);
        }
        ensure!(
            app.noise_actions(
                "Subtract fixture",
                vec![
                    Action::Insert {
                        index: 0,
                        value: normal
                    },
                    Action::Insert {
                        index: 1,
                        value: subtract
                    }
                ]
            ),
            "Could not create fields"
        );
        let source = app.editor.document().as_json().clone();
        preview
            .replace_chart(app.editor.document(), None, &app.project_options)
            .await?;
        for index in [0, 1] {
            app.select_noise(Some(index), false);
            app.open_noise_preview_editor();
            app.transport.beat = 2.;
            for _ in 0..4 {
                paint(gui, app, preview, 330.)?;
                next_frame().await;
            }
            paint(gui, app, preview, 330.)?;
            let initial = preview.snapshot();
            let corner = app.noise.preview_handles[0].center();
            ensure!(app.noise_preview_press(corner), "Missing geometry handle");
            app.noise_preview_move(corner + vec2(45., -20.));
            app.noise_preview_end_gesture();
            paint(gui, app, preview, 330.)?;
            let edited = preview.snapshot();
            ensure!(
                edited.bytes != initial.bytes,
                "Geometry did not update live"
            );
            let (_, raw) = app.noise_preview_override().unwrap();
            let mut expected = source.clone();
            expected[noise::LIST][index] = raw;
            let expected = ChartDocument::from_value(expected)?;
            preview
                .replace_chart(&expected, None, &app.project_options)
                .await?;
            preview.render(1., preview.target_viewport());
            ensure!(
                preview.snapshot().bytes == edited.bytes,
                "{format:?} field {index}: editing uses different subtraction/parity than playback"
            );
            preview
                .replace_chart(app.editor.document(), None, &app.project_options)
                .await?;
            paint(gui, app, preview, 330.)?;
            let label = if format == Format::Native {
                "native"
            } else {
                "next"
            };
            get_screen_data().export_png(
                out.join(format!("{label}-field-{index}-subtract.png"))
                    .to_str()
                    .unwrap(),
            );
            app.transport.beat = 20.;
            paint(gui, app, preview, 330.)?;
            let outside = preview.snapshot();
            preview.set_noise_field_override(None);
            preview.render(10., preview.target_viewport());
            ensure!(
                preview.snapshot().bytes == outside.bytes,
                "{format:?} field {index}: visible outside lifetime"
            );
            app.close_fullscreen_preview();
            app.sync_noise_preview(preview, &mut None, true)?;
            preview.render(1., preview.target_viewport());
            ensure!(
                preview.snapshot().bytes == initial.bytes,
                "Cancel did not restore subtraction"
            );
            ensure!(
                app.editor.document().as_json() == &source,
                "Live editing changed the document"
            );
            report.push(json!({"format":label,"edited_field":index,"live_geometry":true,"matches_playback_subtraction":true,"hidden_outside_lifetime":true,"cancel_restores":true}));
        }
    }
    std::fs::write(
        out.join("live-preview.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("PASS: native/Next live editing matches playback subtraction, lifetime and Cancel");
    Ok(())
}
