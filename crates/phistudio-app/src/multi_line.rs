//! Editor-only line views; object IDs always retain their source line.
use crate::{grid_input::Grid, scroll::TouchScroll as _, workbench::Workbench};
use egui::{Pos2, Rect, pos2};

pub(crate) struct LineRemap {
    destinations: Vec<Option<usize>>,
    focus: Option<usize>,
}

pub(crate) struct LineViewChange {
    pub before: Vec<usize>,
    pub after: Vec<usize>,
    pub before_line: usize,
    pub after_line: usize,
}

pub(crate) struct MultiLine {
    pub show: bool,
    pub enabled: bool,
    pub events: bool,
    pub merged: bool,
    pub lines: Vec<usize>,
    pub widths: [f32; 2],
    pub offsets: [f32; 2],
    pub expression: String,
    pub error: Option<String>,
    pub base: Option<Grid>,
    pub line_history: std::collections::BTreeMap<u64, LineViewChange>,
}

impl Default for MultiLine {
    fn default() -> Self {
        Self {
            show: false,
            enabled: false,
            events: false,
            merged: false,
            lines: Vec::new(),
            widths: [280., 360.],
            offsets: [0.; 2],
            expression: String::new(),
            error: None,
            base: None,
            line_history: Default::default(),
        }
    }
}
impl MultiLine {
    pub fn preferences(&self, count: usize) -> phistudio_core::MultiLinePreferences {
        phistudio_core::MultiLinePreferences {
            all: self.lines.len() == count,
            enabled: self.enabled,
            events: self.events,
            merged: self.merged,
            lines: if self.lines.len() == count {
                vec![]
            } else {
                self.lines.clone()
            },
            widths: self.widths,
        }
    }
    pub fn apply_preferences(&mut self, p: &phistudio_core::MultiLinePreferences, count: usize) {
        self.enabled = p.enabled;
        self.events = p.events;
        self.merged = p.merged;
        self.lines = if p.all {
            (0..count).collect()
        } else {
            p.lines.clone()
        };
        self.widths = p.widths;
    }
}

