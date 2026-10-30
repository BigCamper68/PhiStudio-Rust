//! Immutable, verified project-local objects. Version 1 ZIPs remain readable.
use super::*;
use sha2::{Digest, Sha256};

pub(crate) fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn objects(root: &Path, workspace: &Path) -> Result<PathBuf, ChartError> {
    let parent = folder(root, workspace)?;
    let path = parent.join("objects");
    fs::create_dir_all(&path)?;
    if fs::symlink_metadata(&path)?.file_type().is_symlink()
        || !path.canonicalize()?.starts_with(parent.canonicalize()?)
    {
        return Err(invalid("Invalid version object directory"));
    }
    Ok(path)
}
fn copy_hash(
    input: &mut impl Read,
    output: &mut impl Write,
) -> Result<(Fingerprint, String), ChartError> {
    let mut sha = Sha256::new();
    let mut fp = Fingerprint {
        bytes: 0,
        hash: 0xcbf29ce484222325,
    };
    let mut buffer = [0u8; 65536];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        fp.bytes += n as u64;
        if fp.bytes > MAX_FILE {
            return Err(invalid("Version file exceeds 128 MiB"));
        }
        sha.update(&buffer[..n]);
        for b in &buffer[..n] {
            fp.hash = (fp.hash ^ u64::from(*b)).wrapping_mul(0x100000001b3);
        }
        output.write_all(&buffer[..n])?;
    }
    Ok((fp, format!("{:x}", sha.finalize())))
}
pub(crate) fn fingerprint(input: &mut impl Read) -> Result<(Fingerprint, String), ChartError> {
    copy_hash(input, &mut std::io::sink())
}
pub(crate) fn object_path(
    root: &Path,
    workspace: &Path,
    hash: &str,
) -> Result<PathBuf, ChartError> {
    if !valid_hash(hash) {
        return Err(invalid("Invalid version object hash"));
    }
    let path = objects(root, workspace)?.join(hash);
    let m = fs::symlink_metadata(&path)?;
    if !m.is_file() || m.file_type().is_symlink() || m.len() > MAX_FILE {
        return Err(invalid("Invalid version object"));
    }
    Ok(path)
}
pub(crate) fn put(
    root: &Path,
    workspace: &Path,
    input: &mut impl Read,
) -> Result<(Fingerprint, String), ChartError> {
    let mut stage = tempfile::NamedTempFile::new_in(objects(root, workspace)?)?;
    let (fp, hash) = copy_hash(input, stage.as_file_mut())?;
    stage.as_file().sync_all()?;
    let target = objects(root, workspace)?.join(&hash);
    match stage.persist_noclobber(&target) {
        Ok(_) => {}
        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
            let path = object_path(root, workspace, &hash)?;
            if copy_hash(&mut fs::File::open(path)?, &mut std::io::sink())?
                != (fp.clone(), hash.clone())
            {
                return Err(invalid("Existing version object is corrupt"));
            }
        }
        Err(e) => return Err(e.error.into()),
    }
    Ok((fp, hash))
}
pub(crate) fn folders(base: &Path) -> Result<Vec<String>, ChartError> {
    fn walk(
        base: &Path,
        dir: &Path,
        out: &mut Vec<String>,
        depth: usize,
        count: &mut usize,
    ) -> Result<(), ChartError> {
        if depth > 64 {
            return Err(invalid("Version folders are nested too deeply"));
        }
        for e in fs::read_dir(dir)? {
            *count += 1;
            if *count > 70000 {
                return Err(invalid("Version contains too many filesystem entries"));
            }
            let e = e?;
            let kind = e.file_type()?;
            if kind.is_symlink() {
                return Err(invalid("Version contains a symbolic link"));
            }
            if kind.is_dir() && !crate::package::private_vcs_path(&e.path()) {
                out.push(
                    e.path()
                        .strip_prefix(base)
                        .unwrap()
                        .to_str()
                        .ok_or_else(|| invalid("Invalid version folder name"))?
                        .replace('\\', "/"),
                );
                walk(base, &e.path(), out, depth + 1, count)?;
            }
        }
        Ok(())
    }
    let mut out = vec![];
    walk(base, base, &mut out, 0, &mut 0)?;
    out.sort();
    Ok(out)
}
pub(crate) fn read_file(
    root: &Path,
    workspace: &Path,
    version: &Version,
    name: &str,
) -> Result<Vec<u8>, ChartError> {
    package::relative(name)?;
    let fp = version
        .files
        .get(name)
        .ok_or_else(|| invalid("Version file is missing"))?;
    let mut bytes = Vec::new();
    if version.version == 2 {
        let hash = version
            .blobs
            .get(name)
            .ok_or_else(|| invalid("Version object is missing"))?;
        let actual = copy_hash(
            &mut fs::File::open(object_path(root, workspace, hash)?)?,
            &mut bytes,
        )?;
        if actual != (fp.clone(), hash.clone()) {
            return Err(invalid("Version object checksum mismatch"));
        }
    } else {
        let path = directory(root, workspace, &version.id)?.join("workspace.zip");
        if fs::symlink_metadata(&path)?.file_type().is_symlink() {
            return Err(invalid("Invalid version archive"));
        }
        let mut archive =
            zip::ZipArchive::new(fs::File::open(path)?).map_err(|e| invalid(e.to_string()))?;
        let mut entry = archive.by_name(name).map_err(|e| invalid(e.to_string()))?;
        let actual = transfer(&mut (&mut entry).take(MAX_FILE + 1), &mut bytes)?;
        if actual != *fp {
            return Err(invalid("Version data does not match its manifest"));
        }
    }
    Ok(bytes)
}
pub(crate) fn expand(
    root: &Path,
    workspace: &Path,
    v: &Version,
    destination: &Path,
) -> Result<(), ChartError> {
    for name in &v.folders {
        fs::create_dir_all(destination.join(package::relative(name)?))?;
    }
    let mut total = 0;
    for (name, fp) in &v.files {
        let hash = v
            .blobs
            .get(name)
            .ok_or_else(|| invalid("Version object is missing"))?;
        let target = destination.join(package::relative(name)?);
        fs::create_dir_all(target.parent().unwrap())?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        let actual = copy_hash(
            &mut fs::File::open(object_path(root, workspace, hash)?)?,
            &mut output,
        )?;
        output.sync_all()?;
        total += actual.0.bytes;
        if total > MAX_TOTAL || actual != (fp.clone(), hash.clone()) {
            return Err(invalid("Version object checksum mismatch"));
        }
    }
    Ok(())
}
pub(crate) fn copy_version(
    root: &Path,
    workspace: &Path,
    source: &Version,
) -> Result<Version, ChartError> {
    let mut result = source.clone();
    if result.version == 1 {
        let mut blobs = BTreeMap::new();
        for name in result.files.keys() {
            let bytes = read_file(root, workspace, source, name)?;
            let (_, hash) = put(root, workspace, &mut bytes.as_slice())?;
            blobs.insert(name.clone(), hash);
        }
        result.version = 2;
        result.blobs = blobs;
    }
    let stage = tempfile::Builder::new()
        .prefix("version-")
        .tempdir_in(folder(root, workspace)?)?;
    result.id = stage
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    result.created_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    check_metadata(&result)?;
    storage::write_json(&stage.path().join("version.json"), &result)?;
    let _ = stage.keep();
    Ok(result)
}
pub(crate) fn check_metadata(version: &Version) -> Result<(), ChartError> {
    if serde_json::to_vec(version)?.len() >= 4 * 1024 * 1024 {
        return Err(invalid("Version metadata exceeds 4 MiB"));
    }
    Ok(())
}
pub(crate) fn write_version(
    root: &Path,
    workspace: &Path,
    version: &Version,
) -> Result<(), ChartError> {
    check_metadata(version)?;
    storage::write_json(
        &directory(root, workspace, &version.id)?.join("version.json"),
        version,
    )
}
