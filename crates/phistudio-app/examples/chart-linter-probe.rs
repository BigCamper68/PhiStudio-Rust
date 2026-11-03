//! Exercise the external Wasm against the actual original prpr preview.
use anyhow::{Result, anyhow, ensure};
use macroquad::prelude::*;
use phistudio_core::{ChartDocument, EditorOptions, Note, NoteId, ProjectOptions};
use phistudio_plugins::{Action, Output, Runtime};
use phistudio_prpr::Preview;
use phistudio_sdk::*;
use serde_json::json;
use std::{
    collections::VecDeque,
    path::PathBuf,
    time::{Duration, Instant},
};
const ID: &str = "phistudio.chart-linter";
fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "Chart Linter native probe".into(),
            window_width: 640,
            window_height: 360,
            ..Default::default()
        },
        async {
            if let Err(error) = run().await {
                eprintln!("{error:#}");
                std::process::exit(1)
            }
            std::process::exit(0)
        },
    )
}
async fn wait(runtime: &Runtime, id: u64) -> Result<Output> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(done) = runtime.poll().map_err(|e| anyhow!(e))? {
            ensure!(done.id == id, "Unexpected completion");
            return done.result.map_err(|e| anyhow!(e));
        }
        ensure!(Instant::now() < deadline, "Plugin worker timed out");
        next_frame().await;
    }
}
fn has_text(widgets: &[Widget], needle: &str) -> bool {
    widgets.iter().any(|w| match w {
        Widget::Label { text } => text.contains(needle),
        Widget::Row { children } | Widget::Column { children } => has_text(children, needle),
        _ => false,
    })
}
async fn run() -> Result<()> {
    let tokio = tokio::runtime::Runtime::new()?;
    let _guard = tokio.enter();
    next_frame().await;
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "artifacts/checks/chart-linter".into()),
    );
    std::fs::create_dir_all(&output)?;
    let mut raw = ChartDocument::empty("Native linter fixture", 120.)?
        .as_json()
        .clone();
    let mut note = serde_json::to_value(Note::default())?;
    note["startTime"] = json!([2, 0, 1]);
    note["endTime"] = json!([2, 0, 1]);
    note["visibleTime"] = json!(0.1);
    let mut below = note.clone();
    below["above"] = json!(0);
    below["positionX"] = json!(-250);
    raw["judgeLineList"][0]["notes"] = json!([note, below]);
    raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0]["start"] = json!(135.);
    raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"][0]["end"] = json!(135.);
    let document = ChartDocument::from_value(raw)?;
    let original = document.as_json().clone();
    let mut preview = Preview::new(&document, None, &ProjectOptions::default()).await?;
    preview.set_editor_options(
        &EditorOptions {
            preview_hud: false,
            preview_particles: false,
            ..Default::default()
        },
        0,
    )?;
    preview.render(0.8, preview.target_viewport());
    let version = DocumentVersion {
        session: 1,
        revision: 0,
    };
    let before = preview.snapshot().bytes;
    for index in 0..2 {
        let result = preview.inspect_note(&PreviewInspection {
            version,
            line: 0,
            note: index,
            offsets: vec![0., -0.05, -0.2],
        })?;
        ensure!(
            result.points[0].inside && result.points[0].visible,
            "Judgment should be visible: {result:?}"
        );
        ensure!(
            !result.points[2].visible,
            "Original visibleTime must hide the early probe"
        );
        ensure!(
            preview.snapshot().bytes == before,
            "Read-only inspection changed native pixels"
        );
    }
    preview.render(1., preview.target_viewport());
    preview
        .snapshot()
        .export_png(output.join("native-notes.png").to_string_lossy().as_ref());
    for index in 0..2 {
        let result = preview.inspect_note(&PreviewInspection {
            version,
            line: 0,
            note: index,
            offsets: vec![0.],
        })?;
        let position = result.points[0]
            .position
            .ok_or_else(|| anyhow!("No native position"))?;
        let picked = preview.pick_note(position.map(|v| v as f32), [0.003; 2])?;
        ensure!(
            picked == Some(NoteId { line: 0, index }),
            "Inspection disagrees with original GPU picking: {position:?}, {picked:?}"
        );
    }
    let mut parent_source = original.clone();
    let mut parent = parent_source["judgeLineList"][0].clone();
    parent["notes"] = json!([]);
    for endpoint in ["start", "end"] {
        parent["eventLayers"][0]["rotateEvents"][0][endpoint] = json!(30.);
        parent["eventLayers"][0]["moveXEvents"][0][endpoint] = json!(-100.);
    }
    parent_source["judgeLineList"]
        .as_array_mut()
        .unwrap()
        .push(parent);
    parent_source["judgeLineList"][0]["father"] = json!(1);
    parent_source["judgeLineList"][0]["rotateWithFather"] = json!(true);
    let parent_document = ChartDocument::from_value(parent_source.clone())?;
    preview
        .replace_chart(&parent_document, None, &ProjectOptions::default())
        .await?;
    preview.render(1., preview.target_viewport());
    let parent_point = preview
        .inspect_note(&PreviewInspection {
            version,
            line: 0,
            note: 0,
            offsets: vec![0.],
        })?
        .points[0]
        .position
        .ok_or_else(|| anyhow!("Parent geometry is not finite"))?;
    ensure!(
        preview.pick_note(parent_point.map(|v| v as f32), [0.003; 2])?
            == Some(NoteId { line: 0, index: 0 }),
        "Native parent transform disagrees with GPU picking"
    );
    parent_source["judgeLineList"][0]["sizeControl"] =
        json!([{"x":0,"size":0,"easing":1},{"x":100000,"size":0,"easing":1}]);
    preview
        .replace_chart(
            &ChartDocument::from_value(parent_source)?,
            None,
            &ProjectOptions::default(),
        )
        .await?;
    preview.render(1., preview.target_viewport());
    let zero = preview.inspect_note(&PreviewInspection {
        version,
        line: 0,
        note: 0,
        offsets: vec![0.],
    })?;
    ensure!(
        zero.points[0].width == Some(0.) && !zero.points[0].visible,
        "Native zero-width control was not respected"
    );
    preview
        .replace_chart(&document, None, &ProjectOptions::default())
        .await?;
    preview.render(0.8, preview.target_viewport());
    let root = tempfile::tempdir()?;
    let mut runtime = Runtime::spawn(Some(root.path().into()), vec![]).map_err(|e| anyhow!(e))?;
    wait(&runtime, 0).await?;
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(
        repository.join("plugins/chart-linter/manifest.json"),
    )?)?;
    let package = repository.join(format!(
        "artifacts/plugins/chart-linter-{}.zip",
        manifest.version
    ));
    let id = runtime
        .submit(Action::Install(Box::new(package)))
        .map_err(|e| anyhow!(e))?;
    wait(&runtime, id).await?;
    let id = runtime
        .submit(Action::Enable(ID.into()))
        .map_err(|e| anyhow!(e))?;
    wait(&runtime, id).await?;
    let id = runtime
        .submit(Action::Ui {
            plugin: ID.into(),
            event: UiEvent {
                panel: "report".into(),
                widget: "deep".into(),
                value: serde_json::Value::Null,
            },
            context: EditorContext {
                version,
                ..Default::default()
            },
        })
        .map_err(|e| anyhow!(e))?;
    let mut jobs = VecDeque::from([id]);
    let mut final_widgets = vec![];
    let mut inspections = 0;
    let started = Instant::now();
    while let Some(id) = jobs.pop_front() {
        let output = wait(&runtime, id).await?;
        for effect in output.effects {
            ensure!(
                !matches!(effect.effect, Effect::Edit { .. }),
                "Scan attempted an edit"
            );
            if let Effect::Panel { widgets, .. } = effect.effect {
                final_widgets = widgets
            }
        }
        for call in output.calls {
            let outcome = match call.request.service.id.as_str() {
                "project" => {
                    runtime
                        .authorize_inspection(&call, version, "project", Capability::ProjectRead)
                        .map_err(|e| anyhow!(e))?;
                    ServiceOutcome::Success {
                        value: json!(ProjectInfo {
                            version,
                            assets: ProjectAssetsVersion {
                                session: 1,
                                revision: 0
                            },
                            has_assets: false,
                            chart_name: Some("chart.json".into()),
                            music: None,
                            illustration: None,
                            music_duration: None,
                            music_offset: 0.,
                            locale: "en".into(),
                            plugins: vec![]
                        }),
                    }
                }
                "preview" => {
                    runtime
                        .authorize_inspection(&call, version, "preview", Capability::Preview)
                        .map_err(|e| anyhow!(e))?;
                    let query = serde_json::from_value(call.request.arguments.clone())?;
                    inspections += 1;
                    ServiceOutcome::Success {
                        value: json!(preview.inspect_note(&query)?),
                    }
                }
                _ => runtime.document_service(&call, version, &document),
            };
            jobs.push_back(
                runtime
                    .submit(Action::ServiceReply { call, outcome })
                    .map_err(|e| anyhow!(e))?,
            );
        }
    }
    ensure!(
        has_text(&final_widgets, "Scan complete"),
        "Wasm scan did not finish: {final_widgets:?}"
    );
    ensure!(
        has_text(&final_widgets, "short_read"),
        "Wasm did not detect native visibility limits"
    );
    ensure!(inspections > 2, "Native inspection was not exercised");
    ensure!(document.as_json() == &original, "Source chart changed");
    std::fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"native_queries":inspections,"elapsed_ms":started.elapsed().as_millis(),"widgets":final_widgets,"source_preserved":true,"gpu_picking_agrees":true}),
        )?,
    )?;
    runtime.shutdown().map_err(|e| anyhow!(e))?;
    println!("Chart Linter Wasm + native preview passed ({inspections} native queries)");
    Ok(())
}
