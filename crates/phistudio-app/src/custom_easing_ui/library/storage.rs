use super::*;
use std::{
    fs,
    io::{Read, Write},
    sync::mpsc::Sender,
};

struct Loaded {
    prepared: Arc<Prepared>,
    bytes: Option<Vec<u8>>,
}
fn read(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_LIBRARY_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_LIBRARY_BYTES {
        return Err("The easing library exceeds 8 MiB".into());
    }
    Ok(Some(bytes))
}
fn load(root: &Path) -> Result<Loaded, String> {
    let bytes = read(&root.join("templates.json"))?;
    let library = bytes.as_ref().map_or_else(
        || Ok(CurveLibrary::default()),
        |bytes| CurveLibrary::parse(bytes).map_err(|e| e.to_string()),
    )?;
    Ok(Loaded {
        prepared: Arc::new(Prepared::new(library)?),
        bytes,
    })
}
fn publish(root: &Path, current: &Loaded, library: CurveLibrary) -> Result<Loaded, String> {
    let bytes = library.export().map_err(|e| e.to_string())?;
    let prepared = Arc::new(Prepared::new(library)?);
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let path = root.join("templates.json");
    if read(&path)?.as_deref() != current.bytes.as_deref() {
        return Err("Curve templates changed on disk. Reload the library before saving".into());
    }
    let mut staged = tempfile::NamedTempFile::new_in(root).map_err(|e| e.to_string())?;
    staged.write_all(&bytes).map_err(|e| e.to_string())?;
    staged.as_file().sync_all().map_err(|e| e.to_string())?;
    staged.persist(path).map_err(|e| e.to_string())?;
    Ok(Loaded {
        prepared,
        bytes: Some(bytes),
    })
}
pub(super) fn worker(root: PathBuf, input: Receiver<Job>, output: Sender<Reply>) {
    let mut loaded = None;
    while let Ok(job) = input.recv() {
        let Job::Action { serial, action } = job else {
            if let Job::Flush(sender) = job {
                let _ = sender.send(());
            }
            continue;
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(&root, &mut loaded, action)
        }))
        .unwrap_or_else(|_| Err("Curve template operation failed".into()));
        if output
            .send(Reply {
                serial,
                ready: loaded.is_some(),
                result,
            })
            .is_err()
        {
            break;
        }
    }
}
fn run(root: &Path, loaded: &mut Option<Loaded>, action: Action) -> Result<Update, String> {
    if matches!(action, Action::Reload) {
        *loaded = None;
        *loaded = Some(load(root)?);
        return Ok(Update {
            prepared: loaded
                .as_ref()
                .ok_or_else(|| "Curve template load failed".to_owned())?
                .prepared
                .clone(),
            selected: Selection::Keep,
            export: None,
            message: "Curve templates loaded",
        });
    }
    let current = loaded
        .as_ref()
        .ok_or_else(|| "Load the curve template library first".to_owned())?;
    if let Action::Export(selected) = action {
        let library = match selected {
            Some(id) => CurveLibrary {
                templates: vec![
                    current
                        .prepared
                        .library
                        .templates
                        .iter()
                        .find(|t| t.id == id)
                        .ok_or_else(|| "The easing template no longer exists".to_owned())?
                        .clone(),
                ],
            },
            None => current.prepared.library.clone(),
        };
        let bytes = library.export_rpen().map_err(|e| e.to_string())?;
        let export = ProjectExport::from_bytes("animation-curves.rpencurve".into(), &bytes)
            .map_err(|e| e.to_string())?;
        return Ok(Update {
            prepared: current.prepared.clone(),
            selected: Selection::Keep,
            export: Some(export),
            message: "Choose where to export curve templates",
        });
    }
    let message = match &action {
        Action::Save { .. } => "Curve template saved",
        Action::Rename { .. } => "Curve template renamed",
        Action::Remove(_) => "Curve template deleted",
        Action::Import(_) => "Curve templates imported",
        _ => "Curve templates loaded",
    };
    let mut next = current.prepared.library.clone();
    let selected = match action {
        Action::Save {
            name,
            curve,
            replace,
        } => Selection::Select(
            next.save(&name, curve, replace.as_deref())
                .map_err(|e| e.to_string())?,
        ),
        Action::Rename { id, name } => {
            let curve = next
                .templates
                .iter()
                .find(|t| t.id == id)
                .ok_or_else(|| "The easing template no longer exists".to_owned())?
                .curve
                .clone();
            Selection::Select(
                next.save(&name, curve, Some(&id))
                    .map_err(|e| e.to_string())?,
            )
        }
        Action::Remove(id) => {
            next.remove(&id).map_err(|e| e.to_string())?;
            Selection::Clear
        }
        Action::Import(file) => {
            let bytes = read(file.path())?
                .ok_or_else(|| "Curve template file no longer exists".to_owned())?;
            let imported = CurveLibrary::parse(&bytes).map_err(|e| e.to_string())?;
            let first = next.templates.len();
            next.merge(&imported).map_err(|e| e.to_string())?;
            next.templates
                .get(first)
                .map_or(Selection::Keep, |t| Selection::Select(t.id.clone()))
        }
        Action::Reload | Action::Export(_) => {
            return Err("Unexpected curve template operation".into());
        }
    };
    let next = publish(root, current, next)?;
    let prepared = next.prepared.clone();
    *loaded = Some(next);
    Ok(Update {
        prepared,
        selected,
        export: None,
        message,
    })
}
