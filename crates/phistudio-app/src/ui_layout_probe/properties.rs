//! Render actual property forms and restore a text-focused native window.
use super::*;
use egui::{Event, PointerButton, Pos2};
use phistudio_core::{EventId, EventType};
use std::path::Path;

fn button(point: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: point,
        button: PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}
#[cfg(target_os = "linux")]
fn native_window() -> Result<String> {
    let result = std::process::Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^UI layout regression probe$",
        ])
        .output()?;
    ensure!(
        result.status.success(),
        "Could not find native probe window"
    );
    Ok(String::from_utf8(result.stdout)?
        .lines()
        .last()
        .unwrap()
        .to_owned())
}
fn resize_native(width: f32, height: f32) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let status = std::process::Command::new("xdotool")
            .args([
                "windowsize",
                &native_window()?,
                &(width as u32).to_string(),
                &(height as u32).to_string(),
            ])
            .status()?;
        ensure!(status.success(), "Native resize failed");
    }
    #[cfg(not(target_os = "linux"))]
    request_new_screen_size(width, height);
    Ok(())
}
fn capture(out: &Path, name: &str) {
    get_screen_data().export_png(out.join(format!("{name}.png")).to_str().unwrap());
}
fn floating(gui: &mut Gui, app: &mut Workbench, preview: &mut Preview) -> Result<(Rect, bool)> {
    let mut window = Rect::NOTHING;
    let mut text = false;
    paint_observed(gui, app, preview, 330., |ctx| {
        window = crate::window::visible_rect(ctx, Id::new("floating-properties"))
            .unwrap_or(Rect::NOTHING);
        text = ctx.wants_keyboard_input();
    })?;
    Ok((window, text))
}

pub(super) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &Path,
) -> Result<()> {
    let source = app.editor.document().as_json().clone();
    let framebuffer = [screen_width(), screen_height()];
    let mut report = vec![];
    for language in [
        Language::English,
        Language::Russian,
        Language::SimplifiedChinese,
    ] {
        app.options.language = language;
        app.options.ui_scale = 1.;
        for event in [false, true] {
            if event {
                app.select_event(Some(EventId {
                    line: 0,
                    layer: 0,
                    kind: EventType::MoveX,
                    index: 0,
                }));
            } else {
                app.select(Some(NoteId { line: 0, index: 0 }));
            }
            let label = format!(
                "{}-{}",
                language.code(),
                if event { "event" } else { "note" }
            );
            app.windows.floating_properties = false;
            for width in [190., 220., 400.] {
                for _ in 0..5 {
                    paint(gui, app, preview, width)?;
                    next_frame().await;
                }
                capture(out, &format!("{label}-docked-{width}"));
            }
            app.windows.floating_properties = true;
            for _ in 0..5 {
                floating(gui, app, preview)?;
                next_frame().await;
            }
            let (initial, _) = floating(gui, app, preview)?;
            // Resize using egui's real window corner, then retain that user size.
            let from = initial.max - egui::vec2(2., 2.);
            let to = from
                + egui::vec2(
                    (430. - initial.width()).max(0.),
                    (440. - initial.height()).max(0.),
                );
            gui.probe_input(vec![Event::PointerMoved(from), button(from, true)], false);
            floating(gui, app, preview)?;
            next_frame().await;
            gui.probe_input(vec![Event::PointerMoved(to)], false);
            floating(gui, app, preview)?;
            next_frame().await;
            gui.probe_input(vec![button(to, false)], false);
            for _ in 0..5 {
                floating(gui, app, preview)?;
                next_frame().await;
            }
            let (original, _) = floating(gui, app, preview)?;
            capture(out, &format!("{label}-floating"));
            let mut focused = false;
            // At the right edge only beat text fields occupy these rows. Note
            // type buttons and numeric sliders are safely to the left.
            for y in [72., 108., 135., 160., 185.] {
                let p = egui::pos2(original.right() - 60., original.top() + y);
                #[cfg(target_os = "linux")]
                {
                    let window = native_window()?;
                    ensure!(
                        std::process::Command::new("xdotool")
                            .args(["windowfocus", "--sync", &window])
                            .status()?
                            .success()
                    );
                    ensure!(
                        std::process::Command::new("xdotool")
                            .args([
                                "mousemove",
                                "--sync",
                                "--window",
                                &window,
                                &(p.x * app.pixels_per_point).round().to_string(),
                                &(p.y * app.pixels_per_point).round().to_string()
                            ])
                            .status()?
                            .success()
                    );
                    ensure!(
                        std::process::Command::new("xdotool")
                            .args(["click", "1"])
                            .status()?
                            .success()
                    );
                    next_frame().await;
                }
                #[cfg(not(target_os = "linux"))]
                {
                    gui.probe_input(vec![Event::PointerMoved(p), button(p, true)], false);
                    floating(gui, app, preview)?;
                    next_frame().await;
                    gui.probe_input(vec![button(p, false)], false);
                }
                for _ in 0..3 {
                    focused = floating(gui, app, preview)?.1;
                    next_frame().await;
                }
                println!("{label}: beat-field click {p:?}; focused={focused}; window={original:?}");
                if focused {
                    break;
                }
            }
            ensure!(focused, "Could not focus an actual beat field: {label}");
            let (before, _) = floating(gui, app, preview)?;
            resize_native(framebuffer[0], framebuffer[1] * 0.55)?;
            for _ in 0..12 {
                floating(gui, app, preview)?;
                next_frame().await;
            }
            ensure!(
                (screen_height() - (framebuffer[1] * 0.55).floor()).abs() < 2.,
                "Framebuffer did not actually shrink"
            );
            let (covered, _) = floating(gui, app, preview)?;
            ensure!(
                covered.height() < before.height() - 20.,
                "Window did not encounter a constrained viewport"
            );
            capture(out, &format!("{label}-keyboard"));
            resize_native(framebuffer[0], framebuffer[1])?;
            for _ in 0..12 {
                floating(gui, app, preview)?;
                next_frame().await;
            }
            let (after, _) = floating(gui, app, preview)?;
            ensure!(
                (after.size() - before.size()).length() < 2.,
                "Native window size did not recover: {label}: {before:?} -> {after:?}"
            );
            ensure!(
                (after.min - before.min).length() < 2.,
                "Native window position did not recover: {label}"
            );
            capture(out, &format!("{label}-restored"));
            report.push(serde_json::json!({"case":label,"before":[before.min.x,before.min.y,before.width(),before.height()],"covered":[covered.width(),covered.height()],"after":[after.min.x,after.min.y,after.width(),after.height()]}));
            gui.probe_input(
                vec![Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
                false,
            );
            floating(gui, app, preview)?;
            next_frame().await;
            gui.probe_input(
                vec![Event::Key {
                    key: egui::Key::Escape,
                    pressed: false,
                    repeat: false,
                    modifiers: Default::default(),
                }],
                false,
            );
            floating(gui, app, preview)?;
            next_frame().await;
        }
    }
    ensure!(
        app.editor.document().as_json() == &source,
        "Property/window probe edited the chart"
    );
    std::fs::write(
        out.join("keyboard-geometry.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    std::fs::write(
        out.join("result.txt"),
        "PASS: native property forms at three inspector widths; real beat-field focus, native viewport shrink, restored user window geometry; chart unchanged\n",
    )?;
    println!(
        "PASS: {} native keyboard/window cycles; chart unchanged",
        report.len()
    );
    Ok(())
}
