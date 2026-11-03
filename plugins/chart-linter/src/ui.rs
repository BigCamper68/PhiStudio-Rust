use crate::*;
const PAGE: usize = 20;
fn label(text: impl Into<String>) -> Widget {
    Widget::Label { text: text.into() }
}
fn button(id: impl Into<String>, text: impl Into<String>, enabled: bool) -> Widget {
    Widget::Button {
        id: id.into(),
        text: text.into(),
        enabled,
    }
}
fn choice(id: &str, text: &str, value: &str, options: &[(&str, &str)]) -> Widget {
    Widget::Choice {
        id: id.into(),
        text: text.into(),
        value: value.into(),
        options: options
            .iter()
            .map(|(a, b)| (String::from(*a), String::from(*b)))
            .collect(),
    }
}
impl ChartLinter {
    pub(super) fn panel(&self) -> Effect {
        let busy = self.scan.is_some();
        let mut widgets = vec![
            Widget::Row {
                children: vec![
                    button("scan", "Basic scan", !busy),
                    button("deep", "Deep scan", !busy),
                    button("cancel", "Cancel", busy),
                ],
            },
            choice(
                "target",
                "Compatibility target",
                &self.target,
                &[
                    ("next", "rpe-next / PhiStudio"),
                    ("rpe", "RPE"),
                    ("phira", "Phira"),
                ],
            ),
            label(&self.message),
            label(format!(
                "Errors: {} · Warnings: {} · Info: {}",
                self.report.counts[0], self.report.counts[1], self.report.counts[2]
            )),
        ];
        if let Some(scan) = &self.scan {
            widgets.push(label(format!(
                "Checked {} notes / {} events; native inspections: {}",
                self.report.checked_notes, self.report.checked_events, scan.deep_checked
            )))
        }
        widgets.extend([
            choice(
                "severity",
                "Severity",
                &self.severity,
                &[
                    ("all", "All"),
                    ("error", "Errors"),
                    ("warning", "Warnings"),
                    ("info", "Info"),
                ],
            ),
            choice(
                "category",
                "Object type",
                &self.category,
                &[
                    ("all", "All"),
                    ("notes", "Notes"),
                    ("events", "Events"),
                    ("resources", "Resources"),
                    ("chart", "Chart"),
                ],
            ),
            Widget::Bool {
                id: "info".into(),
                text: "Show informational results".into(),
                value: self.show_info,
            },
            Widget::Text {
                id: "search".into(),
                text: "Filter by message or path".into(),
                value: self.search.clone(),
            },
        ]);
        if let Some(index) = self.confirm
            && let Some(issue) = self.report.issues.get(index)
        {
            widgets.push(label(format!(
                "Remove {}? This is one ordinary editor Undo step.",
                issue.path
            )));
            widgets.push(Widget::Row {
                children: vec![
                    button("remove_confirm", "Remove object", !busy),
                    button("keep", "Keep object", true),
                ],
            });
        }
        let query = self.search.to_lowercase();
        let visible: Vec<_> = self
            .report
            .issues
            .iter()
            .enumerate()
            .filter(|(_, i)| {
                let severity = match i.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Info => "info",
                };
                (self.severity == "all" || self.severity == severity)
                    && (self.show_info || i.severity != Severity::Info)
                    && (self.category == "all" || self.category == i.category)
                    && (query.is_empty()
                        || i.message.to_lowercase().contains(&query)
                        || i.path.to_lowercase().contains(&query)
                        || i.code.contains(&query))
            })
            .collect();
        let pages = visible.len().div_ceil(PAGE).max(1);
        let page = self.page.min(pages - 1);
        widgets.push(Widget::Row {
            children: vec![
                button("previous", "Previous", page > 0),
                label(format!(
                    "Page {} / {} · {} matching results",
                    page + 1,
                    pages,
                    visible.len()
                )),
                button("next", "Next", page + 1 < pages),
            ],
        });
        if self.report.counts.iter().sum::<usize>() > self.report.issues.len() {
            widgets.push(label(format!("Showing the first {} issues (10,000 rows / 8 MiB limit); totals include every checked issue.",self.report.issues.len())))
        }
        for (index, issue) in visible.into_iter().skip(page * PAGE).take(PAGE) {
            let mut children = vec![
                label(format!(
                    "{:?} · {}{}",
                    issue.severity,
                    issue.code,
                    if issue.deep { " · deep" } else { "" }
                )),
                label(&issue.message),
                label(if issue.path.is_empty() {
                    "Chart"
                } else {
                    &issue.path
                }),
            ];
            let mut actions = vec![button(format!("locate.r{index}"), "Locate", !busy)];
            if issue.removable {
                actions.push(button(format!("remove.r{index}"), "Remove…", !busy))
            }
            children.push(Widget::Row { children: actions });
            widgets.push(Widget::Column { children });
        }
        if self.report.complete && self.report.counts.iter().sum::<usize>() == 0 {
            widgets.push(label("No issues found by the completed checks."))
        }
        Effect::Panel {
            id: "report".into(),
            widgets,
        }
    }
}
