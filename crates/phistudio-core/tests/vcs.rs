use phistudio_core::{
    vcs::{self, Outcome, Repository, ResetMode, Working},
    *,
};
use serde_json::json;
use std::{fs, path::PathBuf};
struct Fixture {
    root: tempfile::TempDir,
    workspace: PathBuf,
    location: ProjectLocation,
    editor: Editor,
    options: ProjectOptions,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let document = ChartDocument::empty("Repository", 120.).unwrap();
        let location = create_project(root.path(), &document).unwrap();
        let workspace = location.chart.parent().unwrap().to_owned();
        fs::write(workspace.join("cover.bin"), b"shared resource").unwrap();
        fs::create_dir(workspace.join("empty-folder")).unwrap();
        Self {
            root,
            workspace,
            location,
            editor: Editor::new(document),
            options: ProjectOptions::default(),
        }
    }
    fn working(&self) -> Working<'_> {
        Working {
            document: self.editor.document(),
            location: &self.location,
            options: &self.options,
        }
    }
    fn repo(&self) -> Repository {
        Repository::open(self.root.path(), &self.workspace, &self.working()).unwrap()
    }
    fn note(&mut self, beat: i32, x: f64) {
        self.editor
            .execute(Command::AddNote {
                line: 0,
                note: Note {
                    start_time: Beat::new(beat, 0, 1).unwrap(),
                    end_time: Beat::new(beat, 0, 1).unwrap(),
                    x,
                    ..Default::default()
                },
            })
            .unwrap();
    }
    fn accept(&mut self, copy: vcs::WorkingCopy) {
        self.location = copy.location;
        self.options = copy.options;
        self.editor = Editor::recovered(copy.document);
    }
}
#[test]
fn staging_is_exact_and_committing_does_not_save_or_reset_editor_history() {
    let mut f = Fixture::new();
    let mut repo = f.repo();
    let saved = fs::read(&f.location.chart).unwrap();
    assert!(repo.status(&f.working()).unwrap().clean());
    f.note(1, 10.);
    repo.stage(&f.working()).unwrap();
    f.note(2, 20.);
    let before = f.editor.document().as_json().clone();
    let v = repo
        .commit(
            "First note",
            versions::Author {
                name: "Tester".into(),
                email: "test@example.org".into(),
            },
            false,
        )
        .unwrap();
    assert_eq!(v.notes, 1);
    assert_eq!(v.author.name, "Tester");
    let status = repo.status(&f.working()).unwrap();
    assert!(status.staged.is_empty());
    assert!(!status.unstaged.is_empty());
    assert_eq!(f.editor.document().as_json(), &before);
    assert!(f.editor.can_undo() && f.editor.is_dirty());
    assert_eq!(fs::read(&f.location.chart).unwrap(), saved);
    assert_eq!(
        v.blobs["cover.bin"],
        versions::load(f.root.path(), &f.workspace, &v.parents[0])
            .unwrap()
            .blobs["cover.bin"]
    );
}
#[test]
fn checkout_has_a_separate_recoverable_worktree_and_save_publishes_it() {
    let mut f = Fixture::new();
    let mut repo = f.repo();
    repo.create_branch("alternate", None).unwrap();
    f.note(1, 30.);
    repo.stage(&f.working()).unwrap();
    repo.commit("Main edit", Default::default(), false).unwrap();
    let copy = repo
        .checkout_branch("alternate", &f.working(), false)
        .unwrap();
    f.accept(copy);
    assert_eq!(f.editor.document().note_count(), 0);
    assert!(vcs::is_working_copy(&f.location.chart));
    assert!(repo.status(&f.working()).unwrap().clean());
    let copy = repo.checkout_branch("main", &f.working(), false).unwrap();
    f.accept(copy);
    assert_eq!(f.editor.document().note_count(), 1);
    let saved = open_project(f.root.path(), &f.workspace).unwrap();
    assert_eq!(
        load_chart(&saved.charts[saved.suggested.unwrap_or(0)].path)
            .unwrap()
            .note_count(),
        0
    );
    save_recovery(
        f.root.path(),
        &RecoverySnapshot {
            document: f.editor.document().clone(),
            location: Some(f.location.clone()),
            beat: 0.,
            line: 0,
        },
    )
    .unwrap();
    assert!(load_recovery(f.root.path()).unwrap().is_some());
    assert!(
        vcs::save_working_copy(
            f.root.path(),
            &f.location.chart,
            f.editor.document(),
            &f.options
        )
        .unwrap()
    );
    assert!(!vcs::is_working_copy(&f.location.chart));
    assert!(load_recovery(f.root.path()).unwrap().is_none());
    assert!(repo.status(&f.working()).unwrap().clean());
    let saved = open_project(f.root.path(), &f.workspace).unwrap();
    assert_eq!(
        load_chart(&saved.charts[saved.suggested.unwrap_or(0)].path)
            .unwrap()
            .note_count(),
        1
    );
    assert!(
        f.location
            .assets
            .as_ref()
            .unwrap()
            .join("empty-folder")
            .is_dir()
    );
}
#[test]
fn reset_modes_keep_index_and_working_changes_separate_and_reflog_protects_backups() {
    let mut f = Fixture::new();
    let mut repo = f.repo();
    let initial = repo.state.head_id().unwrap().to_owned();
    f.note(1, 10.);
    repo.stage(&f.working()).unwrap();
    repo.commit("One", Default::default(), false).unwrap();
    f.note(2, 20.);
    repo.stage(&f.working()).unwrap();
    let index = repo.state.index.clone();
    assert!(matches!(
        repo.reset(&initial, ResetMode::Soft, &f.working()).unwrap(),
        Outcome::Unchanged
    ));
    assert_eq!(repo.state.index, index);
    assert_eq!(f.editor.document().note_count(), 2);
    repo.reset(&initial, ResetMode::Mixed, &f.working())
        .unwrap();
    assert_eq!(repo.state.index, initial);
    assert_eq!(f.editor.document().note_count(), 2);
    let Outcome::WorkingCopy(copy) = repo.reset(&initial, ResetMode::Hard, &f.working()).unwrap()
    else {
        panic!()
    };
    f.accept(copy);
    assert_eq!(f.editor.document().note_count(), 0);
    assert!(repo.status(&f.working()).unwrap().clean());
    let backup = repo
        .state
        .reflog
        .iter()
        .find(|e| e.message == "Backup before hard reset")
        .unwrap()
        .to
        .clone()
        .unwrap();
    assert_eq!(
        versions::load(f.root.path(), &f.workspace, &backup)
            .unwrap()
            .notes,
        2
    );
    repo.collect_unused().unwrap();
    assert!(versions::load(f.root.path(), &f.workspace, &backup).is_ok());
}
#[test]
fn amend_detached_head_tags_and_stashes_preserve_previous_commits() {
    let mut f = Fixture::new();
    let mut repo = f.repo();
    f.note(1, 10.);
    repo.stage(&f.working()).unwrap();
    let first = repo.commit("First", Default::default(), false).unwrap();
    repo.create_tag("release", None, "Named tag").unwrap();
    f.note(2, 20.);
    let copy = repo.stash_push(&f.working(), "Temporary notes").unwrap();
    f.accept(copy);
    assert_eq!(f.editor.document().note_count(), 1);
    let stash = repo.state.stash[0].id.clone();
    let Outcome::WorkingCopy(copy) = repo.stash_apply(&stash, &f.working(), true).unwrap() else {
        panic!()
    };
    f.accept(copy);
    assert_eq!(f.editor.document().note_count(), 2);
    assert!(repo.state.stash.is_empty());
    repo.stage(&f.working()).unwrap();
    let amended = repo.commit("Amended", Default::default(), true).unwrap();
    assert_eq!(amended.parents, first.parents);
    assert_eq!(repo.state.tags["release"].commit, first.id);
    let copy = repo
        .checkout_commit(&first.id, &f.working(), false)
        .unwrap();
    f.accept(copy);
    assert!(repo.state.branch().is_none());
    repo.create_branch("rescued", None).unwrap();
    assert!(repo.delete_branch("main").is_ok());
    assert_eq!(repo.state.branches["rescued"], first.id);
}
#[test]
fn bundle_roundtrip_renames_refs_and_leaves_local_working_data_and_head_untouched() {
    let mut f = Fixture::new();
    let mut repo = f.repo();
    f.note(1, 50.);
    repo.stage(&f.working()).unwrap();
    repo.commit("For export", Default::default(), false)
        .unwrap();
    repo.create_tag("tag", None, "tag").unwrap();
    let bundle = vcs::export_bundle(&repo).unwrap();
    let before_head = repo.state.head.clone();
    let before_index = repo.state.index.clone();
    let before_doc = f.editor.document().as_json().clone();
    let imported = vcs::import_bundle(&mut repo, bundle.path()).unwrap();
    assert!(imported.commits >= 2);
    assert_ne!(imported.branches["main"], "main");
    assert_ne!(imported.tags["tag"], "tag");
    assert_eq!(repo.state.head, before_head);
    assert_eq!(repo.state.index, before_index);
    assert_eq!(f.editor.document().as_json(), &before_doc);
    let rows = vcs::graph(
        &repo.status(&f.working()).unwrap().commits,
        &repo.state,
        false,
        false,
    );
    assert!(rows.len() >= 4);
    for (i, row) in rows.iter().enumerate() {
        assert!(row.parents.iter().all(|(p, _)| *p > i));
    }
}
#[test]
fn metadata_merge_and_revert_keep_unknown_fields_and_saved_workspace_unchanged() {
    let mut f = Fixture::new();
    let mut raw = f.editor.document().as_json().clone();
    raw["future"] = json!({"left":0,"right":0});
    f.editor = Editor::new(ChartDocument::from_value(raw).unwrap());
    let mut repo = f.repo();
    repo.create_branch("other", None).unwrap();
    let edit = |f: &mut Fixture, key: &str, n: i32| {
        let mut raw = f.editor.document().as_json().clone();
        raw["future"][key] = json!(n);
        f.editor = Editor::recovered(ChartDocument::from_value(raw).unwrap());
    };
    edit(&mut f, "left", 1);
    repo.stage(&f.working()).unwrap();
    let left = repo.commit("Left", Default::default(), false).unwrap();
    let copy = repo.checkout_branch("other", &f.working(), false).unwrap();
    f.accept(copy);
    edit(&mut f, "right", 2);
    repo.stage(&f.working()).unwrap();
    repo.commit("Right", Default::default(), false).unwrap();
    let copy = repo.checkout_branch("main", &f.working(), false).unwrap();
    f.accept(copy);
    let Outcome::WorkingCopy(copy) = repo.merge("other", &f.working()).unwrap() else {
        panic!()
    };
    f.accept(copy);
    assert_eq!(
        f.editor.document().as_json()["future"],
        json!({"left":1,"right":2})
    );
    assert_eq!(
        versions::load(f.root.path(), &f.workspace, repo.state.head_id().unwrap())
            .unwrap()
            .parents
            .len(),
        2
    );
    let Outcome::WorkingCopy(copy) = repo.revert(&left.id, &f.working()).unwrap() else {
        panic!()
    };
    f.accept(copy);
    assert_eq!(
        f.editor.document().as_json()["future"],
        json!({"left":0,"right":2})
    );
    assert!(
        load_chart(&open_project(f.root.path(), &f.workspace).unwrap().charts[0].path)
            .unwrap()
            .as_json()
            .get("future")
            .is_none()
    );
}

