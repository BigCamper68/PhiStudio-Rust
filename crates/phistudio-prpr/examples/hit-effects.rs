//! GPU regression: editor scrubbing has no hit effects; native playback keeps them.
use anyhow::{Result, ensure};
use macroquad::prelude::*;
use phistudio_core::{
    Beat, ChartDocument, Command, Editor, EditorOptions, Note, NoteKind, ProjectOptions,
};
use phistudio_prpr::Preview;

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "Hit effect regression".into(),
            window_width: 1280,
            window_height: 720,
            ..Default::default()
        },
        async {
            if let Err(error) = run().await {
                eprintln!("{error:#}");
                std::process::exit(1);
            }
            std::process::exit(0);
        },
    );
}
async fn run() -> Result<()> {
    let output = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "artifacts/checks/hit-effects".into()),
    );
    std::fs::create_dir_all(&output)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let _runtime = runtime.enter();
    let mut editor = Editor::new(ChartDocument::empty("FX regression", 120.)?);
    for (kind, start, end) in [
        (NoteKind::Tap, 2, 2),
        (NoteKind::Flick, 4, 4),
        (NoteKind::Drag, 6, 6),
        (NoteKind::Hold, 8, 10),
    ] {
        editor.execute(Command::AddNote {
            line: 0,
            note: Note {
                kind,
                start_time: Beat::new(start, 0, 1)?,
                end_time: Beat::new(end, 0, 1)?,
                ..Default::default()
            },
        })?;
    }
    let original = editor.document().as_json().clone();
    let mut preview = Preview::new(editor.document(), None, &ProjectOptions::default()).await?;
    preview.set_audio_levels(0., 0.)?;
    let mut options = EditorOptions {
        preview_hud: false,
        mark_active_line_id: false,
        highlight_current_line: false,
        ..Default::default()
    };
    for (name, time, playing) in [
        ("paused-forward", 1.08, false),
        ("paused-backward", 1.04, false),
        ("paused-flick", 2.08, false),
        ("paused-drag", 3.08, false),
        ("paused-hold", 4.45, false),
        ("paused-hold-end", 5.08, false),
        ("expired", 8., false),
        ("before-hit", 0.7, false),
        ("live-seek", 1.08, true),
    ] {
        options.preview_particles = true;
        preview.set_editor_options(&options, 0)?;
        preview.set_playback(time, playing, true)?;
        preview.render(time, (0, 0, 1280, 720));
        let particles = preview.snapshot();
        ensure!(
            preview.playback_report()["playing"] == playing,
            "Seek changed playback intent"
        );
        if !playing {
            next_frame().await;
            preview.render(time, (0, 0, 1280, 720));
            ensure!(
                particles.bytes == preview.snapshot().bytes,
                "Paused particles changed without a cursor movement"
            );
        }
        particles.export_png(output.join(format!("{name}-on.png")).to_str().unwrap());
        options.preview_particles = false;
        preview.set_editor_options(&options, 0)?;
        preview.render(time, (0, 0, 1280, 720));
        let plain = preview.snapshot();
        plain.export_png(output.join(format!("{name}-off.png")).to_str().unwrap());
        let different = particles
            .bytes
            .as_chunks::<4>()
            .0
            .iter()
            .zip(plain.bytes.as_chunks::<4>().0.iter())
            .filter(|(a, b)| a[..3] != b[..3])
            .count();
        ensure!(
            different == 0,
            "Historical particles appeared after {name}: {different}"
        );
        println!("{name}: {different} particle pixels");
        next_frame().await;
    }
    // Toggling the setting on a paused cursor must not reconstruct hit effects.
    preview.set_playback(1.08, false, true)?;
    options.preview_particles = false;
    preview.set_editor_options(&options, 0)?;
    preview.render(1.08, (0, 0, 1280, 720));
    let plain = preview.snapshot();
    options.preview_particles = true;
    preview.set_editor_options(&options, 0)?;
    preview.render(1.08, (0, 0, 1280, 720));
    ensure!(
        plain.bytes == preview.snapshot().bytes,
        "Enabling effects at a paused cursor reconstructed historical particles"
    );
    ensure!(
        preview.playback_report()["playing"] == false,
        "Enabling effects started playback"
    );
    for (name, hit) in [
        ("tap", 1.),
        ("flick", 2.),
        ("drag", 3.),
        ("hold", 4.),
        ("hold-end", 5.),
    ] {
        options.preview_particles = true;
        preview.set_editor_options(&options, 0)?;
        preview.set_playback(hit - 0.03, true, true)?;
        preview.render(hit - 0.03, preview.target_viewport());
        next_frame().await;
        preview.render(hit + 0.02, preview.target_viewport());
        if name == "hold" {
            next_frame().await;
            preview.render(hit + 0.04, preview.target_viewport());
        }
        let time = if name == "hold" {
            hit + 0.04
        } else {
            hit + 0.02
        };
        let live = preview.snapshot();
        live.export_png(output.join(format!("live-{name}.png")).to_str().unwrap());
        // Compare the same judged chart state with particles hidden, then pause
        // and resume without moving: neither action may replay the old burst.
        options.preview_particles = false;
        preview.set_editor_options(&options, 0)?;
        preview.render(time, preview.target_viewport());
        let plain = preview.snapshot();
        let different = live
            .bytes
            .as_chunks::<4>()
            .0
            .iter()
            .zip(plain.bytes.as_chunks::<4>().0.iter())
            .filter(|(a, b)| a[..3] != b[..3])
            .count();
        ensure!(
            different > 40,
            "Native playback lost {name} effects: {different}"
        );
        println!("live-{name}: {different} particle pixels");
        options.preview_particles = true;
        preview.set_editor_options(&options, 0)?;
        preview.set_playback(time, false, false)?;
        preview.render(time, preview.target_viewport());
        ensure!(
            preview.snapshot().bytes == plain.bytes,
            "Pause retained {name} particles"
        );
        preview.set_playback(time, true, false)?;
        preview.render(time, preview.target_viewport());
        ensure!(
            preview.snapshot().bytes == plain.bytes,
            "Resume replayed {name} particles"
        );
    }
    // Pausing with an actual live burst must clear it even without toggling
    // the effects setting or moving the chart cursor.
    preview.set_playback(0.97, true, true)?;
    preview.render(0.97, preview.target_viewport());
    next_frame().await;
    preview.render(1.02, preview.target_viewport());
    let live = preview.snapshot();
    preview.set_playback(1.02, false, false)?;
    preview.render(1.02, preview.target_viewport());
    let paused = preview.snapshot();
    ensure!(
        live.bytes != paused.bytes,
        "Pause did not hide the live burst"
    );
    options.preview_particles = false;
    preview.set_editor_options(&options, 0)?;
    preview.render(1.02, preview.target_viewport());
    ensure!(
        paused.bytes == preview.snapshot().bytes,
        "Paused frame still contains effects"
    );
    ensure!(
        editor.document().as_json() == &original,
        "Preview changed the authored document"
    );
    std::fs::write(
        output.join("result.txt"),
        "PASS: paused forward/backward and live seeks have no historical hit effects; paused setting toggles do not reconstruct particles; native playback shows Tap/Flick/Drag/Hold/end effects; pause clears live particles; resume does not replay old bursts; chart data is unchanged.\n",
    )?;
    Ok(())
}
