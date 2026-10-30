//! Self-contained history archives. Imports add refs and never check out work.
use super::*;
use crate::versions::objects;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};
const MAX_BUNDLE: u64 = 256 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
struct Bundle {
    format: String,
    version: u32,
    exported_ms: u64,
    state: State,
    commits: Vec<Version>,
}
pub struct ImportSummary {
    pub commits: usize,
    pub branches: BTreeMap<String, String>,
    pub tags: BTreeMap<String, String>,
}
pub struct CollectionSummary {
    pub snapshots: usize,
    pub objects: usize,
    pub bytes: u64,
}
impl Repository {
    pub(super) fn reachable(&self) -> Result<BTreeMap<String, Version>, ChartError> {
        let mut todo: Vec<String> = self
            .state
            .branches
            .values()
            .cloned()
            .chain(self.state.tags.values().map(|t| t.commit.clone()))
            .chain(self.state.head_id().map(str::to_owned))
            .chain(Some(self.state.index.clone()))
            .chain(self.state.retained.iter().cloned())
            .collect();
        for stash in &self.state.stash {
            todo.extend([stash.base.clone(), stash.snapshot.clone()]);
        }
        for log in &self.state.reflog {
            todo.extend(log.from.iter().chain(log.to.iter()).cloned());
        }
        if let Some(p) = &self.state.pending {
            todo.extend([p.base.clone(), p.ours.clone(), p.theirs.clone()]);
            todo.extend(p.parents.iter().cloned());
        }
        for entry in fs::read_dir(versions::folder(&self.root, &self.workspace)?)? {
            let entry = entry?;
            let id = entry.file_name().to_string_lossy().into_owned();
            if id.starts_with("version-")
                && let Ok(v) = versions::load(&self.root, &self.workspace, &id)
                && v.kind == "legacy"
            {
                todo.push(id);
            }
        }
        let mut found = BTreeMap::new();
        while let Some(id) = todo.pop() {
            if found.contains_key(&id) {
                continue;
            }
            if found.len() >= 8192 {
                return Err(invalid("Repository history exceeds 8192 entries"));
            }
            let v = versions::load(&self.root, &self.workspace, &id)?;
            todo.extend(v.parents.iter().cloned());
            found.insert(id, v);
        }
        validate_graph(&found)?;
        Ok(found)
    }
    pub fn collect_unused(&mut self) -> Result<CollectionSummary, ChartError> {
        self.idle()?;
        let keep = self.reachable()?;
        let mut hashes = BTreeSet::new();
        for v in keep.values() {
            hashes.extend(v.blobs.values().cloned());
        }
        let parent = versions::folder(&self.root, &self.workspace)?;
        let mut remove = vec![];
        for entry in fs::read_dir(&parent)? {
            let entry = entry?;
            let id = entry.file_name().to_string_lossy().into_owned();
            if id.starts_with("version-") && !keep.contains_key(&id) {
                // Refuse cleanup when a manifest is unreadable; its objects may be
                // the only remaining copy of work that can still be recovered.
                versions::load(&self.root, &self.workspace, &id)?;
                remove.push(id);
            }
        }
        let mut result = CollectionSummary {
            snapshots: 0,
            objects: 0,
            bytes: 0,
        };
        let mut after = self.state.clone();
        Self::log(
            &mut after,
            "Collect unused objects",
            self.state.head_id().map(str::to_owned),
            self.state.head_id().map(str::to_owned),
        );
        self.save(after)?;
        for id in remove {
            fs::remove_dir_all(parent.join(id))?;
            result.snapshots += 1;
        }
        for entry in fs::read_dir(objects::objects(&self.root, &self.workspace)?)? {
            let e = entry?;
            let name = e.file_name().to_string_lossy().into_owned();
            if objects::valid_hash(&name) && !hashes.contains(&name) {
                let path = objects::object_path(&self.root, &self.workspace, &name)?;
                result.bytes += fs::metadata(&path)?.len();
                fs::remove_file(path)?;
                result.objects += 1;
            }
        }
        Ok(result)
    }
}
fn validate_graph(commits: &BTreeMap<String, Version>) -> Result<(), ChartError> {
    let mut remaining: BTreeMap<String, usize> = commits.keys().map(|id| (id.clone(), 0)).collect();
    for v in commits.values() {
        for parent in &v.parents {
            let n = remaining
                .get_mut(parent)
                .ok_or_else(|| invalid("History references a missing parent commit"))?;
            *n += 1;
        }
    }
    let mut todo: Vec<_> = remaining
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(id, _)| id.clone())
        .collect();
    let mut count = 0;
    while let Some(id) = todo.pop() {
        count += 1;
        for p in &commits[&id].parents {
            let n = remaining.get_mut(p).unwrap();
            *n -= 1;
            if *n == 0 {
                todo.push(p.clone());
            }
        }
    }
    if count != commits.len() {
        return Err(invalid("History contains a parent cycle"));
    }
    Ok(())
}
pub fn export_bundle(repo: &Repository) -> Result<crate::ProjectExport, ChartError> {
    let mut commits = repo.reachable()?;
    let mut hashes = BTreeSet::new();
    for v in commits.values_mut() {
        if v.version == 1 {
            let old = v.clone();
            v.blobs.clear();
            let mut folders = BTreeSet::new();
            for name in old.files.keys() {
                let bytes = objects::read_file(&repo.root, &repo.workspace, &old, name)?;
                let (_, hash) = objects::put(&repo.root, &repo.workspace, &mut bytes.as_slice())?;
                v.blobs.insert(name.clone(), hash);
                for p in Path::new(name).ancestors().skip(1) {
                    if !p.as_os_str().is_empty() {
                        folders.insert(p.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
            v.folders = folders.into_iter().collect();
            v.version = 2;
        }
        hashes.extend(v.blobs.values().cloned());
    }
    let bundle = Bundle {
        format: "phistudio-project-vcs".into(),
        version: 1,
        exported_ms: now(),
        state: repo.state.clone(),
        commits: commits.into_values().collect(),
    };
    let bytes = serde_json::to_vec(&bundle)?;
    if bytes.len() > 32 * 1024 * 1024 || hashes.len() > 32768 {
        return Err(invalid("Repository bundle exceeds the metadata limit"));
    }
    let cache = repo.root.join("export-cache");
    fs::create_dir_all(&cache)?;
    let mut file = tempfile::Builder::new()
        .prefix("phistudio-history-")
        .suffix(".zip")
        .tempfile_in(cache)?;
    {
        let mut zip = ZipWriter::new(file.as_file_mut());
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("repository.json", options)
            .map_err(|e| invalid(&e.to_string()))?;
        zip.write_all(&bytes)?;
        let mut total = bytes.len() as u64;
        for hash in hashes {
            let path = objects::object_path(&repo.root, &repo.workspace, &hash)?;
            let mut input = fs::File::open(path)?;
            zip.start_file(format!("objects/{hash}"), options)
                .map_err(|e| invalid(&e.to_string()))?;
            // Verify before exporting a shared object, then stream its bytes.
            let (fp, actual) = objects::fingerprint(&mut input)?;
            if actual != hash {
                return Err(invalid("Version object checksum mismatch"));
            }
            total += fp.bytes;
            if total > 1032 * 1024 * 1024 {
                return Err(invalid(
                    "Repository bundle exceeds the uncompressed size limit",
                ));
            }
            std::io::Seek::rewind(&mut input)?;
            std::io::copy(&mut input, &mut zip)?;
        }
        zip.finish()
            .map_err(|e| invalid(&e.to_string()))?
            .sync_all()?;
    }
    if file.as_file().metadata()?.len() > MAX_BUNDLE {
        return Err(invalid("Repository bundle exceeds 256 MiB"));
    }
    Ok(crate::ProjectExport {
        file,
        name: "Project history.phiversions.zip".into(),
    })
}
fn unique_name(base: &str, used: &BTreeSet<String>) -> String {
    if !used.contains(base) {
        return base.into();
    }
    for n in 2..10000 {
        let suffix = format!("-import-{n}");
        let mut stem = base.to_owned();
        while stem.len() + suffix.len() > 128 {
            stem.pop();
        }
        let name = format!("{stem}{suffix}");
        if !used.contains(&name) {
            return name;
        }
    }
    unreachable!()
}
pub fn import_bundle(repo: &mut Repository, path: &Path) -> Result<ImportSummary, ChartError> {
    repo.idle()?;
    if fs::metadata(path)?.len() > MAX_BUNDLE {
        return Err(invalid("Repository bundle exceeds 256 MiB"));
    }
    let mut zip = ZipArchive::new(fs::File::open(path)?)
        .map_err(|_| invalid("Not a PhiStudio history bundle"))?;
    if zip.len() > 32769 {
        return Err(invalid("Repository bundle contains too many objects"));
    }
    let mut seen = BTreeSet::new();
    let mut total = 0;
    for i in 0..zip.len() {
        let e = zip.by_index(i).map_err(|e| invalid(&e.to_string()))?;
        let name = e.name().to_owned();
        if !seen.insert(name.clone())
            || e.is_dir()
            || e.unix_mode()
                .is_some_and(|m| ![0, 0o100000].contains(&(m & 0o170000)))
            || (name != "repository.json"
                && !name
                    .strip_prefix("objects/")
                    .is_some_and(objects::valid_hash))
            || e.size() > 128 * 1024 * 1024
        {
            return Err(invalid("Invalid entry in repository bundle"));
        }
        total += e.size();
        if total > 1032 * 1024 * 1024 {
            return Err(invalid(
                "Repository bundle exceeds the uncompressed size limit",
            ));
        }
    }
    let bundle: Bundle = {
        let e = zip
            .by_name("repository.json")
            .map_err(|_| invalid("Repository bundle manifest is missing"))?;
        if e.size() > 32 * 1024 * 1024 {
            return Err(invalid("Repository bundle metadata exceeds 32 MiB"));
        }
        serde_json::from_reader(e.take(32 * 1024 * 1024 + 1))?
    };
    if bundle.format != "phistudio-project-vcs"
        || bundle.version != 1
        || bundle.commits.len() > 8192
    {
        return Err(invalid("Unsupported repository bundle"));
    }
    let mut old = BTreeMap::new();
    for v in bundle.commits {
        if v.version != 2
            || v.id.len() > 128
            || !v.id.starts_with("version-")
            || !v.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || v.name.len() > 512
            || v.author.name.len() > 512
            || v.author.email.len() > 512
            || v.kind.len() > 64
            || v.files.len() > 2049
            || v.files.len() != v.blobs.len()
            || v.folders.len() > 65536
            || v.parents.len() > 2
        {
            return Err(invalid("Invalid bundled commit"));
        }
        crate::package::relative(&v.chart.to_string_lossy())?;
        crate::package::relative(&v.assets.to_string_lossy())?;
        for n in v.files.keys().chain(v.folders.iter()) {
            crate::package::relative(n)?;
        }
        for (n, h) in &v.blobs {
            if !v.files.contains_key(n)
                || !objects::valid_hash(h)
                || !seen.contains(&format!("objects/{h}"))
            {
                return Err(invalid("Bundled commit references a missing object"));
            }
        }
        if old.insert(v.id.clone(), v).is_some() {
            return Err(invalid("Duplicate bundled commit identity"));
        }
    }
    validate_graph(&old)?;
    let incoming = bundle.state;
    let mut references: Vec<&str> = incoming
        .branches
        .values()
        .map(String::as_str)
        .chain(incoming.tags.values().map(|t| t.commit.as_str()))
        .chain(incoming.head_id())
        .chain(Some(incoming.index.as_str()))
        .chain(incoming.retained.iter().map(String::as_str))
        .collect();
    for stash in &incoming.stash {
        references.extend([stash.snapshot.as_str(), stash.base.as_str()]);
    }
    for entry in &incoming.reflog {
        references.extend(entry.from.iter().chain(entry.to.iter()).map(String::as_str));
    }
    if let Some(p) = &incoming.pending {
        references.extend([p.base.as_str(), p.ours.as_str(), p.theirs.as_str()]);
        references.extend(p.parents.iter().map(String::as_str));
    }
    if incoming.version != 1
        || incoming.head_id().is_none()
        || incoming.branches.len() + repo.state.branches.len() > 64
        || incoming.tags.len() + repo.state.tags.len() > 512
        || incoming.stash.len() + repo.state.stash.len() > 64
        || references.iter().any(|id| !old.contains_key(*id))
        || incoming
            .branches
            .keys()
            .chain(incoming.tags.keys())
            .any(|n| n.is_empty() || n.len() > 128 || n.chars().any(char::is_control))
    {
        return Err(invalid("Invalid bundled repository references"));
    }
    let mut hashes = BTreeSet::new();
    for v in old.values() {
        hashes.extend(v.blobs.values().cloned());
    }
    if seen.len() != hashes.len() + 1 {
        return Err(invalid("Repository bundle contains unreferenced objects"));
    }
    let mut actual_files = BTreeMap::new();
    for hash in &hashes {
        let mut entry = zip
            .by_name(&format!("objects/{hash}"))
            .map_err(|_| invalid("Repository bundle object is missing"))?;
        let (fp, actual) = objects::put(
            &repo.root,
            &repo.workspace,
            &mut (&mut entry).take(128 * 1024 * 1024 + 1),
        )?;
        if actual != *hash {
            return Err(invalid("Repository bundle object hash mismatch"));
        }
        actual_files.insert(hash.clone(), fp);
    }
    for v in old.values() {
        let mut names = BTreeSet::new();
        let mut bytes = 0;
        for (name, fp) in &v.files {
            let normalized = crate::package::relative(name)?;
            let key = normalized.to_string_lossy().replace('\\', "/");
            if key.is_empty() || !names.insert(key) || actual_files.get(&v.blobs[name]) != Some(fp)
            {
                return Err(invalid(
                    "Bundled file fingerprint does not match its object",
                ));
            }
            bytes += fp.bytes;
            if bytes > 1032 * 1024 * 1024 {
                return Err(invalid("Bundled version exceeds the workspace size limit"));
            }
        }
        if !v
            .files
            .contains_key(&v.chart.to_string_lossy().replace('\\', "/"))
        {
            return Err(invalid("Bundled version chart is missing"));
        }
        for name in &names {
            for parent in Path::new(name).ancestors().skip(1) {
                if names.contains(&parent.to_string_lossy().replace('\\', "/")) {
                    return Err(invalid("Bundled file paths overlap"));
                }
            }
        }
        for folder in &v.folders {
            let path = crate::package::relative(folder)?;
            if names.contains(&path.to_string_lossy().replace('\\', "/")) {
                return Err(invalid("Bundled folder conflicts with a file"));
            }
        }
    }
    let parent = versions::folder(&repo.root, &repo.workspace)?;
    let mut stages = vec![];
    let mut mapping = BTreeMap::new();
    for id in old.keys() {
        let stage = tempfile::Builder::new()
            .prefix("version-")
            .tempdir_in(&parent)?;
        mapping.insert(
            id.clone(),
            stage
                .path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        );
        stages.push(stage);
    }
    for v in old.values() {
        let mut v = v.clone();
        v.id = mapping[&v.id].clone();
        v.parents = v.parents.iter().map(|id| mapping[id].clone()).collect();
        let dir = parent.join(&v.id);
        objects::check_metadata(&v)?;
        validate_layout(repo, &v)?;
        crate::storage::write_json(&dir.join("version.json"), &v)?;
        // Verify chart shape and per-file fingerprints before adding any refs.
        tree::read(&repo.root, &repo.workspace, &v)?;
    }
    let mut after = repo.state.clone();
    let mut branch_names = after.branches.keys().cloned().collect();
    let mut tag_names = after.tags.keys().cloned().collect();
    let mut result = ImportSummary {
        commits: old.len(),
        branches: BTreeMap::new(),
        tags: BTreeMap::new(),
    };
    for (name, id) in incoming.branches {
        let next = unique_name(&name, &branch_names);
        branch_names.insert(next.clone());
        after.branches.insert(next.clone(), mapping[&id].clone());
        result.branches.insert(name, next);
    }
    if let Head::Detached(id) = incoming.head {
        let name = unique_name("imported-HEAD", &branch_names);
        after.branches.insert(name.clone(), mapping[&id].clone());
        result.branches.insert("HEAD".into(), name);
    }
    for (name, mut tag) in incoming.tags {
        let next = unique_name(&name, &tag_names);
        tag_names.insert(next.clone());
        tag.commit = mapping[&tag.commit].clone();
        after.tags.insert(next.clone(), tag);
        result.tags.insert(name, next);
    }
    for mut stash in incoming.stash {
        stash.id = mapping[&stash.snapshot].clone();
        stash.snapshot = mapping[&stash.snapshot].clone();
        stash.base = mapping[&stash.base].clone();
        after.stash.push(stash);
    }
    // Keep imported rescue/staged/pending snapshots reachable even though import
    // deliberately leaves the local HEAD, index and editor work untouched.
    after.retained.extend(mapping.values().cloned());
    after.retained.sort();
    after.retained.dedup();
    if after.branches.len() > 64 || after.tags.len() > 512 || after.retained.len() > 8192 {
        return Err(invalid(
            "Imported repository exceeds the local reference limit",
        ));
    }
    Repository::log(
        &mut after,
        "Import repository bundle",
        repo.state.head_id().map(str::to_owned),
        repo.state.head_id().map(str::to_owned),
    );
    repo.save(after)?;
    for stage in stages {
        let _ = stage.keep();
    }
    Ok(result)
}

fn validate_layout(repo: &Repository, v: &Version) -> Result<(), ChartError> {
    let stage = tempfile::tempdir_in(versions::folder(&repo.root, &repo.workspace)?)?;
    for folder in &v.folders {
        fs::create_dir_all(stage.path().join(crate::package::relative(folder)?))?;
    }
    fs::create_dir_all(stage.path().join(&v.assets))?;
    for (name, hash) in &v.blobs {
        let target = stage.path().join(crate::package::relative(name)?);
        fs::create_dir_all(target.parent().unwrap())?;
        let source = objects::object_path(&repo.root, &repo.workspace, hash)?;
        if fs::hard_link(&source, &target).is_err() {
            fs::copy(source, &target)?;
        }
    }
    crate::package::generation_description(stage.path(), &v.chart, &v.assets, v.options.clone())?;
    Ok(())
}