#[test]
fn line_reordering_merges_other_line_edits_and_remaps_parents_and_owned_effects() {
    let mut f = Fixture::new();
    let mut raw = f.editor.document().as_json().clone();
    let mut lines = vec![];
    for name in ["A", "B", "C"] {
        let mut line = raw["judgeLineList"][0].clone();
        line["Name"] = json!(name);
        line["father"] = json!(-1);
        lines.push(line);
    }
    lines[2]["father"] = json!(0);
    raw["judgeLineList"] = json!(lines);
    raw["effects"] = json!([{"shader":"glitch","line":0,"future":true}]);
    raw["phiStudioExtra"] = json!({"effects":[{"shader":"glitch","line":15}]});
    f.editor = Editor::new(ChartDocument::from_value(raw).unwrap());
    let mut repo = f.repo();
    repo.create_branch("order", None).unwrap();
    f.note(4, 33.);
    repo.stage(&f.working()).unwrap();
    repo.commit("Edit A", Default::default(), false).unwrap();
    f.accept(repo.checkout_branch("order", &f.working(), false).unwrap());
    f.editor
        .execute(Command::MoveLine { from: 2, to: 0 })
        .unwrap();
    repo.stage(&f.working()).unwrap();
    repo.commit("Move C", Default::default(), false).unwrap();
    f.accept(repo.checkout_branch("main", &f.working(), false).unwrap());
    let Outcome::WorkingCopy(copy) = repo.merge("order", &f.working()).unwrap() else {
        panic!("Independent line edits conflicted")
    };
    f.accept(copy);
    let raw = f.editor.document().as_json();
    assert_eq!(raw["judgeLineList"][0]["Name"], "C");
    assert_eq!(raw["judgeLineList"][0]["father"], 1);
    assert_eq!(
        raw["judgeLineList"][1]["notes"].as_array().unwrap().len(),
        1
    );
    assert_eq!(raw["effects"][0]["line"], 1);
    assert_eq!(raw["phiStudioExtra"]["effects"][0]["line"], 15);
    assert!(!raw.to_string().contains("base-0"));
    assert!(repo.status(&f.working()).unwrap().clean());
}
#[test]
fn independent_note_insertions_in_the_same_gap_combine_without_helper_ids() {
    let mut f = Fixture::new();
    let mut repo = f.repo();
    repo.create_branch("other", None).unwrap();
    f.note(1, 10.);
    repo.stage(&f.working()).unwrap();
    repo.commit("Left note", Default::default(), false).unwrap();
    f.accept(repo.checkout_branch("other", &f.working(), false).unwrap());
    f.note(2, 20.);
    repo.stage(&f.working()).unwrap();
    repo.commit("Right note", Default::default(), false)
        .unwrap();
    f.accept(repo.checkout_branch("main", &f.working(), false).unwrap());
    let Outcome::WorkingCopy(copy) = repo.merge("other", &f.working()).unwrap() else {
        panic!("Independent insertions conflicted")
    };
    f.accept(copy);
    assert_eq!(f.editor.document().note_count(), 2);
    assert_eq!(
        f.editor.document().as_json()["judgeLineList"][0]["numOfNotes"],
        2
    );
    assert!(repo.status(&f.working()).unwrap().clean());
}
#[test]
fn pending_conflicts_survive_restart_reject_stale_work_and_resolve_explicitly() {
    let mut f = Fixture::new();
    let mut raw = f.editor.document().as_json().clone();
    raw["future"] = json!(0);
    f.editor = Editor::new(ChartDocument::from_value(raw).unwrap());
    let mut repo = f.repo();
    repo.create_branch("other", None).unwrap();
    let edit = |f: &mut Fixture, n: i32| {
        let mut raw = f.editor.document().as_json().clone();
        raw["future"] = json!(n);
        f.editor = Editor::recovered(ChartDocument::from_value(raw).unwrap());
    };
    edit(&mut f, 1);
    repo.stage(&f.working()).unwrap();
    repo.commit("One", Default::default(), false).unwrap();
    f.accept(repo.checkout_branch("other", &f.working(), false).unwrap());
    edit(&mut f, 2);
    repo.stage(&f.working()).unwrap();
    repo.commit("Two", Default::default(), false).unwrap();
    f.accept(repo.checkout_branch("main", &f.working(), false).unwrap());
    let before = f.editor.document().as_json().clone();
    assert!(matches!(
        repo.merge("other", &f.working()).unwrap(),
        Outcome::Conflicts(_)
    ));
    assert_eq!(f.editor.document().as_json(), &before);
    drop(repo);
    let mut repo = f.repo();
    let preview = repo.merge_preview().unwrap().unwrap();
    assert_eq!(preview.conflicts.len(), 1);
    assert!(repo.commit("Blocked", Default::default(), false).is_err());
    assert!(
        repo.resolve_merge(&f.working(), &Default::default())
            .is_err()
    );
    let choices = preview
        .conflicts
        .into_iter()
        .map(|c| (c.path, vcs::Side::Incoming))
        .collect();
    edit(&mut f, 3);
    assert!(repo.resolve_merge(&f.working(), &choices).is_err());
    edit(&mut f, 1);
    let Outcome::WorkingCopy(copy) = repo.resolve_merge(&f.working(), &choices).unwrap() else {
        panic!()
    };
    f.accept(copy);
    assert_eq!(f.editor.document().as_json()["future"], 2);
    assert!(repo.state.pending.is_none());
    assert!(repo.status(&f.working()).unwrap().clean());
}
#[test]
fn stash_pop_conflict_preserves_stash_until_resolved_and_abort_keeps_work() {
    let mut f = Fixture::new();
    let mut raw = f.editor.document().as_json().clone();
    raw["future"] = json!(0);
    f.editor = Editor::new(ChartDocument::from_value(raw).unwrap());
    let mut repo = f.repo();
    let edit = |f: &mut Fixture, n: i32| {
        let mut raw = f.editor.document().as_json().clone();
        raw["future"] = json!(n);
        f.editor = Editor::recovered(ChartDocument::from_value(raw).unwrap());
    };
    edit(&mut f, 1);
    f.accept(repo.stash_push(&f.working(), "One").unwrap());
    let id = repo.state.stash[0].id.clone();
    edit(&mut f, 2);
    let Outcome::Conflicts(preview) = repo.stash_apply(&id, &f.working(), true).unwrap() else {
        panic!()
    };
    assert_eq!(repo.state.stash.len(), 1);
    assert_eq!(f.editor.document().as_json()["future"], 2);
    repo.abort_merge().unwrap();
    assert_eq!(repo.state.stash.len(), 1);
    assert_eq!(f.editor.document().as_json()["future"], 2);
    assert!(matches!(
        repo.stash_apply(&id, &f.working(), true).unwrap(),
        Outcome::Conflicts(_)
    ));
    let choices = preview
        .conflicts
        .into_iter()
        .map(|c| (c.path, vcs::Side::Incoming))
        .collect();
    let Outcome::WorkingCopy(copy) = repo.resolve_merge(&f.working(), &choices).unwrap() else {
        panic!()
    };
    f.accept(copy);
    assert_eq!(f.editor.document().as_json()["future"], 1);
    assert!(repo.state.stash.is_empty());
    assert!(!repo.status(&f.working()).unwrap().unstaged.is_empty());
}
#[test]
fn fast_forward_cherry_pick_rename_refs_and_last_branch_deletion_are_consistent() {
    let mut f = Fixture::new();
    let mut repo = f.repo();
    repo.create_branch("topic", None).unwrap();
    f.note(1, 44.);
    repo.stage(&f.working()).unwrap();
    let commit = repo.commit("Note", Default::default(), false).unwrap();
    f.accept(repo.checkout_branch("topic", &f.working(), false).unwrap());
    let Outcome::WorkingCopy(copy) = repo.merge("main", &f.working()).unwrap() else {
        panic!()
    };
    f.accept(copy);
    assert_eq!(repo.state.head_id(), Some(commit.id.as_str()));
    assert_eq!(repo.status(&f.working()).unwrap().commits.len(), 2);
    repo.rename_branch("topic", "renamed").unwrap();
    assert_eq!(repo.state.branch(), Some("renamed"));
    assert!(repo.create_branch("../bad", None).is_err());
    repo.create_tag("v1", None, "Tag").unwrap();
    repo.delete_tag("v1").unwrap();
    let initial = commit.parents[0].clone();
    f.accept(repo.checkout_commit(&initial, &f.working(), false).unwrap());
    repo.delete_branch("main").unwrap();
    repo.delete_branch("renamed").unwrap();
    assert!(repo.state.branches.is_empty());
    let Outcome::WorkingCopy(copy) = repo.cherry_pick(&commit.id, &f.working()).unwrap() else {
        panic!()
    };
    f.accept(copy);
    assert_eq!(f.editor.document().note_count(), 1);
    assert!(repo.state.branch().is_none());
}
#[test]
fn saved_project_changes_block_publication_without_losing_working_copy() {
    let mut f = Fixture::new();
    let mut repo = f.repo();
    f.note(1, 10.);
    repo.stage(&f.working()).unwrap();
    let commit = repo.commit("Note", Default::default(), false).unwrap();
    f.accept(
        repo.checkout_commit(&commit.id, &f.working(), false)
            .unwrap(),
    );
    let path = f.workspace.join(".phistudio-workspace.json");
    let mut desc: serde_json::Value = fs::read(&path)
        .ok()
        .map(|b| serde_json::from_slice(&b).unwrap())
        .unwrap_or(json!({}));
    desc["future"] = json!(true);
    fs::write(&path, serde_json::to_vec(&desc).unwrap()).unwrap();
    assert!(
        vcs::save_working_copy(
            f.root.path(),
            &f.location.chart,
            f.editor.document(),
            &f.options
        )
        .is_err()
    );
    assert!(vcs::is_working_copy(&f.location.chart));
    assert_eq!(f.editor.document().note_count(), 1);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(path).unwrap()).unwrap(),
        desc
    );
}
#[test]
fn history_import_rejects_corrupt_graph_hashes_and_paths_without_moving_refs() {
    use std::io::{Read, Write};
    use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};
    let f = Fixture::new();
    let mut repo = f.repo();
    let bundle = vcs::export_bundle(&repo).unwrap();
    let before = repo.state.clone();
    for mutation in 0..4 {
        let mut source = ZipArchive::new(fs::File::open(bundle.path()).unwrap()).unwrap();
        let mut entries = vec![];
        for i in 0..source.len() {
            let mut e = source.by_index(i).unwrap();
            let name = e.name().to_owned();
            let mut bytes = vec![];
            e.read_to_end(&mut bytes).unwrap();
            entries.push((name, bytes));
        }
        let manifest = &mut entries
            .iter_mut()
            .find(|(n, _)| n == "repository.json")
            .unwrap()
            .1;
        let mut value: serde_json::Value = serde_json::from_slice(manifest).unwrap();
        match mutation {
            0 => {
                let id = value["commits"][0]["id"].clone();
                value["commits"][0]["parents"] = json!([id]);
            }
            1 => {
                value["commits"][0]["chart"] = json!("../escaped.json");
            }
            2 => {
                value["state"]["head"] = json!({"Detached":"version-unknown"});
            }
            _ => {}
        }
        *manifest = serde_json::to_vec(&value).unwrap();
        if mutation == 3 {
            entries
                .iter_mut()
                .find(|(n, _)| n.starts_with("objects/"))
                .unwrap()
                .1[0] ^= 1;
        }
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut zip = ZipWriter::new(fs::File::create(file.path()).unwrap());
        for (name, bytes) in entries {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(&bytes).unwrap();
        }
        zip.finish().unwrap();
        assert!(
            vcs::import_bundle(&mut repo, file.path()).is_err(),
            "Accepted corruption {mutation}"
        );
        assert_eq!(repo.state, before);
    }
}
#[test]
fn legacy_zip_versions_remain_readable_and_migrate_into_content_addressed_history() {
    use std::io::Write;
    use zip::{ZipWriter, write::SimpleFileOptions};
    let f = Fixture::new();
    let mut v = versions::save(
        f.root.path(),
        &f.workspace,
        "Legacy",
        f.editor.document(),
        &f.location,
        &f.options,
    )
    .unwrap();
    let dir = f
        .root
        .path()
        .join("versions")
        .join(f.workspace.file_name().unwrap())
        .join(&v.id);
    let mut zip = ZipWriter::new(fs::File::create(dir.join("workspace.zip")).unwrap());
    for name in v.files.keys() {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        let bytes = if name == "chart.json" {
            serde_json::to_vec(f.editor.document().as_json()).unwrap()
        } else {
            fs::read(f.workspace.join(name)).unwrap()
        };
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
    v.version = 1;
    v.blobs.clear();
    v.folders.clear();
    fs::write(dir.join("version.json"), serde_json::to_vec(&v).unwrap()).unwrap();
    assert_eq!(
        versions::load(f.root.path(), &f.workspace, &v.id)
            .unwrap()
            .version,
        1
    );
    let mut repo = f.repo();
    repo.stage(&f.working()).unwrap();
    repo.commit("Preserve empty folders", Default::default(), false)
        .unwrap();
    let bundle = vcs::export_bundle(&repo).unwrap();
    let summary = vcs::import_bundle(&mut repo, bundle.path()).unwrap();
    assert!(summary.commits >= 1);
    let copy = repo.checkout_commit(&v.id, &f.working(), false).unwrap();
    assert_eq!(copy.document.as_json(), f.editor.document().as_json());
}

#[test]
fn asset_conflicts_choose_whole_files_with_different_sizes_and_independent_folders_merge() {
    for side in [vcs::Side::Current, vcs::Side::Incoming] {
        let mut f = Fixture::new();
        let mut repo = f.repo();
        repo.create_branch("other", None).unwrap();
        fs::write(
            f.location.assets.as_ref().unwrap().join("cover.bin"),
            b"ours",
        )
        .unwrap();
        fs::create_dir(f.location.assets.as_ref().unwrap().join("ours-empty")).unwrap();
        repo.stage(&f.working()).unwrap();
        repo.commit("Our resource", Default::default(), false)
            .unwrap();
        f.accept(repo.checkout_branch("other", &f.working(), false).unwrap());
        fs::write(
            f.location.assets.as_ref().unwrap().join("cover.bin"),
            b"a longer incoming resource",
        )
        .unwrap();
        fs::create_dir(f.location.assets.as_ref().unwrap().join("incoming-empty")).unwrap();
        repo.stage(&f.working()).unwrap();
        repo.commit("Incoming resource", Default::default(), false)
            .unwrap();
        f.accept(repo.checkout_branch("main", &f.working(), false).unwrap());
        let Outcome::Conflicts(preview) = repo.merge("other", &f.working()).unwrap() else {
            panic!()
        };
        assert_eq!(preview.conflicts.len(), 1);
        assert_eq!(preview.conflicts[0].path, "/files/cover.bin");
        let choices = std::collections::BTreeMap::from([("/files/cover.bin".into(), side)]);
        let Outcome::WorkingCopy(copy) = repo.resolve_merge(&f.working(), &choices).unwrap() else {
            panic!()
        };
        f.accept(copy);
        let assets = f.location.assets.as_ref().unwrap();
        assert_eq!(
            fs::read(assets.join("cover.bin")).unwrap(),
            if side == vcs::Side::Current {
                b"ours".to_vec()
            } else {
                b"a longer incoming resource".to_vec()
            }
        );
        assert!(assets.join("ours-empty").is_dir() && assets.join("incoming-empty").is_dir());
        assert!(repo.status(&f.working()).unwrap().clean());
    }
}
