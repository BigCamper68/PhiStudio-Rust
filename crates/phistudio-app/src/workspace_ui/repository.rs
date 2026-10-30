use super::*;
use phistudio_core::vcs::{self, Outcome, Repository, ResetMode, Side, Working};
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct State {
    pub status: Option<Box<vcs::Status>>,
    pub tab: usize,
    pub message: String,
    pub author: String,
    pub email: String,
    pub amend: bool,
    pub name: String,
    pub rename: String,
    pub tag_message: String,
    pub stash_message: String,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub import_path: String,
    pub selected: Option<String>,
    pub detail: Option<(String, vcs::Difference)>,
    pub choices: BTreeMap<String, Side>,
    pub confirm: Option<String>,
    pub current_only: bool,
    pub unreferenced: bool,
    pub notice: String,
    pub page: usize,
    pub at_selected: bool,
    pub import_requested: bool,
    pub graph: Option<(bool, bool, Vec<vcs::GraphRow>)>,
}
pub(crate) enum Action {
    Refresh,
    Stage,
    Unstage,
    Commit {
        message: String,
        author: versions::Author,
        amend: bool,
    },
    Compare(String),
    CreateBranch(String, Option<String>),
    RenameBranch(String, String),
    DeleteBranch(String),
    CheckoutBranch(String),
    CheckoutCommit(String),
    Tag(String, Option<String>, String),
    DeleteTag(String),
    Merge(String),
    CherryPick(String),
    Revert(String),
    Reset(String, ResetMode),
    Discard,
    StashPush(String),
    StashApply(String, bool),
    StashDrop(String),
    Resolve(BTreeMap<String, Side>),
    Abort,
    Export,
    Import(PathBuf, bool),
    Collect,
}
impl WorkspaceContext {
    pub(super) fn repository_job(
        &self,
        workspace: PathBuf,
        revision: Option<u64>,
        action: Action,
    ) -> Result<WorkspaceResult, ChartError> {
        let (doc, location, options, _) = self.snapshot(&workspace)?;
        let working = Working {
            document: &doc,
            location: &location,
            options: &options,
        };
        let mut repo = Repository::open(&self.root, &workspace, &working)?;
        if revision.is_some_and(|r| r != repo.state.revision)
            && !matches!(action, Action::Refresh | Action::Compare(_))
        {
            return Err(invalid("Repository changed; refresh before retrying"));
        }
        let mut outcome = Outcome::Unchanged;
        let mut detail = None;
        let mut notice = "Project versions updated".to_owned();
        match action {
            Action::Refresh => {}
            Action::Stage => repo.stage(&working)?,
            Action::Unstage => repo.unstage()?,
            Action::Commit {
                message,
                author,
                amend,
            } => {
                repo.commit(&message, author, amend)?;
                notice = "Commit created; Save writes the project separately".into();
            }
            Action::Compare(id) => detail = Some((id.clone(), repo.compare_commit(&id)?)),
            Action::CreateBranch(name, at) => repo.create_branch(&name, at.as_deref())?,
            Action::RenameBranch(name, next) => repo.rename_branch(&name, &next)?,
            Action::DeleteBranch(name) => repo.delete_branch(&name)?,
            Action::CheckoutBranch(name) => {
                outcome = Outcome::WorkingCopy(repo.checkout_branch(&name, &working, false)?)
            }
            Action::CheckoutCommit(id) => {
                outcome = Outcome::WorkingCopy(repo.checkout_commit(&id, &working, false)?)
            }
            Action::Tag(name, at, message) => repo.create_tag(&name, at.as_deref(), &message)?,
            Action::DeleteTag(name) => repo.delete_tag(&name)?,
            Action::Merge(name) => outcome = repo.merge(&name, &working)?,
            Action::CherryPick(id) => outcome = repo.cherry_pick(&id, &working)?,
            Action::Revert(id) => outcome = repo.revert(&id, &working)?,
            Action::Reset(id, mode) => outcome = repo.reset(&id, mode, &working)?,
            Action::Discard => outcome = Outcome::WorkingCopy(repo.discard_unstaged(&working)?),
            Action::StashPush(message) => {
                outcome = Outcome::WorkingCopy(repo.stash_push(&working, &message)?)
            }
            Action::StashApply(id, pop) => outcome = repo.stash_apply(&id, &working, pop)?,
            Action::StashDrop(id) => repo.stash_drop(&id)?,
            Action::Resolve(choices) => outcome = repo.resolve_merge(&working, &choices)?,
            Action::Abort => repo.abort_merge()?,
            Action::Export => {
                return Ok(WorkspaceResult::RepositoryExport(vcs::export_bundle(
                    &repo,
                )?));
            }
            Action::Import(path, remove) => {
                let result = vcs::import_bundle(&mut repo, &path);
                if remove {
                    let _ = std::fs::remove_file(path);
                }
                let result = result?;
                notice = crate::i18n::format(
                    "Imported {0} commits; current work was kept",
                    &[result.commits.to_string()],
                );
            }
            Action::Collect => {
                let result = repo.collect_unused()?;
                notice = crate::i18n::format(
                    "Removed {0} unused snapshots and {1} objects",
                    &[result.snapshots.to_string(), result.objects.to_string()],
                );
            }
        }
        if let Outcome::WorkingCopy(copy) = outcome {
            let working = Working {
                document: &copy.document,
                location: &copy.location,
                options: &copy.options,
            };
            let status = Box::new(repo.status(&working)?);
            return Ok(WorkspaceResult::RepositoryWorking {
                workspace,
                copy: Box::new(copy),
                status,
            });
        }
        Ok(WorkspaceResult::Repository {
            workspace,
            status: Box::new(repo.status(&working)?),
            detail,
            notice,
        })
    }
}
impl Workbench {
    pub(crate) fn repository_request(&mut self, workspace: PathBuf, action: Action) {
        let revision = self
            .workspace
            .repository
            .status
            .as_ref()
            .map(|s| s.state.revision);
        self.workspace_request(WorkspaceJob::Repository {
            workspace,
            revision,
            action,
        });
    }
    pub(super) fn accept_repository(&mut self, status: Box<vcs::Status>) {
        let state = &mut self.workspace.repository;
        if state.author.is_empty() {
            state.author = status.state.author.name.clone();
            state.email = status.state.author.email.clone();
        }
        if state
            .status
            .as_ref()
            .and_then(|s| s.state.pending.as_ref())
            .map(|p| &p.ours)
            != status.state.pending.as_ref().map(|p| &p.ours)
        {
            state.choices.clear();
        }
        state.graph = None;
        state.status = Some(status);
    }
}
