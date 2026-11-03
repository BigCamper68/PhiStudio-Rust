//! Run an external action through the real host and core without a GL window.
use phistudio_core::{Editor, load_chart, save_chart};
use phistudio_plugins::{Host, Package, WasmLimits};
use phistudio_sdk::{DocumentVersion, EditorContext, Effect};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.len() != 5 {
        return Err(
            "Usage: plugin-probe PACKAGE_DIR CHART COMMAND ARGUMENTS_JSON OUTPUT_CHART".into(),
        );
    }
    let package = Package::open(Path::new(&arguments[0]))?;
    let mut limits = WasmLimits::default();
    if let Ok(fuel) = std::env::var("PHISTUDIO_PLUGIN_FUEL") {
        limits.fuel_per_action = fuel.parse()?;
    }
    let instance = package.load(limits)?;
    let manifest = package.manifest;
    let id = manifest.id.clone();
    let mut host = Host::default();
    // This developer probe explicitly grants the manifest's requested access.
    host.install(manifest.clone(), instance, manifest.capabilities)?;
    host.enable(&id)?;
    let mut editor = Editor::new(load_chart(Path::new(&arguments[1]))?);
    let context = EditorContext {
        version: DocumentVersion {
            session: 1,
            revision: editor.revision(),
        },
        chart: Some(editor.document().as_json().clone()),
        ..Default::default()
    };
    let started = std::time::Instant::now();
    let result = host.command(
        &id,
        &arguments[2],
        serde_json::from_str(&arguments[3])?,
        context,
    )?;
    println!("Plugin action completed in {:?}", started.elapsed());
    for owned in result.effects {
        match owned.effect {
            Effect::Edit { transaction } => {
                host.apply_transaction(&owned.origin, 1, &mut editor, transaction)?;
            }
            Effect::Notify { message } => println!("{message}"),
            _ => {}
        }
    }
    host.uninstall(&id);
    let after = editor.document().to_json()?;
    if editor.can_undo() {
        assert!(editor.undo());
        assert!(editor.redo());
        assert_eq!(editor.document().to_json()?, after);
    }
    save_chart(editor.document(), Path::new(&arguments[4]))?;
    println!("Plugin action and history verified; saved {}", arguments[4]);
    Ok(())
}
