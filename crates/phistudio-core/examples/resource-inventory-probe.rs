//! Inventory a real package in an isolated private workspace; never writes its source.
use phistudio_core::{import_project, load_chart, load_project_options_with_assets, resources};
use std::{path::PathBuf, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = PathBuf::from(std::env::args().nth(1).ok_or("Pass a PEZ path")?);
    let root = tempfile::tempdir()?;
    let started = Instant::now();
    let imported = import_project(
        &source,
        root.path(),
        source.file_name().unwrap().to_str().unwrap(),
        true,
    )?;
    let chart = imported.charts[imported.suggested.unwrap_or(0)]
        .path
        .clone();
    let assets = imported.assets;
    let imported_ms = started.elapsed().as_millis();
    let clock = Instant::now();
    let doc = load_chart(&chart)?;
    let parsed_ms = clock.elapsed().as_millis();
    let options = load_project_options_with_assets(&chart, Some(&assets))?;
    let clock = Instant::now();
    let refs = resources::references(&doc, &options)?;
    let snapshot_ms = clock.elapsed().as_millis();
    let reference_keys = refs.len();
    let clock = Instant::now();
    let inventory = resources::list_with_references(&assets, refs, Some(&chart))?;
    let scan_ms = clock.elapsed().as_millis();
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"lines":doc.line_count(),"notes":doc.note_count(),"import_ms":imported_ms,"parse_ms":parsed_ms,"reference_snapshot_ms":snapshot_ms,"inventory_ms":scan_ms,"reference_keys":reference_keys,"resources":inventory.iter().map(|r|serde_json::json!({"name":r.name,"bytes":r.bytes,"references":r.references})).collect::<Vec<_>>()})
        )?
    );
    Ok(())
}
