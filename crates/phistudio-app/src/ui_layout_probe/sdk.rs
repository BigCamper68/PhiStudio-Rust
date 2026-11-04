//! A separately compiled public-SDK Wasm guest in actual egui/prpr GPU frames.
use super::*;
use phistudio_plugins::Action;
use phistudio_sdk::{PermissionDecision, PermissionRequest};
use std::path::Path;

async fn settle(gui: &mut Gui, app: &mut Workbench, preview: &mut Preview) -> Result<()> {
    let deadline = get_time() + 20.;
    loop {
        app.poll_plugins();
        ensure!(app.error.is_none(), "{:?}", app.error);
        paint(gui, app, preview, 210.)?;
        if !app.plugins.working() {
            return Ok(());
        }
        ensure!(get_time() < deadline, "SDK probe worker timed out");
        next_frame().await;
    }
}
pub(super) async fn run(
    gui: &mut Gui,
    app: &mut Workbench,
    preview: &mut Preview,
    out: &Path,
) -> Result<()> {
    let original = app.editor.document().as_json().clone();
    let root = tempfile::tempdir()?;
    app.start_plugins(Some(root.path().into()));
    settle(gui, app, preview).await?;
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/sdk-lab");
    app.plugin_action(Action::Install(Box::new(source)));
    settle(gui, app, preview).await?;
    let manifest = app
        .plugins
        .registry
        .plugins
        .iter()
        .find(|p| p.manifest.id == "example.sdk-lab")
        .ok_or_else(|| anyhow::anyhow!("SDK Lab was not installed"))?
        .manifest
        .clone();
    let requested = PermissionRequest::from_manifest(&manifest);
    app.plugin_action(Action::Permissions {
        plugin: manifest.id.clone(),
        decision: Some(PermissionDecision {
            requested: requested.clone(),
            granted: requested,
        }),
    });
    app.plugin_action(Action::Enable(manifest.id.clone()));
    settle(gui, app, preview).await?;
    let origin = app
        .plugins
        .registry
        .plugins
        .iter()
        .find_map(|p| p.origin.clone())
        .ok_or_else(|| anyhow::anyhow!("SDK Lab did not activate"))?;
    ensure!(
        app.plugins.panels.values().all(|p| !p.open),
        "Activation opened a plugin window"
    );
    ensure!(
        app.plugins
            .panels
            .get(&(origin.plugin.clone(), "lab".into()))
            .is_some_and(|p| p
                .widgets
                .iter()
                .any(|w| matches!(w, phistudio_sdk::Widget::Tabs { .. }))),
        "SDK Lab did not receive feature discovery or populate its rich UI"
    );
    app.open_plugin_panel(&origin, "lab");
    app.open_plugin_track(&origin, "clock");
    for _ in 0..8 {
        paint(gui, app, preview, 210.)?;
        app.poll_plugins();
        next_frame().await;
    }
    paint(gui, app, preview, 210.)?;
    get_screen_data().export_png(
        out.join("sdk-lab.png")
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Output path is not UTF-8"))?,
    );
    ensure!(
        app.editor.document().as_json() == &original && !app.editor.is_dirty(),
        "SDK Lab changed the chart"
    );
    app.plugin_action(Action::Disable(manifest.id));
    settle(gui, app, preview).await?;
    ensure!(
        app.plugins.panels.is_empty(),
        "Disabled plugin retained a UI panel"
    );
    app.plugins.shutdown().map_err(anyhow::Error::msg)?;
    std::fs::write(
        out.join("result.txt"),
        "PASS: external SDK Lab Wasm, explicit permissions, rich panel and embedded track in real egui/prpr GPU frames; chart unchanged; disabled UI cleaned up. No network request was started.\n",
    )?;
    println!("PASS: external SDK expansion GPU probe");
    Ok(())
}
