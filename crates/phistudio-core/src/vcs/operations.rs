use super::*;
use crate::versions::objects;
impl Repository {
    pub fn stage(&mut self, working: &Working<'_>) -> Result<(), ChartError> {
        self.idle()?;
        let v = tree::capture(&self.root, &self.workspace, working, "stage")?;
        let mut after = self.state.clone();
        after.index = v.id;
        self.save(after)
    }
    pub fn unstage(&mut self) -> Result<(), ChartError> {
        self.idle()?;
        let mut after = self.state.clone();
        after.index = after.head_id().unwrap().into();
        self.save(after)
    }
    pub fn set_author(&mut self, author: Author) -> Result<(), ChartError> {
        if author.name.trim().is_empty()
            || author.name.len() > 512
            || author.email.len() > 512
            || author
                .name
                .chars()
                .chain(author.email.chars())
                .any(char::is_control)
        {
            return Err(invalid(
                "Use an author name and email of at most 512 bytes without control characters",
            ));
        }
        let mut after = self.state.clone();
        after.author = Author {
            name: author.name.trim().into(),
            email: author.email.trim().into(),
        };
        self.save(after)
    }
    pub fn commit(
        &mut self,
        message: &str,
        author: Author,
        amend: bool,
    ) -> Result<Version, ChartError> {
        self.idle()?;
        if author.name.trim().is_empty()
            || author.name.len() > 512
            || author.email.len() > 512
            || author
                .name
                .chars()
                .chain(author.email.chars())
                .any(char::is_control)
        {
            return Err(invalid(
                "Use an author name and email of at most 512 bytes without control characters",
            ));
        }
        let head = versions::load(&self.root, &self.workspace, self.state.head_id().unwrap())?;
        let index = versions::load(&self.root, &self.workspace, &self.state.index)?;
        if !amend
            && tree::same(
                &tree::read(&self.root, &self.workspace, &head)?,
                &tree::read(&self.root, &self.workspace, &index)?,
            )?
        {
            return Err(invalid("Stage changed project contents before committing"));
        }
        let parents = if amend {
            head.parents.clone()
        } else {
            vec![head.id.clone()]
        };
        let mut commit = self.promote(&index, message, parents, "commit")?;
        commit.author = Author {
            name: author.name.trim().into(),
            email: author.email.trim().into(),
        };
        objects::write_version(&self.root, &self.workspace, &commit)?;
        let mut after = self.state.clone();
        after.author = commit.author.clone();
        Self::set_head(&mut after, commit.id.clone());
        after.index = commit.id.clone();
        Self::log(
            &mut after,
            if amend { "Amend commit" } else { "Commit" },
            Some(head.id),
            Some(commit.id.clone()),
        );
        self.save(after)?;
        Ok(commit)
    }
    pub fn create_branch(&mut self, name: &str, at: Option<&str>) -> Result<(), ChartError> {
        self.idle()?;
        let name = ref_name(name)?;
        if self.state.branches.contains_key(&name) || self.state.branches.len() >= 64 {
            return Err(invalid(
                "Branch already exists or the 64-branch limit was reached",
            ));
        }
        let id = at
            .map(|s| self.resolve(s))
            .transpose()?
            .unwrap_or_else(|| self.state.head_id().unwrap().into());
        let mut after = self.state.clone();
        after.branches.insert(name.clone(), id.clone());
        Self::log(
            &mut after,
            format!("Create branch: {name}"),
            self.state.head_id().map(str::to_owned),
            Some(id),
        );
        self.save(after)
    }
    pub fn rename_branch(&mut self, name: &str, next: &str) -> Result<(), ChartError> {
        self.idle()?;
        let next = ref_name(next)?;
        if self.state.branches.contains_key(&next) {
            return Err(invalid("Branch already exists"));
        }
        let mut after = self.state.clone();
        let id = after
            .branches
            .remove(name)
            .ok_or_else(|| invalid("Branch no longer exists"))?;
        after.branches.insert(next.clone(), id.clone());
        if after.head == Head::Branch(name.into()) {
            after.head = Head::Branch(next.clone());
        }
        Self::log(
            &mut after,
            format!("Rename branch: {name} → {next}"),
            Some(id.clone()),
            Some(id),
        );
        self.save(after)
    }
    pub fn delete_branch(&mut self, name: &str) -> Result<(), ChartError> {
        self.idle()?;
        if self.state.branch() == Some(name) {
            return Err(invalid("Cannot delete the active branch"));
        }
        let mut after = self.state.clone();
        let old = after
            .branches
            .remove(name)
            .ok_or_else(|| invalid("Branch no longer exists"))?;
        Self::log(
            &mut after,
            format!("Delete branch: {name}"),
            Some(old),
            self.state.head_id().map(str::to_owned),
        );
        self.save(after)
    }
    pub fn create_tag(
        &mut self,
        name: &str,
        at: Option<&str>,
        message: &str,
    ) -> Result<(), ChartError> {
        self.idle()?;
        let name = ref_name(name)?;
        if self.state.tags.contains_key(&name) || self.state.tags.len() >= 512 {
            return Err(invalid(
                "Tag already exists or the 512-tag limit was reached",
            ));
        }
        if message.len() > 512 {
            return Err(invalid("Use a tag message of at most 512 bytes"));
        }
        let id = at
            .map(|s| self.resolve(s))
            .transpose()?
            .unwrap_or_else(|| self.state.head_id().unwrap().into());
        let mut after = self.state.clone();
        after.tags.insert(
            name.clone(),
            Tag {
                commit: id.clone(),
                message: message.into(),
                author: after.author.clone(),
                created_ms: now(),
            },
        );
        Self::log(
            &mut after,
            format!("Create tag: {name}"),
            Some(id.clone()),
            Some(id),
        );
        self.save(after)
    }
    pub fn delete_tag(&mut self, name: &str) -> Result<(), ChartError> {
        self.idle()?;
        let mut after = self.state.clone();
        let tag = after
            .tags
            .remove(name)
            .ok_or_else(|| invalid("Tag no longer exists"))?;
        Self::log(
            &mut after,
            format!("Delete tag: {name}"),
            Some(tag.commit),
            self.state.head_id().map(str::to_owned),
        );
        self.save(after)
    }
    pub(super) fn clean(&self, working: &Working<'_>) -> Result<bool, ChartError> {
        let head = versions::load(&self.root, &self.workspace, self.state.head_id().unwrap())?;
        let index = versions::load(&self.root, &self.workspace, &self.state.index)?;
        let a = tree::read(&self.root, &self.workspace, &head)?;
        let b = if head.id == index.id {
            a.clone()
        } else {
            tree::read(&self.root, &self.workspace, &index)?
        };
        let c = tree::working(&self.workspace, working)?;
        Ok(tree::same(&a, &b)? && tree::same(&b, &c)?)
    }
    pub fn checkout_branch(
        &mut self,
        name: &str,
        working: &Working<'_>,
        force: bool,
    ) -> Result<WorkingCopy, ChartError> {
        let id = self
            .state
            .branches
            .get(name)
            .cloned()
            .ok_or_else(|| invalid("Branch no longer exists"))?;
        if self.state.head == Head::Branch(name.into()) {
            return Err(invalid("This branch is already active"));
        }
        self.checkout(Head::Branch(name.into()), &id, working, force)
    }
    pub fn checkout_commit(
        &mut self,
        id: &str,
        working: &Working<'_>,
        force: bool,
    ) -> Result<WorkingCopy, ChartError> {
        let id = self.resolve(id)?;
        self.checkout(Head::Detached(id.clone()), &id, working, force)
    }
    fn checkout(
        &mut self,
        target: Head,
        id: &str,
        working: &Working<'_>,
        force: bool,
    ) -> Result<WorkingCopy, ChartError> {
        self.idle()?;
        let clean = self.clean(working)?;
        if !clean && !force {
            return Err(invalid(
                "Commit or stash the staged and unstaged changes before checkout",
            ));
        }
        let v = versions::load(&self.root, &self.workspace, id)?;
        let copy = worktree::create(&self.root, &self.workspace, &v, working)?;
        let mut after = self.state.clone();
        if !clean {
            let backup = self.backup(working, "Before checkout")?;
            Self::log(
                &mut after,
                "Backup before checkout",
                self.state.head_id().map(str::to_owned),
                Some(backup.id),
            );
        }
        after.head = target;
        after.index = id.into();
        Self::log(
            &mut after,
            "Checkout",
            self.state.head_id().map(str::to_owned),
            Some(id.into()),
        );
        self.save(after)?;
        Ok(copy)
    }
    pub fn reset(
        &mut self,
        id: &str,
        mode: ResetMode,
        working: &Working<'_>,
    ) -> Result<Outcome, ChartError> {
        self.idle()?;
        let id = self.resolve(id)?;
        let v = versions::load(&self.root, &self.workspace, &id)?;
        let copy = if mode == ResetMode::Hard {
            Some(worktree::create(&self.root, &self.workspace, &v, working)?)
        } else {
            None
        };
        let mut after = self.state.clone();
        if mode == ResetMode::Hard {
            let backup = self.backup(working, "Before hard reset")?;
            Self::log(
                &mut after,
                "Backup before hard reset",
                self.state.head_id().map(str::to_owned),
                Some(backup.id),
            );
        }
        Self::set_head(&mut after, id.clone());
        if mode != ResetMode::Soft {
            after.index = id.clone();
        }
        Self::log(
            &mut after,
            match mode {
                ResetMode::Soft => "Soft reset",
                ResetMode::Mixed => "Mixed reset",
                ResetMode::Hard => "Hard reset",
            },
            self.state.head_id().map(str::to_owned),
            Some(id),
        );
        self.save(after)?;
        Ok(copy.map_or(Outcome::Unchanged, Outcome::WorkingCopy))
    }
    pub fn discard_unstaged(&mut self, working: &Working<'_>) -> Result<WorkingCopy, ChartError> {
        self.idle()?;
        let index = versions::load(&self.root, &self.workspace, &self.state.index)?;
        let copy = worktree::create(&self.root, &self.workspace, &index, working)?;
        let backup = self.backup(working, "Before discarding unstaged changes")?;
        let mut after = self.state.clone();
        Self::log(
            &mut after,
            "Backup before discarding unstaged changes",
            self.state.head_id().map(str::to_owned),
            Some(backup.id),
        );
        self.save(after)?;
        Ok(copy)
    }
    pub fn stash_push(
        &mut self,
        working: &Working<'_>,
        message: &str,
    ) -> Result<WorkingCopy, ChartError> {
        self.idle()?;
        if self.state.stash.len() >= 64 || message.len() > 512 {
            return Err(invalid(
                "Stash limit reached or stash message exceeds 512 bytes",
            ));
        }
        if self.clean(working)? {
            return Err(invalid("There are no working changes to stash"));
        }
        let head = self.state.head_id().unwrap().to_owned();
        let version = versions::load(&self.root, &self.workspace, &head)?;
        let snapshot = tree::capture(&self.root, &self.workspace, working, "stash")?;
        let copy = worktree::create(&self.root, &self.workspace, &version, working)?;
        let mut after = self.state.clone();
        after.stash.insert(
            0,
            Stash {
                id: snapshot.id.clone(),
                snapshot: snapshot.id,
                base: head.clone(),
                message: if message.trim().is_empty() {
                    "Working changes".into()
                } else {
                    message.trim().into()
                },
                branch: after.branch().map(str::to_owned),
                created_ms: now(),
            },
        );
        after.index = head.clone();
        Self::log(&mut after, "Stash changes", Some(head.clone()), Some(head));
        self.save(after)?;
        Ok(copy)
    }
    pub fn stash_drop(&mut self, id: &str) -> Result<(), ChartError> {
        self.idle()?;
        let mut after = self.state.clone();
        let index = after
            .stash
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| invalid("Stash no longer exists"))?;
        after.stash.remove(index);
        Self::log(
            &mut after,
            "Drop stash",
            self.state.head_id().map(str::to_owned),
            self.state.head_id().map(str::to_owned),
        );
        self.save(after)
    }
    pub fn abort_merge(&mut self) -> Result<(), ChartError> {
        if self.state.pending.is_none() {
            return Err(invalid("There is no pending merge"));
        }
        let mut after = self.state.clone();
        after.pending = None;
        Self::log(
            &mut after,
            "Abort merge",
            self.state.head_id().map(str::to_owned),
            self.state.head_id().map(str::to_owned),
        );
        self.save(after)
    }
}
