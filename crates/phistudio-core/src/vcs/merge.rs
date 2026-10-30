use super::*;
use crate::{
    package,
    versions::{Fingerprint, objects},
};
impl Repository {
    fn ancestors(&self, id: &str) -> Result<BTreeMap<String, usize>, ChartError> {
        let mut found = BTreeMap::new();
        let mut todo = std::collections::VecDeque::from([(id.to_owned(), 0)]);
        while let Some((id, depth)) = todo.pop_front() {
            if found.contains_key(&id) {
                continue;
            }
            if found.len() >= 8192 {
                return Err(invalid("Repository ancestry exceeds 8192 entries"));
            }
            let v = versions::load(&self.root, &self.workspace, &id)?;
            found.insert(id, depth);
            for parent in v.parents {
                todo.push_back((parent, depth + 1));
            }
        }
        Ok(found)
    }
    fn merge_base(&self, a: &str, b: &str) -> Result<String, ChartError> {
        let aa = self.ancestors(a)?;
        let bb = self.ancestors(b)?;
        aa.keys()
            .filter(|id| bb.contains_key(*id))
            .min_by_key(|id| (aa[*id] + bb[*id], aa[*id].max(bb[*id]), *id))
            .cloned()
            .ok_or_else(|| invalid("Versions have no common ancestor"))
    }
    pub fn merge(&mut self, reference: &str, working: &Working<'_>) -> Result<Outcome, ChartError> {
        self.idle()?;
        if !self.clean(working)? {
            return Err(invalid(
                "Commit or stash the staged and unstaged changes before merging",
            ));
        }
        let theirs = self.resolve(reference)?;
        let head = self.state.head_id().unwrap().to_owned();
        if self.ancestors(&head)?.contains_key(&theirs) {
            return Ok(Outcome::Unchanged);
        }
        if self.ancestors(&theirs)?.contains_key(&head) {
            let target = versions::load(&self.root, &self.workspace, &theirs)?;
            let copy = worktree::create(&self.root, &self.workspace, &target, working)?;
            let mut after = self.state.clone();
            Self::set_head(&mut after, theirs.clone());
            after.index = theirs.clone();
            Self::log(&mut after, "Fast-forward merge", Some(head), Some(theirs));
            self.save(after)?;
            return Ok(Outcome::WorkingCopy(copy));
        }
        let base = self.merge_base(&head, &theirs)?;
        let ours = tree::capture(&self.root, &self.workspace, working, "merge-input")?;
        self.begin_merge(
            PendingMerge {
                base,
                ours: ours.id,
                theirs: theirs.clone(),
                parents: vec![head, theirs],
                kind: MergeKind::Merge,
                target: reference.into(),
                message: format!("Merge {reference}"),
                stash: None,
            },
            working,
        )
    }
    pub fn cherry_pick(&mut self, id: &str, working: &Working<'_>) -> Result<Outcome, ChartError> {
        self.apply_commit(id, working, false)
    }
    pub fn revert(&mut self, id: &str, working: &Working<'_>) -> Result<Outcome, ChartError> {
        self.apply_commit(id, working, true)
    }
    fn apply_commit(
        &mut self,
        id: &str,
        working: &Working<'_>,
        revert: bool,
    ) -> Result<Outcome, ChartError> {
        self.idle()?;
        if !self.clean(working)? {
            return Err(invalid(
                "Commit or stash the staged and unstaged changes before applying a commit",
            ));
        }
        let id = self.resolve(id)?;
        let selected = versions::load(&self.root, &self.workspace, &id)?;
        let parent = if let Some(parent) = selected.parents.first() {
            parent.clone()
        } else {
            self.empty_base(&selected)?.id
        };
        let ours = tree::capture(&self.root, &self.workspace, working, "merge-input")?;
        self.begin_merge(
            PendingMerge {
                base: if revert { id.clone() } else { parent.clone() },
                ours: ours.id,
                theirs: if revert { parent } else { id.clone() },
                parents: vec![self.state.head_id().unwrap().into()],
                kind: if revert {
                    MergeKind::Revert
                } else {
                    MergeKind::CherryPick
                },
                target: id,
                message: operation_message(
                    if revert { "Revert" } else { "Cherry-pick" },
                    &selected.name,
                ),
                stash: None,
            },
            working,
        )
    }
    fn empty_base(&self, layout: &Version) -> Result<Version, ChartError> {
        let parent = self.root.join("staging");
        fs::create_dir_all(&parent)?;
        let stage = tempfile::tempdir_in(parent)?;
        let doc = ChartDocument::empty("Initial version", 120.)?;
        let chart = stage.path().join(&layout.chart);
        fs::create_dir_all(chart.parent().unwrap())?;
        crate::save_chart(&doc, &chart)?;
        fs::create_dir_all(stage.path().join(&layout.assets))?;
        let mut v = versions::snapshot(
            &self.root,
            &self.workspace,
            "Empty base",
            &doc,
            stage.path(),
            layout.chart.clone(),
            layout.assets.clone(),
            &ProjectOptions::default(),
            vec![],
        )?;
        v.kind = "merge-input".into();
        objects::write_version(&self.root, &self.workspace, &v)?;
        Ok(v)
    }
    pub fn stash_apply(
        &mut self,
        id: &str,
        working: &Working<'_>,
        pop: bool,
    ) -> Result<Outcome, ChartError> {
        self.idle()?;
        let stash = self
            .state
            .stash
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| invalid("Stash no longer exists"))?;
        let ours = tree::capture(&self.root, &self.workspace, working, "merge-input")?;
        self.begin_merge(
            PendingMerge {
                base: stash.base,
                ours: ours.id,
                theirs: stash.snapshot,
                parents: vec![],
                kind: if pop {
                    MergeKind::StashPop
                } else {
                    MergeKind::StashApply
                },
                target: id.into(),
                message: operation_message("Apply stash", &stash.message),
                stash: Some(id.into()),
            },
            working,
        )
    }
    fn begin_merge(
        &mut self,
        pending: PendingMerge,
        working: &Working<'_>,
    ) -> Result<Outcome, ChartError> {
        let preview = self.preview_pending(&pending)?;
        let mut after = self.state.clone();
        after.pending = Some(pending);
        self.save(after)?;
        if preview.conflicts.is_empty() {
            self.resolve_merge(working, &BTreeMap::new())
        } else {
            Ok(Outcome::Conflicts(preview))
        }
    }
    fn merged(
        &self,
        pending: &PendingMerge,
        choices: &BTreeMap<String, Side>,
    ) -> Result<(Value, Vec<Conflict>, tree::Tree, tree::Tree), ChartError> {
        let b = tree::read(
            &self.root,
            &self.workspace,
            &versions::load(&self.root, &self.workspace, &pending.base)?,
        )?;
        let o = tree::read(
            &self.root,
            &self.workspace,
            &versions::load(&self.root, &self.workspace, &pending.ours)?,
        )?;
        let t = tree::read(
            &self.root,
            &self.workspace,
            &versions::load(&self.root, &self.workspace, &pending.theirs)?,
        )?;
        let mut base = tree::envelope(&b)?;
        let mut ours = tree::envelope(&o)?;
        let mut theirs = tree::envelope(&t)?;
        let mut conflicts = vec![];
        let chart = super::line_merge::chart(
            &base["chart"],
            &ours["chart"],
            &theirs["chart"],
            choices,
            &mut conflicts,
        )?;
        let files = merge_files(
            &base["files"],
            &ours["files"],
            &theirs["files"],
            choices,
            &mut conflicts,
        )?;
        let sets: Vec<BTreeSet<String>> = [&base, &ours, &theirs]
            .into_iter()
            .map(|v| serde_json::from_value(v["folders"].clone()))
            .collect::<Result<_, _>>()?;
        let folders: Vec<String> = sets
            .iter()
            .flat_map(|s| s.iter())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|name| {
                let (b, o, t) = (
                    sets[0].contains(*name),
                    sets[1].contains(*name),
                    sets[2].contains(*name),
                );
                if o == b { t } else { o }
            })
            .cloned()
            .collect();
        for envelope in [&mut base, &mut ours, &mut theirs] {
            envelope["files"] = Value::Null;
            envelope["folders"] = Value::Null;
        }
        base["chart"] = Value::Null;
        ours["chart"] = Value::Null;
        theirs["chart"] = Value::Null;
        let mut merged = crate::branches::merge_logic::merge(
            Some(&base),
            Some(&ours),
            Some(&theirs),
            "",
            choices,
            &mut conflicts,
            0,
        )?
        .unwrap();
        merged["chart"] = chart;
        merged["files"] = files;
        merged["folders"] = json!(folders);
        Ok((merged, conflicts, o, t))
    }
    fn preview_pending(&self, pending: &PendingMerge) -> Result<MergePreview, ChartError> {
        let (_, conflicts, _, _) = self.merged(pending, &BTreeMap::new())?;
        Ok(MergePreview {
            pending: pending.clone(),
            conflicts,
        })
    }
    pub fn merge_preview(&self) -> Result<Option<MergePreview>, ChartError> {
        self.state
            .pending
            .as_ref()
            .map(|p| self.preview_pending(p))
            .transpose()
    }
    pub fn resolve_merge(
        &mut self,
        working: &Working<'_>,
        choices: &BTreeMap<String, Side>,
    ) -> Result<Outcome, ChartError> {
        let pending = self
            .state
            .pending
            .clone()
            .ok_or_else(|| invalid("There is no pending merge"))?;
        let (merged, conflicts, ours, theirs) = self.merged(&pending, choices)?;
        if conflicts.iter().any(|c| c.choice.is_none()) {
            return Err(invalid("Choose a side for every merge conflict"));
        }
        if !tree::same(&ours, &tree::working(&self.workspace, working)?)? {
            return Err(invalid(
                "Working project changed; abort and prepare the merge again",
            ));
        }
        let document = ChartDocument::from_value(merged["chart"].clone())?;
        let options: ProjectOptions = serde_json::from_value(merged["options"].clone())?;
        let chart: PathBuf = serde_json::from_value(merged["chartName"].clone())?;
        let chart = package::relative(&chart.to_string_lossy())?;
        if chart.as_os_str().is_empty() {
            return Err(invalid("Merged chart filename is empty"));
        }
        let assets: PathBuf = serde_json::from_value(merged["assets"].clone())?;
        let assets = package::relative(&assets.to_string_lossy())?;
        let assets = if assets.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            assets
        };
        let parent = self.root.join("staging");
        fs::create_dir_all(&parent)?;
        let stage = tempfile::Builder::new()
            .prefix("vcs-merge-")
            .tempdir_in(parent)?;
        let folders: Vec<String> = serde_json::from_value(merged["folders"].clone())?;
        if folders.len() > 65536 {
            return Err(invalid("Merged project has too many folders"));
        }
        for folder in folders {
            fs::create_dir_all(stage.path().join(package::relative(&folder)?))?;
        }
        fs::create_dir_all(stage.path().join(&assets))?;
        let files = merged["files"]
            .as_object()
            .ok_or_else(|| invalid("Invalid merged resources"))?;
        if files.len() > 2048 {
            return Err(invalid("Merged project has too many files"));
        }
        let ours_v = versions::load(&self.root, &self.workspace, &pending.ours)?;
        let theirs_v = versions::load(&self.root, &self.workspace, &pending.theirs)?;
        let oe = tree::envelope(&ours)?;
        let te = tree::envelope(&theirs)?;
        let mut total = 0;
        for (name, value) in files {
            let relative = package::relative(name)?;
            let source = if oe["files"].get(name) == Some(value) {
                &ours_v
            } else if te["files"].get(name) == Some(value) {
                &theirs_v
            } else {
                return Err(invalid("Merged resource has no verified source"));
            };
            let bytes = objects::read_file(&self.root, &self.workspace, source, name)?;
            let (fp, hash) = objects::fingerprint(&mut bytes.as_slice())?;
            let expected: Fingerprint = serde_json::from_value(value.clone())?;
            if fp != expected || value["sha256"].as_str() != Some(&hash) {
                return Err(invalid("Merge resource checksum mismatch"));
            }
            total += fp.bytes;
            if total > 1032 * 1024 * 1024 {
                return Err(invalid("Merged project exceeds the size limit"));
            }
            let target = stage.path().join(relative);
            fs::create_dir_all(target.parent().unwrap())?;
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)?;
            f.write_all(&bytes)?;
        }
        let target = stage.path().join(&chart);
        if target.exists() {
            return Err(invalid("Merged chart filename conflicts with a resource"));
        }
        fs::create_dir_all(target.parent().unwrap())?;
        fs::write(target, serde_json::to_vec(document.as_json())?)?;
        let mut project = ours.project.clone();
        if let Some(charts) = project.get_mut("charts").and_then(Value::as_array_mut) {
            for entry in charts {
                if entry["working"]
                    .as_str()
                    .is_some_and(|p| Path::new(p) == ours.chart)
                {
                    entry["working"] = json!(chart);
                    if entry["source"]
                        .as_str()
                        .is_some_and(|p| !stage.path().join(p).is_file())
                    {
                        entry["source"] = json!(chart);
                    }
                }
            }
        }
        let project_path = stage.path().join(".phistudio-workspace.json");
        if project.is_object() {
            crate::storage::write_json(&project_path, &project)?;
        }
        package::generation_description(stage.path(), &chart, &assets, options.clone())?;
        let native: Value =
            crate::json_io::read(fs::File::open(&project_path)?.take(4 * 1024 * 1024 + 1))?;
        let project = tree::restore_project(&native, &merged["project"]);
        crate::storage::write_json(&project_path, &project)?;
        for path in [&options.music, &options.illustration]
            .into_iter()
            .flatten()
        {
            let relative = package::relative(path)?;
            if !stage.path().join(&assets).join(relative).is_file() {
                return Err(invalid(
                    "Merged project references a missing music or illustration file",
                ));
            }
        }
        let mut result = versions::snapshot(
            &self.root,
            &self.workspace,
            &pending.message,
            &document,
            stage.path(),
            chart,
            assets,
            &options,
            pending.parents.clone(),
        )?;
        result.author = self.state.author.clone();
        result.kind = match pending.kind {
            MergeKind::Merge => "merge",
            MergeKind::CherryPick => "cherry-pick",
            MergeKind::Revert => "revert",
            _ => "stash-result",
        }
        .into();
        objects::write_version(&self.root, &self.workspace, &result)?;
        let copy = worktree::create(&self.root, &self.workspace, &result, working)?;
        let mut after = self.state.clone();
        after.pending = None;
        if matches!(
            pending.kind,
            MergeKind::Merge | MergeKind::CherryPick | MergeKind::Revert
        ) {
            Self::set_head(&mut after, result.id.clone());
            after.index = result.id.clone();
        }
        if pending.kind == MergeKind::StashPop {
            after
                .stash
                .retain(|s| Some(&s.id) != pending.stash.as_ref());
        }
        Self::log(
            &mut after,
            pending.message,
            self.state.head_id().map(str::to_owned),
            Some(result.id),
        );
        self.save(after)?;
        Ok(Outcome::WorkingCopy(copy))
    }
}

