use phistudio_sdk::*;
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

fn confined(root: &Path, name: &str) -> Result<PathBuf, String> {
    if !name.is_empty() {
        validate_asset_path(name)?;
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let mut path = root;
    for part in name.split('/').filter(|p| !p.is_empty()) {
        path.push(part);
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("Asset links are not supported".into());
        }
    }
    Ok(path)
}

pub(super) fn snapshot(
    root: &Path,
    name: &str,
    allowance: u64,
    storage: Option<&Path>,
    current: impl Fn() -> Result<(), String>,
) -> Result<(tempfile::TempPath, u64), String> {
    current()?;
    let path = confined(root, name)?;
    let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > allowance {
        return Err("Asset must be a regular file within the 256 MiB snapshot budget".into());
    }
    let mut input = File::open(&path).map_err(|e| e.to_string())?;
    let metadata = input.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > allowance {
        return Err("Asset changed or exceeds the snapshot budget".into());
    }
    let mut output = if let Some(storage) = storage {
        let directory = storage.join("plugins-work");
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        tempfile::Builder::new()
            .prefix("asset-")
            .tempfile_in(directory)
    } else {
        tempfile::Builder::new()
            .prefix("phistudio-asset-")
            .tempfile()
    }
    .map_err(|e| e.to_string())?;
    let mut size = 0u64;
    let mut bytes = vec![0; MAX_FILE_CHUNK_BYTES];
    loop {
        current()?;
        let n = input.read(&mut bytes).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        size += n as u64;
        if size > allowance {
            return Err("Assets exceed the 256 MiB snapshot budget".into());
        }
        output.write_all(&bytes[..n]).map_err(|e| e.to_string())?;
    }
    if size != metadata.len() || input.metadata().map_err(|e| e.to_string())?.len() != size {
        return Err("Asset changed while preparing its snapshot".into());
    }
    output.flush().map_err(|e| e.to_string())?;
    current()?;
    Ok((output.into_temp_path(), size))
}

pub(super) fn read(path: &Path, size: u64, offset: u64, limit: usize) -> Result<FileChunk, String> {
    if offset > size {
        return Err("Asset offset is past the end".into());
    }
    let length = (size - offset).min(limit as u64) as usize;
    let mut bytes = vec![0; length];
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    let end = offset + length as u64;
    Ok(FileChunk {
        offset,
        size,
        next_offset: (end < size).then_some(end),
        data: encode_file_chunk(&bytes)?,
    })
}

pub(super) fn list(
    root: &Path,
    name: &str,
    current: impl Fn() -> Result<(), String>,
) -> Result<Vec<AssetEntry>, String> {
    let path = confined(root, name)?;
    let directory = fs::read_dir(&path).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    for (index, entry) in directory.enumerate() {
        current()?;
        if index >= 4096 {
            return Err("Asset directory exceeds 4096 children; list a subdirectory".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        let Ok(basename) = entry.file_name().into_string() else {
            continue;
        };
        let relative = if name.is_empty() {
            basename
        } else {
            format!("{name}/{basename}")
        };
        if validate_asset_path(&relative).is_err() {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
        if !metadata.is_file() && !metadata.is_dir() {
            continue;
        }
        entries.push(AssetEntry {
            path: relative,
            directory: metadata.is_dir(),
            size: if metadata.is_file() {
                metadata.len()
            } else {
                0
            },
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    current()?;
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn cancelled_streams_and_size_limits_leave_no_private_snapshots() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("input");
        let storage = root.path().join("storage");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("data.bin"), vec![4; MAX_FILE_CHUNK_BYTES * 3]).unwrap();
        let checkpoints = Cell::new(0);
        let result = snapshot(&source, "data.bin", MAX_ASSET_BYTES, Some(&storage), || {
            checkpoints.set(checkpoints.get() + 1);
            if checkpoints.get() >= 4 {
                Err("Cancelled during copy".into())
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        assert!(checkpoints.get() >= 4);
        assert_eq!(
            fs::read_dir(storage.join("plugins-work")).unwrap().count(),
            0
        );
        assert!(snapshot(&source, "data.bin", 5, Some(&storage), || Ok(())).is_err());
        assert_eq!(
            fs::read_dir(storage.join("plugins-work")).unwrap().count(),
            0
        );
        let (snapshot, size) =
            snapshot(&source, "data.bin", MAX_ASSET_BYTES, Some(&storage), || {
                Ok(())
            })
            .unwrap();
        assert_eq!(size, (MAX_FILE_CHUNK_BYTES * 3) as u64);
        fs::write(source.join("data.bin"), b"changed").unwrap();
        assert_eq!(
            fs::read(&snapshot).unwrap(),
            vec![4; MAX_FILE_CHUNK_BYTES * 3]
        );
        drop(snapshot);
        assert_eq!(
            fs::read_dir(storage.join("plugins-work")).unwrap().count(),
            0
        );
    }
}
