use phistudio_plugins::{create_plugin_template, inspect_external_package, pack_external_package};
use std::path::Path;

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["new", destination, id] => {
            let sdk = Path::new(env!("CARGO_MANIFEST_DIR")).join("../phistudio-sdk");
            create_plugin_template(Path::new(destination), id, &sdk)?;
            println!("Created {destination}");
        }
        [command, source] if matches!(*command, "validate" | "inspect") => {
            let info = inspect_external_package(Path::new(source))?;
            if *command == "inspect" {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&info).map_err(|e| e.to_string())?
                );
            } else {
                println!(
                    "Valid package: {} {}",
                    info["manifest"]["id"].as_str().unwrap_or(""),
                    info["manifest"]["version"].as_str().unwrap_or("")
                );
            }
        }
        ["pack", source, destination] => {
            pack_external_package(Path::new(source), Path::new(destination))?;
            println!("Created {destination}");
        }
        ["--help"] | ["-h"] | [] => println!(
            "phistudio-plugin new <directory> <id>\nphistudio-plugin validate <directory-or-zip>\nphistudio-plugin inspect <directory-or-zip>\nphistudio-plugin pack <directory-or-zip> <output.zip>"
        ),
        _ => return Err("Unknown arguments; use --help".into()),
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
