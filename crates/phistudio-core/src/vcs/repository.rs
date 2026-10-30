use super::*;
use crate::{library, storage, versions::objects};
pub struct Repository {
    pub root: PathBuf,
    pub workspace: PathBuf,
    pub state: State,
    _lock: fs::File,
}
impl Drop for Repository {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self._lock);
    }
}
impl Repository {
    pub fn open(root: &Path, workspace: &Path, working: &Working<'_>) -> Result<Self, ChartError> {
        let workspace = library::checked_workspace(root, workspace)?;
        let root = root.canonicalize()?;
        let folder = versions::folder(&root, &workspace)?;
        let lock_path = folder.join("repository.lock");
        if fs::symlink_metadata(&lock_path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(invalid("Invalid repository lock"));
        }
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        fs2::FileExt::lock_exclusive(&lock)?;
        let path = folder.join("repository.json");
        let state = if path.exists() {
            let meta = fs::symlink_metadata(&path)?;
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 4 * 1024 * 1024 {
                return Err(invalid("Invalid project repository"));
            }
            crate::json_io::read(fs::File::open(path)?.take(4 * 1024 * 1024 + 1))?
        } else {
            let old = crate::branches::load(&root, &workspace)?;
            let mut branches: BTreeMap<String, String> = old
                .heads
                .into_iter()
                .filter_map(|(n, id)| id.map(|id| (n, id)))
                .collect();
            let id = if let Some(id) = branches.get(&old.current) {
                id.clone()
            } else {
                let mut initial = tree::capture(&root, &workspace, working, "initial")?;
                initial.name = "Initial version".into();
                initial.system_name = Some("Initial version".into());
                initial.kind = "commit".into();
                objects::write_version(&root, &workspace, &initial)?;
                branches.insert(old.current.clone(), initial.id.clone());
                initial.id
            };
            let state = State {
                version: 1,
                revision: 0,
                head: Head::Branch(old.current),
                branches,
                index: id.clone(),
                tags: BTreeMap::new(),
                stash: vec![],
                reflog: vec![LogEntry {
                    created_ms: now(),
                    message: "Initialize repository".into(),
                    from: None,
                    to: Some(id),
                }],
                author: Author::default(),
                pending: None,
                retained: vec![],
            };
            storage::write_json(&path, &state)?;
            state
        };
        let repo = Self {
            root,
            workspace,
            state,
            _lock: lock,
        };
        repo.validate(&repo.state)?;
        Ok(repo)
    }
    pub(super) fn validate(&self, state: &State) -> Result<(), ChartError> {
        if serde_json::to_vec(state)?.len() >= 4 * 1024 * 1024
            || state.tags.values().any(|t| {
                t.message.len() > 512 || t.author.name.len() > 512 || t.author.email.len() > 512
            })
            || state.stash.iter().any(|s| s.message.len() > 512)
            || state.reflog.iter().any(|r| r.message.len() > 2048)
            || state.version != 1
            || state.branches.len() > 64
            || state.tags.len() > 512
            || state.stash.len() > 64
            || state.reflog.len() > 200
            || state.retained.len() > 8192
            || state.author.name.len() > 512
            || state.author.email.len() > 512
            || state.head_id().is_none()
        {
            return Err(invalid("Invalid project repository"));
        }
        for name in state.branches.keys().chain(state.tags.keys()) {
            if name.is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
                return Err(invalid("Invalid repository reference"));
            }
        }
        let mut ids: BTreeSet<&str> = state.branches.values().map(String::as_str).collect();
        ids.insert(&state.index);
        if let Some(id) = state.head_id() {
            ids.insert(id);
        }
        ids.extend(state.retained.iter().map(String::as_str));
        for tag in state.tags.values() {
            ids.insert(&tag.commit);
        }
        for stash in &state.stash {
            ids.insert(&stash.snapshot);
            ids.insert(&stash.base);
        }
        if let Some(p) = &state.pending {
            ids.extend([p.base.as_str(), p.ours.as_str(), p.theirs.as_str()]);
            ids.extend(p.parents.iter().map(String::as_str));
        }
        for id in ids {
            versions::load(&self.root, &self.workspace, id)?;
        }
        Ok(())
    }
    pub(super) fn save(&mut self, mut after: State) -> Result<(), ChartError> {
        self.validate(&after)?;
        let path = versions::folder(&self.root, &self.workspace)?.join("repository.json");
        let current: State =
            crate::json_io::read(fs::File::open(&path)?.take(4 * 1024 * 1024 + 1))?;
        if current != self.state {
            return Err(invalid("Repository changed; refresh before retrying"));
        }
        after.revision = after
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("Repository revision overflow"))?;
        storage::write_json(&path, &after)?;
        self.state = after;
        Ok(())
    }
    pub(super) fn idle(&self) -> Result<(), ChartError> {
        if self.state.pending.is_some() {
            Err(invalid("Resolve or abort the pending merge first"))
        } else {
            Ok(())
        }
    }
    pub(super) fn set_head(state: &mut State, id: String) {
        match &state.head {
            Head::Branch(name) => {
                state.branches.insert(name.clone(), id);
            }
            Head::Detached(_) => state.head = Head::Detached(id),
        }
    }
    pub(super) fn log(
        state: &mut State,
        message: impl Into<String>,
        from: Option<String>,
        to: Option<String>,
    ) {
        state.reflog.insert(
            0,
            LogEntry {
                created_ms: now(),
                message: message.into(),
                from,
                to,
            },
        );
        state.reflog.truncate(200);
    }
    pub(super) fn promote(
        &self,
        source: &Version,
        message: &str,
        parents: Vec<String>,
        kind: &str,
    ) -> Result<Version, ChartError> {
        if message.trim().is_empty() || message.len() > 512 {
            return Err(invalid("Use a commit message of 1–512 bytes"));
        }
        let mut v = objects::copy_version(&self.root, &self.workspace, source)?;
        v.name = message.trim().into();
        v.system_name = None;
        v.parents = parents;
        v.kind = kind.into();
        v.author = self.state.author.clone();
        objects::write_version(&self.root, &self.workspace, &v)?;
        Ok(v)
    }
    pub(super) fn backup(
        &self,
        working: &Working<'_>,
        message: &str,
    ) -> Result<Version, ChartError> {
        let source = tree::capture(&self.root, &self.workspace, working, "stage")?;
        self.promote(
            &source,
            message,
            self.state
                .head_id()
                .into_iter()
                .map(str::to_owned)
                .collect(),
            "backup",
        )
    }
    pub fn resolve(&self, reference: &str) -> Result<String, ChartError> {
        let id = if reference == "HEAD" {
            self.state.head_id().map(str::to_owned)
        } else if let Some(name) = reference.strip_prefix("branch:") {
            self.state.branches.get(name).cloned()
        } else if let Some(name) = reference.strip_prefix("tag:") {
            self.state.tags.get(name).map(|t| t.commit.clone())
        } else {
            self.state
                .branches
                .get(reference)
                .cloned()
                .or_else(|| self.state.tags.get(reference).map(|t| t.commit.clone()))
                .or_else(|| Some(reference.to_owned()))
        }
        .ok_or_else(|| invalid("Repository has no HEAD"))?;
        versions::load(&self.root, &self.workspace, &id)?;
        Ok(id)
    }
    pub fn status(&self, working: &Working<'_>) -> Result<Status, ChartError> {
        let head = versions::load(
            &self.root,
            &self.workspace,
            self.state
                .head_id()
                .ok_or_else(|| invalid("Repository has no HEAD"))?,
        )?;
        let index = versions::load(&self.root, &self.workspace, &self.state.index)?;
        let a = tree::read(&self.root, &self.workspace, &head)?;
        let b = if head.id == index.id {
            a.clone()
        } else {
            tree::read(&self.root, &self.workspace, &index)?
        };
        let c = tree::working(&self.workspace, working)?;
        let referenced: BTreeSet<&str> = self
            .state
            .branches
            .values()
            .map(String::as_str)
            .chain(self.state.tags.values().map(|t| t.commit.as_str()))
            .chain(self.state.head_id())
            .collect();
        let mut commits = vec![];
        for entry in fs::read_dir(versions::folder(&self.root, &self.workspace)?)? {
            let e = entry?;
            let id = e.file_name().to_string_lossy().into_owned();
            if id.starts_with("version-")
                && let Ok(v) = versions::load(&self.root, &self.workspace, &id)
            {
                if referenced.contains(v.id.as_str())
                    || !matches!(
                        v.kind.as_str(),
                        "stage" | "merge-input" | "stash" | "stash-result"
                    )
                {
                    commits.push(Commit::from(v));
                }
                if commits.len() > 8192 {
                    return Err(invalid("Repository history exceeds 8192 entries"));
                }
            }
        }
        Ok(Status {
            state: self.state.clone(),
            commits,
            staged: tree::difference(&a, &b)?,
            unstaged: tree::difference(&b, &c)?,
            merge: self.merge_preview()?,
        })
    }
    pub fn compare_commit(&self, id: &str) -> Result<Difference, ChartError> {
        let v = versions::load(&self.root, &self.workspace, &self.resolve(id)?)?;
        let current = tree::read(&self.root, &self.workspace, &v)?;
        if let Some(parent) = v.parents.first() {
            let p = versions::load(&self.root, &self.workspace, parent)?;
            tree::difference(&tree::read(&self.root, &self.workspace, &p)?, &current)
        } else {
            let empty = tree::Tree {
                document: ChartDocument::empty("Initial version", 120.)?,
                options: ProjectOptions::default(),
                chart: current.chart.clone(),
                assets: current.assets.clone(),
                files: BTreeMap::new(),
                blobs: BTreeMap::new(),
                folders: vec![],
                project: Value::Null,
            };
            tree::difference(&empty, &current)
        }
    }
}
