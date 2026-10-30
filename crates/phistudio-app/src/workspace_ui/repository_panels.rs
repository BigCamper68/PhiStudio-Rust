use super::*;
use super::{
    repository::{Action, State},
    versions_panel::{confirm, difference, field, summary},
};
use phistudio_core::vcs::{self, Side};
impl Workbench {
    pub(super) fn repository_changes(
        ui: &mut egui::Ui,
        state: &mut State,
        status: &vcs::Status,
        action: &mut Option<Action>,
    ) {
        field(ui, "Commit message", "vcs-message", &mut state.message);
        field(ui, "Author", "vcs-author", &mut state.author);
        field(ui, "Email", "vcs-email", &mut state.email);
        ui.checkbox(&mut state.amend, crate::i18n::tr("Amend last commit"));
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Stage all")).clicked() {
                *action = Some(Action::Stage);
            }
            if ui.button(crate::i18n::tr("Unstage all")).clicked() {
                *action = Some(Action::Unstage);
            }
            if ui
                .add_enabled(
                    !state.message.trim().is_empty() && (state.amend || !status.staged.is_empty()),
                    egui::Button::new(crate::i18n::tr("Commit staged")),
                )
                .clicked()
            {
                *action = Some(Action::Commit {
                    message: state.message.clone(),
                    author: versions::Author {
                        name: state.author.clone(),
                        email: state.email.clone(),
                    },
                    amend: state.amend,
                });
            }
            if confirm(ui, state, "discard", "Discard unstaged…") {
                *action = Some(Action::Discard);
            }
        });
        ui.label(crate::i18n::tr(
            "Staging freezes the current chart and resources. Later edits remain unstaged.",
        ));
        ui.collapsing(
            crate::i18n::format("Staged · {0} changes", &[status.staged.total().to_string()]),
            |ui| difference(ui, &status.staged),
        );
        ui.collapsing(
            crate::i18n::format(
                "Unstaged · {0} changes",
                &[status.unstaged.total().to_string()],
            ),
            |ui| difference(ui, &status.unstaged),
        );
    }
    pub(super) fn repository_refs(
        ui: &mut egui::Ui,
        state: &mut State,
        status: &vcs::Status,
        action: &mut Option<Action>,
    ) {
        field(ui, "Branch / tag name", "vcs-ref", &mut state.name);
        field(ui, "Tag message", "vcs-tag-message", &mut state.tag_message);
        ui.checkbox(
            &mut state.at_selected,
            crate::i18n::tr("Create at selected commit"),
        );
        let at = state.at_selected.then(|| state.selected.clone()).flatten();
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Create branch")).clicked() {
                *action = Some(Action::CreateBranch(state.name.clone(), at.clone()));
            }
            if ui.button(crate::i18n::tr("Create tag")).clicked() {
                *action = Some(Action::Tag(
                    state.name.clone(),
                    at,
                    state.tag_message.clone(),
                ));
            }
        });
        if state.at_selected {
            ui.small(state.selected.as_deref().unwrap_or("HEAD"));
        }
        field(ui, "New branch name", "vcs-rename", &mut state.rename);
        for (name, id) in &status.state.branches {
            ui.push_id(("branch", name), |ui| {
                ui.separator();
                ui.strong(format!(
                    "{}{}",
                    name,
                    if status.state.branch() == Some(name) {
                        " · HEAD"
                    } else {
                        ""
                    }
                ));
                ui.horizontal_wrapped(|ui| {
                    if ui.button(crate::i18n::tr("Switch branch")).clicked() {
                        *action = Some(Action::CheckoutBranch(name.clone()));
                    }
                    if ui.button(crate::i18n::tr("Merge into current")).clicked() {
                        *action = Some(Action::Merge(format!("branch:{name}")));
                    }
                    if ui.button(crate::i18n::tr("Rename")).clicked() {
                        *action = Some(Action::RenameBranch(name.clone(), state.rename.clone()));
                    }
                    if confirm(ui, state, &format!("delete-branch:{name}"), "Delete") {
                        *action = Some(Action::DeleteBranch(name.clone()));
                    }
                    if ui.button(crate::i18n::tr("Show commit")).clicked() {
                        state.selected = Some(id.clone());
                        state.tab = 0;
                    }
                });
            });
        }
        for (name, tag) in &status.state.tags {
            ui.push_id(("tag", name), |ui| {
                ui.separator();
                ui.strong(format!("#{name}"));
                ui.label(&tag.message);
                ui.horizontal_wrapped(|ui| {
                    if ui.button(crate::i18n::tr("Show commit")).clicked() {
                        state.selected = Some(tag.commit.clone());
                        state.tab = 0;
                    }
                    if confirm(ui, state, &format!("delete-tag:{name}"), "Delete") {
                        *action = Some(Action::DeleteTag(name.clone()));
                    }
                });
            });
        }
    }
    pub(super) fn repository_stashes(
        ui: &mut egui::Ui,
        state: &mut State,
        status: &vcs::Status,
        action: &mut Option<Action>,
    ) {
        field(
            ui,
            "Stash message",
            "vcs-stash-message",
            &mut state.stash_message,
        );
        if ui
            .button(crate::i18n::tr("Stash current changes"))
            .clicked()
        {
            *action = Some(Action::StashPush(
                if state.stash_message.trim().is_empty() {
                    crate::i18n::tr("Working changes")
                } else {
                    state.stash_message.clone()
                },
            ));
        }
        ui.label(crate::i18n::tr(
            "Apply keeps the stash. Pop removes it only after successful application.",
        ));
        for stash in &status.state.stash {
            ui.push_id(&stash.id, |ui| {
                ui.separator();
                ui.label(&stash.message);
                ui.small(stash.branch.as_deref().unwrap_or("HEAD"));
                ui.horizontal_wrapped(|ui| {
                    if ui.button(crate::i18n::tr("Apply")).clicked() {
                        *action = Some(Action::StashApply(stash.id.clone(), false));
                    }
                    if ui.button(crate::i18n::tr("Pop stash")).clicked() {
                        *action = Some(Action::StashApply(stash.id.clone(), true));
                    }
                    if confirm(ui, state, &format!("drop:{}", stash.id), "Delete") {
                        *action = Some(Action::StashDrop(stash.id.clone()));
                    }
                });
            });
        }
    }
    pub(super) fn repository_reflog(
        ui: &mut egui::Ui,
        state: &mut State,
        status: &vcs::Status,
        action: &mut Option<Action>,
    ) {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        field(
            ui,
            "History archive path",
            "vcs-import-path",
            &mut state.import_path,
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Export history…")).clicked() {
                *action = Some(Action::Export);
            }
            if ui.button(crate::i18n::tr("Import history…")).clicked() {
                state.import_requested = true;
            }
            if confirm(ui, state, "collect", "Clean unused snapshots…") {
                *action = Some(Action::Collect);
            }
        });
        ui.label(crate::i18n::tr("History archives include project resources. Import adds renamed references and keeps current work."));
        for (i, entry) in status.state.reflog.iter().enumerate() {
            ui.push_id(i, |ui| {
                ui.separator();
                ui.label(crate::i18n::tr(&entry.message));
                ui.small(super::versions_panel::date(entry.created_ms));
                if let Some(id) = &entry.to {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(crate::i18n::tr("Show commit")).clicked() {
                            state.selected = Some(id.clone());
                            state.tab = 0;
                            state.unreferenced = true;
                            state.current_only = false;
                        }
                        if ui.button(crate::i18n::tr("Recover as branch")).clicked() {
                            *action = Some(Action::CreateBranch(
                                format!("recovered-{}-{i}", entry.created_ms),
                                Some(id.clone()),
                            ));
                        }
                    });
                }
            });
        }
    }
    pub(super) fn repository_conflicts(
        ui: &mut egui::Ui,
        state: &mut State,
        merge: &vcs::MergePreview,
        action: &mut Option<Action>,
    ) {
        ui.strong(crate::i18n::tr("Resolve merge conflicts"));
        ui.label(&merge.pending.message);
        ui.horizontal_wrapped(|ui| {
            for (label, side) in [
                ("Use current for all", Side::Current),
                ("Use incoming for all", Side::Incoming),
            ] {
                if ui.button(crate::i18n::tr(label)).clicked() {
                    for c in &merge.conflicts {
                        state.choices.insert(c.path.clone(), side);
                    }
                }
            }
            if ui
                .add_enabled(
                    merge
                        .conflicts
                        .iter()
                        .all(|c| state.choices.contains_key(&c.path)),
                    egui::Button::new(crate::i18n::tr("Apply merge")),
                )
                .clicked()
            {
                *action = Some(Action::Resolve(state.choices.clone()));
            }
            if ui.button(crate::i18n::tr("Abort merge")).clicked() {
                *action = Some(Action::Abort);
            }
        });
        for c in &merge.conflicts {
            ui.push_id(&c.path, |ui| {
                ui.collapsing(&c.path, |ui| {
                    ui.label(crate::i18n::tr("Base"));
                    ui.label(summary(&c.base));
                    ui.label(crate::i18n::tr("Current"));
                    ui.label(summary(&c.current));
                    ui.label(crate::i18n::tr("Incoming"));
                    ui.label(summary(&c.incoming));
                });
                ui.horizontal_wrapped(|ui| {
                    for (label, side) in [("Current", Side::Current), ("Incoming", Side::Incoming)]
                    {
                        if ui
                            .selectable_label(
                                state.choices.get(&c.path) == Some(&side),
                                crate::i18n::tr(label),
                            )
                            .clicked()
                        {
                            state.choices.insert(c.path.clone(), side);
                        }
                    }
                });
            });
        }
    }
}