pub(crate) fn parse_lines(
    text: &str,
    doc: &phistudio_core::ChartDocument,
) -> Result<Vec<usize>, String> {
    if text.len() > 4096 {
        return Err("Invalid line selection".into());
    }
    let groups = doc.group_names().map_err(|e| e.to_string())?;
    let mut result = std::collections::BTreeSet::new();
    // Quotes allow group names containing spaces; numbers are zero based.
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    for ch in text.chars() {
        match ch {
            '"' => quoted = !quoted,
            c if !quoted && (c.is_whitespace() || c == ',') => {
                if !token.is_empty() {
                    tokens.push(std::mem::take(&mut token));
                }
            }
            c => token.push(c),
        }
    }
    if quoted {
        return Err("Invalid line selection".into());
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    for token in tokens {
        if token.eq_ignore_ascii_case("all") {
            result.extend(0..doc.line_count());
        } else if let Some(group) = groups.iter().position(|g| *g == token) {
            result.extend((0..doc.line_count()).filter(|i| doc.line_group(*i) == group));
        } else if let Ok(line) = token.parse::<usize>() {
            if line >= doc.line_count() {
                return Err("Invalid line selection".into());
            }
            result.insert(line);
        } else if let Some((a, b)) = token.split_once(':').or_else(|| token.split_once('-')) {
            let a = a.parse::<usize>().map_err(|_| "Invalid line selection")?;
            let b = b.parse::<usize>().map_err(|_| "Invalid line selection")?;
            if a > b || b >= doc.line_count() {
                return Err("Invalid line selection".into());
            }
            result.extend(a..=b);
        } else {
            return Err("Invalid line selection".into());
        }
    }
    if result.is_empty() {
        return Err("Choose at least one line".into());
    }
    Ok(result.into_iter().collect())
}

impl Workbench {
    pub(crate) fn reconcile_multi_selection(&mut self) {
        let notes = self.target_lines(false);
        let events = self.target_lines(true);
        let mut selected = self.selection();
        selected.notes.retain(|id| notes.contains(&id.line));
        selected.events.retain(|id| events.contains(&id.line));
        let stories = self
            .story_selection()
            .into_iter()
            .filter(|id| events.contains(&id.line))
            .collect();
        self.set_object_selection(selected, stories);
    }

    pub(crate) fn open_multi_line(&mut self) {
        self.multi_line.show = true;
        self.multi_line.expression =
            if self.multi_line.lines.len() == self.editor.document().line_count() {
                "all".into()
            } else {
                self.multi_line
                    .lines
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            };
    }

    pub(crate) fn pending_line_remap(
        &self,
        command: &phistudio_core::Command,
    ) -> Option<LineRemap> {
        use phistudio_core::Command;
        let count = self.editor.document().line_count();
        let focus = match command {
            Command::AddLines { .. } | Command::DuplicateLine(_) => Some(count),
            Command::MoveLine { .. } | Command::DeleteLine(_) => None,
            _ => return None,
        };
        let destinations = (0..count)
            .map(|old| match *command {
                Command::MoveLine { from, to } => Some(
                    phistudio_core::line_management::moved_line_index(old, from, to),
                ),
                Command::DeleteLine(deleted) if old == deleted => None,
                Command::DeleteLine(deleted) => Some(if old > deleted { old - 1 } else { old }),
                _ => Some(old),
            })
            .collect();
        Some(LineRemap {
            destinations,
            focus,
        })
    }

    pub(crate) fn apply_line_remap(&mut self, remap: LineRemap) {
        let before = self.multi_line.lines.clone();
        let before_line = self.line;
        self.line = remap
            .focus
            .or_else(|| remap.destinations.get(self.line).copied().flatten())
            .unwrap_or(self.line.min(self.editor.document().line_count() - 1));
        self.multi_line.lines = before
            .iter()
            .filter_map(|old| remap.destinations.get(*old).copied().flatten())
            .collect();
        if self.multi_line.lines.is_empty() || (self.multi_line.enabled && remap.focus.is_some()) {
            self.multi_line.lines.push(self.line);
        }
        self.multi_line.lines.sort_unstable();
        self.multi_line.lines.dedup();
        self.multi_line.expression = self
            .multi_line
            .lines
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        if let Some(id) = self.editor.action_id(false) {
            self.multi_line.line_history.insert(
                id,
                LineViewChange {
                    before,
                    after: self.multi_line.lines.clone(),
                    before_line,
                    after_line: self.line,
                },
            );
            while self.multi_line.line_history.len() > 512 {
                self.multi_line.line_history.pop_first();
            }
        }
        self.select(None);
        self.line_draft = None;
        self.timeline.cancel_gesture();
    }

    pub(crate) fn multi_active(&self, events: bool) -> bool {
        self.multi_line.enabled
            && self.multi_line.events == events
            && !self.multi_line.lines.is_empty()
            && if events {
                matches!(
                    self.advanced.right_mode,
                    crate::advanced_ui::RightMode::Events
                        | crate::advanced_ui::RightMode::Storyboard
                )
            } else {
                !self.advanced.right_mode.dedicated()
            }
    }
    pub(crate) fn target_lines(&self, events: bool) -> Vec<usize> {
        if self.multi_active(events) {
            self.multi_line
                .lines
                .iter()
                .copied()
                .filter(|i| *i < self.editor.document().line_count())
                .collect()
        } else {
            vec![self.line]
        }
    }
    pub(crate) fn line_panels(&self, grid: Grid, events: bool) -> Vec<(usize, Grid)> {
        let lines = self.target_lines(events);
        if !self.multi_active(events) {
            return vec![(self.line, grid)];
        }
        if !events && self.multi_line.merged {
            return lines.into_iter().map(|line| (line, grid)).collect();
        }
        let viewport = if events { grid.event_rect } else { grid.rect };
        let width = self.multi_line.widths[usize::from(events)];
        let width = if width > 0. {
            width.clamp(80., 2400.)
        } else {
            viewport.width().max(80.)
        };
        let max = (lines.len() as f32 * (width + 8.) - 8. - viewport.width()).max(0.);
        let offset = self.multi_line.offsets[usize::from(events)].clamp(0., max);
        lines
            .into_iter()
            .enumerate()
            .skip((offset / (width + 8.)).floor() as usize)
            .take((viewport.width() / (width + 8.)).ceil() as usize + 2)
            .filter_map(|(i, line)| {
                let rect = Rect::from_min_max(
                    pos2(
                        viewport.left() + i as f32 * (width + 8.) - offset,
                        viewport.top(),
                    ),
                    pos2(
                        viewport.left() + i as f32 * (width + 8.) - offset + width,
                        viewport.bottom(),
                    ),
                );
                rect.intersects(viewport).then_some((
                    line,
                    if events {
                        Grid {
                            event_rect: rect,
                            ..grid
                        }
                    } else {
                        Grid { rect, ..grid }
                    },
                ))
            })
            .collect()
    }
    pub(crate) fn multi_base(&self, grid: Grid) -> Grid {
        self.multi_line.base.map_or(grid, |base| Grid {
            rect: base.rect,
            event_rect: base.event_rect,
            ..grid
        })
    }
    pub(crate) fn input_line_grid(&mut self, grid: Grid, pos: Pos2) -> Option<Grid> {
        let events = grid.event_rect.contains(pos);
        if !grid.rect.contains(pos) && !events {
            return Some(grid);
        }
        if !self.multi_active(events) {
            return Some(grid);
        }
        if !events && self.multi_line.merged {
            return Some(grid);
        }
        if let Some((line, panel)) = self.line_panels(grid, events).into_iter().find(|(_, p)| {
            if events {
                p.event_rect.contains(pos)
            } else {
                p.rect.contains(pos)
            }
        }) {
            if self.line != line {
                self.line = line;
                self.draft = None;
                self.clear_event_draft();
            }
            Some(panel)
        } else {
            None
        }
    }
    pub(crate) fn multi_scrollbar(&mut self, ui: &mut egui::Ui, grid: Grid, events: bool) {
        if !self.multi_active(events) || (!events && self.multi_line.merged) {
            return;
        }
        let viewport = if events { grid.event_rect } else { grid.rect };
        let rect = Rect::from_min_max(
            pos2(viewport.left(), viewport.top() - 62.),
            pos2(viewport.right(), viewport.top() - 42.),
        );
        let width = self.multi_line.widths[usize::from(events)].max(80.);
        let max =
            (self.multi_line.lines.len() as f32 * (width + 8.) - 8. - viewport.width()).max(0.);
        ui.allocate_ui_at_rect(rect, |ui| {
            ui.set_clip_rect(rect.intersect(ui.clip_rect()));
            ui.spacing_mut().interact_size.y = 18.;
            ui.spacing_mut().slider_width = (rect.width() - 12.).max(20.);
            if ui
                .add(
                    egui::Slider::new(&mut self.multi_line.offsets[usize::from(events)], 0. ..=max)
                        .show_value(false),
                )
                .changed()
            {
                self.timeline.cancel_gesture();
            }
        });
    }
    pub(crate) fn multi_line_window(&mut self, ctx: &egui::Context) {
        if self.show_projects || !self.multi_line.show {
            return;
        }
        let mut open = true;
        let max_size = egui::vec2(
            (ctx.screen_rect().width() - 24.).max(180.),
            (ctx.screen_rect().height() - 80.).max(100.),
        );
        crate::i18n::window("Multiple lines")
            .open(&mut open)
            .default_pos([48., 56.])
            .default_width(700.)
            .default_height(560.)
            // Also enlarge a small window restored from an earlier version.
            .resize(|r| {
                r.min_size([max_size.x.min(640.), max_size.y.min(280.)])
                    .max_size(max_size)
            })
            .show(ctx, |ui| {
                ui.set_enabled(!self.modal_open());
                ui.style_mut().wrap = Some(true);
                let before = (
                    self.multi_line.enabled,
                    self.multi_line.events,
                    self.multi_line.merged,
                    self.multi_line.lines.clone(),
                    self.multi_line.widths,
                );
                let height = ui.available_height();
                // Sibling viewports keep settings from consuming the line list's
                // height, including when the window is restored or scaled down.
                ui.columns(2, |columns| {
                    egui::ScrollArea::vertical()
                        .id_source("multiple-lines-controls")
                        .max_height(height)
                        .touch_show_y(&mut columns[0], |ui| self.multi_line_controls(ui));
                    egui::ScrollArea::vertical()
                        .id_source("multiple-lines-list")
                        .max_height(height)
                        .touch_show_y(&mut columns[1], |ui| self.multi_line_list(ui));
                });

                self.multi_line.lines.sort_unstable();
                self.multi_line.lines.dedup();
                if self.multi_line.enabled
                    && !self.multi_line.lines.is_empty()
                    && !self.multi_line.lines.contains(&self.line)
                {
                    self.line = self.multi_line.lines[0];
                }
                if self.multi_line.enabled && self.multi_line.lines.is_empty() {
                    self.multi_line.lines.push(self.line);
                }
                let after = (
                    self.multi_line.enabled,
                    self.multi_line.events,
                    self.multi_line.merged,
                    self.multi_line.lines.clone(),
                    self.multi_line.widths,
                );
                if before.3 != after.3 {
                    self.multi_line.expression = self
                        .multi_line
                        .lines
                        .iter()
                        .map(|i| i.to_string())
                        .collect::<Vec<_>>()
                        .join(" ");
                }
                if before.0 != after.0 || before.1 != after.1 || before.3 != after.3 {
                    self.reconcile_multi_selection();
                }
                if before != after {
                    self.timeline.cancel_gesture();
                    self.multi_line.offsets = [0.; 2];
                }
            });
        self.multi_line.show = open;
    }

    fn multi_line_controls(&mut self, ui: &mut egui::Ui) {
        use crate::i18n::tr;
        ui.checkbox(&mut self.multi_line.enabled, tr("Edit multiple lines"));
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.multi_line.events, false, tr("Notes"));
            ui.selectable_value(&mut self.multi_line.events, true, tr("Events / Storyboard"));
            ui.add_enabled_ui(!self.multi_line.events, |ui| {
                ui.checkbox(&mut self.multi_line.merged, tr("Overlay note lines"));
            });
        });
        ui.label(tr(
            "New objects go to the current line. Select objects to edit their original lines.",
        ));
        let index = usize::from(self.multi_line.events);
        ui.collapsing(tr("Groups"), |ui| {
            if let Ok(groups) = self.editor.document().group_names() {
                for (group, name) in groups.iter().enumerate() {
                    if ui.button(name).clicked() {
                        self.multi_line.lines = (0..self.editor.document().line_count())
                            .filter(|i| self.editor.document().line_group(*i) == group)
                            .collect();
                        self.multi_line.enabled = !self.multi_line.lines.is_empty();
                        self.multi_line.error = None;
                    }
                }
            }
        });
        ui.label(tr("Line column width"));
        if self.multi_line.widths[index] < 80. {
            self.multi_line.widths[index] = if index == 0 { 280. } else { 360. };
        }
        ui.add(egui::Slider::new(&mut self.multi_line.widths[index], 80. ..=1200.).suffix(" px"));
        if ui.button(tr("Fit line columns")).clicked()
            && let Some(base) = self.multi_line.base
        {
            let width = if self.multi_line.events {
                base.event_rect.width()
            } else {
                base.rect.width()
            };
            self.multi_line.widths[index] = ((width
                - 8. * self.multi_line.lines.len().saturating_sub(1) as f32)
                / self.multi_line.lines.len().max(1) as f32)
                .max(80.);
        }
        ui.label(tr("Lines: numbers, ranges, group names or all"));
        ui.add(
            egui::TextEdit::singleline(&mut self.multi_line.expression)
                .desired_width(ui.available_width()),
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr("Apply")).clicked() {
                match parse_lines(&self.multi_line.expression, self.editor.document()) {
                    Ok(lines) => {
                        self.multi_line.lines = lines;
                        self.multi_line.enabled = true;
                        self.multi_line.error = None;
                    }
                    Err(e) => self.multi_line.error = Some(e),
                }
            }
            if ui.button(tr("All lines")).clicked() {
                self.multi_line.lines = (0..self.editor.document().line_count()).collect();
                self.multi_line.enabled = true;
            }
            if ui.button(tr("Current line only")).clicked() {
                self.multi_line.lines = vec![self.line];
                self.multi_line.enabled = false;
            }
            if ui.button(tr("Clear")).clicked() {
                self.multi_line.lines.clear();
                self.multi_line.enabled = false;
            }
        });
        if let Some(error) = &self.multi_line.error {
            ui.colored_label(ui.visuals().error_fg_color, tr(error));
        }
    }

    fn multi_line_list(&mut self, ui: &mut egui::Ui) {
        use crate::i18n::tr;
        let summaries = self.line_summaries();
        for summary in summaries.iter() {
            ui.horizontal(|ui| {
                let mut checked = self.multi_line.lines.contains(&summary.index);
                if ui
                    .allocate_ui_with_layout(
                        egui::vec2((ui.available_width() - 90.).max(60.), 32.),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.checkbox(
                                &mut checked,
                                format!("L{} · {}", summary.index, summary.name),
                            )
                        },
                    )
                    .inner
                    .changed()
                {
                    if checked {
                        self.multi_line.lines.push(summary.index);
                    } else {
                        self.multi_line.lines.retain(|i| *i != summary.index);
                    }
                    self.multi_line.enabled = !self.multi_line.lines.is_empty();
                }
                if ui
                    .selectable_label(self.line == summary.index, tr("Current"))
                    .clicked()
                {
                    self.line = summary.index;
                    if self.multi_line.enabled && !self.multi_line.lines.contains(&self.line) {
                        self.multi_line.lines.push(self.line);
                    }
                }
            });
        }
    }
}