fn merge_files(
    base: &Value,
    ours: &Value,
    theirs: &Value,
    choices: &BTreeMap<String, Side>,
    conflicts: &mut Vec<Conflict>,
) -> Result<Value, ChartError> {
    let (b, o, t) = (
        base.as_object().unwrap(),
        ours.as_object().unwrap(),
        theirs.as_object().unwrap(),
    );
    let mut result = serde_json::Map::new();
    for key in b
        .keys()
        .chain(o.keys())
        .chain(t.keys())
        .collect::<BTreeSet<_>>()
    {
        let (b, o, t) = (b.get(key), o.get(key), t.get(key));
        let value = if o == t || t == b {
            o
        } else if o == b {
            t
        } else {
            if conflicts.len() >= 512 {
                return Err(invalid(
                    "Merge has more than 512 conflicts; merge smaller changes",
                ));
            }
            let path = format!("/files/{}", key.replace('~', "~0").replace('/', "~1"));
            let choice = choices.get(&path).copied();
            conflicts.push(Conflict {
                path,
                base: b.cloned(),
                current: o.cloned(),
                incoming: t.cloned(),
                choice,
            });
            if choice == Some(Side::Incoming) { t } else { o }
        };
        if let Some(value) = value {
            result.insert(key.clone(), value.clone());
        }
    }
    Ok(Value::Object(result))
}
