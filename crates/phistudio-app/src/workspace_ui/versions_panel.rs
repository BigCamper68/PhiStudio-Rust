use super::repository::{Action, State};
use super::*;
use phistudio_core::vcs::{self, ResetMode};

impl Workbench {
    pub(super) fn versions_ui(&mut self, ui: &mut egui::Ui, workspace: PathBuf) {
        let mut state = std::mem::take(&mut self.workspace.repository);
        let mut action = None;
        ui.horizontal_wrapped(|ui| {
            for (i, label) in ["Graph", "Changes", "Branches / tags", "Stash", "Reflog"]
                .iter()
                .enumerate()
            {
                ui.selectable_value(&mut state.tab, i, crate::i18n::tr(label));
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(match state.status.as_ref().and_then(|s| s.state.branch()) {
                Some(name) => format!("HEAD · {name}"),
                None if state.status.is_some() => crate::i18n::tr("Detached HEAD"),
                None => "HEAD".into(),
            });
            if ui
                .small_button("↻")
                .on_hover_text(crate::i18n::tr("Refresh"))
                .clicked()
            {
                action = Some(Action::Refresh);
            }
        });
        if let Some(status) = state.status.take() {
            let height = ui.available_height().max(40.);
            egui::ScrollArea::vertical().id_source(("repository-tab",state.tab)).max_height(height).touch_show_y(ui,|ui| {
                ui.set_min_width(ui.available_width());
                if let Some(merge)=&status.merge {Self::repository_conflicts(ui,&mut state,merge,&mut action);}
                ui.add_enabled_ui(status.merge.is_none(),|ui|match state.tab {
                    0=>Self::repository_graph(ui,&mut state,&status,&mut action),
                    1=>Self::repository_changes(ui,&mut state,&status,&mut action),
                    2=>Self::repository_refs(ui,&mut state,&status,&mut action),
                    3=>Self::repository_stashes(ui,&mut state,&status,&mut action),
                    _=>Self::repository_reflog(ui,&mut state,&status,&mut action),
                });
                ui.separator();
                ui.small(crate::i18n::tr("Version operations load unsaved work. Save keeps it in the project; commits keep history separately."));
                if !state.notice.is_empty(){ui.label(crate::i18n::tr(&state.notice));}
                if let Some(error)=&self.error {ui.colored_label(crate::theme::flick(),crate::i18n::tr(error));}
            });
            state.status = Some(status);
        } else {
            ui.spinner();
            if let Some(error) = &self.error {
                ui.colored_label(crate::theme::flick(), crate::i18n::tr(error));
            }
        }
        self.workspace.repository = state;
        if let Some(action) = action {
            self.repository_request(workspace.clone(), action);
        }
        if std::mem::take(&mut self.workspace.repository.import_requested) {
            self.native_request = Some(FileRequest::ImportRepository(workspace));
        }
    }
    fn repository_graph(
        ui: &mut egui::Ui,
        state: &mut State,
        status: &vcs::Status,
        action: &mut Option<Action>,
    ) {
        let mut rows = vec![];
        let page_size = 30;
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(
                &mut state.current_only,
                crate::i18n::tr("Current branch only"),
            );
            ui.checkbox(
                &mut state.unreferenced,
                crate::i18n::tr("Include unreachable commits"),
            );
            rows = match state.graph.take() {
                Some((a, b, rows)) if a == state.current_only && b == state.unreferenced => rows,
                _ => vcs::graph(
                    &status.commits,
                    &status.state,
                    state.current_only,
                    state.unreferenced,
                ),
            };
            let pages = rows.len().div_ceil(page_size).max(1);
            state.page = state.page.min(pages - 1);
            if ui
                .add_enabled(state.page > 0, egui::Button::new("◀"))
                .clicked()
            {
                state.page -= 1;
            }
            ui.label(format!("{} / {}", state.page + 1, pages));
            if ui
                .add_enabled(state.page + 1 < pages, egui::Button::new("▶"))
                .clicked()
            {
                state.page += 1;
            }
        });
        let start = state.page * page_size;
        let end = (start + page_size).min(rows.len());
        let top = ui.cursor().top();
        let height = 32.;
        let width = ui.available_width();
        let lanes = rows.iter().map(|r| r.lane + 1).max().unwrap_or(1);
        let lane_width = (width * 0.25 / (lanes as f32)).clamp(3., 14.);
        let graph_width = ((lanes + 1) as f32 * lane_width).min(width * 0.3);
        let colors = [
            crate::theme::tap(),
            crate::theme::flick(),
            crate::theme::drag(),
            ui.visuals().hyperlink_color,
        ];
        let bounds = egui::Rect::from_min_size(
            egui::pos2(ui.cursor().left(), top),
            egui::vec2(width, (end - start) as f32 * height),
        );
        let painter = ui
            .painter()
            .with_clip_rect(bounds.intersect(ui.clip_rect()));
        for row in rows.iter().take(end).skip(start) {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
            if state.selected.as_ref() == Some(&row.id) {
                painter.rect_filled(rect, 3., ui.visuals().selection.bg_fill);
            }
            let x = rect.left() + 8. + row.lane as f32 * lane_width;
            let y = rect.center().y;
            let color = colors[row.lane % colors.len()];
            for (parent, lane) in &row.parents {
                let py = top + ((*parent - start) as f32 + 0.5) * height;
                painter.line_segment(
                    [
                        egui::pos2(x, y),
                        egui::pos2(rect.left() + 8. + *lane as f32 * lane_width, py),
                    ],
                    egui::Stroke::new(1.5_f32, color),
                );
            }
            painter.circle_filled(egui::pos2(x, y), 3.5, color);
            if let Some(commit) = status.commits.iter().find(|v| v.id == row.id) {
                let name = commit_name(commit);
                let label = format!("{} {}", row.refs.join(" · "), name);
                let max = ((width - graph_width - 8.) / 8.).max(4.) as usize;
                let clipped = if label.chars().count() > max {
                    format!(
                        "{}…",
                        label
                            .chars()
                            .take(max.saturating_sub(1))
                            .collect::<String>()
                    )
                } else {
                    label
                };
                painter.text(
                    egui::pos2(rect.left() + graph_width, y),
                    egui::Align2::LEFT_CENTER,
                    clipped,
                    egui::FontId::proportional(14.),
                    ui.visuals().text_color(),
                );
                if response
                    .on_hover_text(format!(
                        "{}\n{}\n{}",
                        commit.name, commit.author.name, commit.id
                    ))
                    .clicked()
                {
                    state.selected = Some(row.id.clone());
                    state.detail = None;
                    state.confirm = None;
                }
            }
        }
        state.graph = Some((state.current_only, state.unreferenced, rows));
        if let Some(id) = state.selected.clone() {
            Self::repository_commit(ui, state, status, &id, action);
        }
    }
    pub(super) fn repository_commit(
        ui: &mut egui::Ui,
        state: &mut State,
        status: &vcs::Status,
        id: &str,
        action: &mut Option<Action>,
    ) {
        ui.separator();
        ui.label(id);
        if let Some(commit) = status.commits.iter().find(|v| v.id == id) {
            ui.label(commit_name(commit));
            ui.label(format!("{} <{}>", commit.author.name, commit.author.email));
            ui.small(date(commit.created_ms));
            ui.label(crate::i18n::format(
                "{0} notes · {1} events · {2} files",
                &[
                    commit.notes.to_string(),
                    commit.events.to_string(),
                    commit.files.to_string(),
                ],
            ));
            for parent in &commit.parents {
                if ui.link(parent).clicked() {
                    state.selected = Some(parent.clone());
                    state.detail = None;
                }
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button(crate::i18n::tr("Show changes")).clicked() {
                *action = Some(Action::Compare(id.into()));
            }
            if ui.button(crate::i18n::tr("Checkout commit")).clicked() {
                *action = Some(Action::CheckoutCommit(id.into()));
            }
            if ui.button(crate::i18n::tr("Cherry-pick")).clicked() {
                *action = Some(Action::CherryPick(id.into()));
            }
            if ui.button(crate::i18n::tr("Revert commit")).clicked() {
                *action = Some(Action::Revert(id.into()));
            }
        });
        ui.label(crate::i18n::tr("Reset active branch to this commit"));
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(crate::i18n::tr("Soft · keep index and work"))
                .clicked()
            {
                *action = Some(Action::Reset(id.into(), ResetMode::Soft));
            }
            if ui.button(crate::i18n::tr("Mixed · keep work")).clicked() {
                *action = Some(Action::Reset(id.into(), ResetMode::Mixed));
            }
            if confirm(ui, state, &format!("hard:{id}"), "Hard · replace work") {
                *action = Some(Action::Reset(id.into(), ResetMode::Hard));
            }
        });
        if let Some((selected, diff)) = &state.detail
            && selected == id
        {
            difference(ui, diff);
        }
    }
}
pub(super) fn field(ui: &mut egui::Ui, label: &str, id: &str, value: &mut String) {
    ui.label(crate::i18n::tr(label));
    ui.add(
        egui::TextEdit::singleline(value)
            .id_source(id)
            .desired_width(ui.available_width().min(380.)),
    );
}
pub(super) fn confirm(ui: &mut egui::Ui, state: &mut State, id: &str, label: &str) -> bool {
    if state.confirm.as_deref() == Some(id) {
        let yes = ui
            .button(egui::RichText::new(crate::i18n::tr("Confirm")).color(crate::theme::flick()))
            .clicked();
        if yes || ui.button(crate::i18n::tr("Cancel")).clicked() {
            state.confirm = None;
        }
        yes
    } else {
        if ui.button(crate::i18n::tr(label)).clicked() {
            state.confirm = Some(id.into());
        }
        false
    }
}
pub(super) fn difference(ui: &mut egui::Ui, diff: &vcs::Difference) {
    ui.label(crate::i18n::format(
        "{0} file changes · {1} value changes",
        &[diff.files.len().to_string(), diff.values.len().to_string()],
    ));
    for file in &diff.files {
        ui.label(format!(
            "{} {} ({} → {} B)",
            change(&file.kind),
            file.path,
            file.before_bytes,
            file.after_bytes
        ));
    }
    for value in &diff.values {
        ui.collapsing(format!("{} {}", change(&value.kind), value.path), |ui| {
            ui.label(crate::i18n::tr("Before"));
            ui.label(summary(&value.before));
            ui.label(crate::i18n::tr("After"));
            ui.label(summary(&value.after));
        });
    }
    if diff.omitted {
        ui.label(crate::i18n::tr(
            "More changes omitted from this summary; the complete snapshot is preserved.",
        ));
    }
}
fn change(kind: &vcs::ChangeKind) -> &'static str {
    match kind {
        vcs::ChangeKind::Added => "+",
        vcs::ChangeKind::Modified => "~",
        vcs::ChangeKind::Deleted => "−",
    }
}
pub(super) fn summary(value: &Option<serde_json::Value>) -> String {
    match value {
        None => crate::i18n::tr("Deleted / missing"),
        Some(v) => {
            let text = v.to_string();
            if text.chars().count() > 500 {
                format!("{}…", text.chars().take(500).collect::<String>())
            } else {
                text
            }
        }
    }
}

pub(super) fn date(ms: u64) -> String {
    i64::try_from(ms)
        .ok()
        .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_millis)
        .map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| ms.to_string())
}

fn commit_name(commit: &vcs::Commit) -> String {
    if let Some(system) = &commit.system_name {
        crate::i18n::tr(system)
    } else if matches!(
        commit.kind.as_str(),
        "merge" | "cherry-pick" | "revert" | "backup"
    ) {
        crate::i18n::tr(&commit.name)
    } else {
        commit.name.clone()
    }
}
