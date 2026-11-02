use super::*;
impl Workbench {
    pub(super) fn remap_noise_order(&mut self, from: usize, to: usize) {
        self.noise.selected = Some(to);
        self.noise.selection = self
            .noise
            .selection
            .iter()
            .map(|i| {
                if *i == from {
                    to
                } else if from < to && *i > from && *i <= to {
                    *i - 1
                } else if from > to && *i >= to && *i < from {
                    *i + 1
                } else {
                    *i
                }
            })
            .collect();
        self.refresh_noise();
    }
    pub(crate) fn noise_actions(&mut self, label: &'static str, actions: Vec<Action>) -> bool {
        let anchor = self.anchor();
        let commands = actions.into_iter().map(Command::EditNoise).collect();
        match self.editor.execute_batch(label, commands) {
            Ok(changed) => {
                if changed {
                    self.transport.playing = false;
                    self.remember_anchor(anchor);
                    self.noise.cache = None;
                }
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(e.to_string());
                false
            }
        }
    }
    pub(crate) fn flush_noise_draft(&mut self) -> bool {
        if self.advanced.right_mode != RightMode::NoiseFields {
            return true;
        }
        self.refresh_noise();
        let result = (|| -> Result<Vec<Action>, phistudio_core::ChartError> {
            let Some(index) = self.noise.selected else {
                return Ok(vec![]);
            };
            let Some(draft) = &self.noise.draft else {
                return Ok(vec![]);
            };
            let mut value = draft.raw.clone();
            let f = noise::format(&value)?;
            for (i, key) in noise::PHASES.iter().enumerate() {
                if f == Format::Next && (i == 1 || i == 2) {
                    continue;
                }
                noise::put_beat(
                    &mut value,
                    key,
                    draft.times[i].parse()?,
                    f,
                    self.editor.document().tempo(),
                )?;
            }
            let mut actions = vec![Action::Update { index, value }];
            if let (Some(key_index), Some(d)) = (self.noise.key_selected, &self.noise.key_draft) {
                let mut key = d.raw.clone();
                let native = self.noise.track.native(f);
                if native {
                    noise::put_beat(
                        &mut key,
                        "time",
                        d.times[0].parse()?,
                        Format::Native,
                        self.editor.document().tempo(),
                    )?;
                } else {
                    for (name, text) in ["startTime", "endTime"].iter().zip(&d.times) {
                        noise::put_beat(
                            &mut key,
                            name,
                            text.parse()?,
                            Format::Next,
                            self.editor.document().tempo(),
                        )?;
                    }
                }
                actions.push(Action::Key {
                    field: index,
                    track: self.noise.track,
                    index: Some(key_index),
                    value: Some(key),
                });
            }
            Ok(actions)
        })();
        match result {
            Ok(actions) => {
                let result = self.noise_actions("Edit Noise Field", actions);
                if result {
                    self.refresh_noise();
                }
                result
            }
            Err(e) => {
                self.error = Some(e.to_string());
                false
            }
        }
    }
    pub(crate) fn select_noise(&mut self, index: Option<usize>, additive: bool) {
        if !self.flush_noise_draft() {
            return;
        }
        self.refresh_noise();
        if self.noise.animation {
            self.noise.key_selected = index;
            self.noise.key_draft = None;
            self.load_noise_key();
        } else {
            self.noise.selected = index;
            self.noise.cache = None;
            self.noise.key_selected = None;
            self.refresh_noise();
        }
        if !additive {
            self.noise.selection.clear();
        }
        if let Some(i) = index {
            if additive && !self.noise.selection.insert(i) {
                self.noise.selection.remove(&i);
                let next = self.noise.selection.first().copied();
                if self.noise.animation {
                    self.noise.key_selected = next;
                    self.noise.key_draft = None;
                    self.load_noise_key();
                } else {
                    self.noise.selected = next;
                    self.noise.cache = None;
                    self.refresh_noise();
                }
            } else {
                self.noise.selection.insert(i);
            }
        }
        self.windows.sidebar = true;
        self.windows.scroll_to_properties = true;
        self.transport.playing = false;
    }
    pub(crate) fn add_noise(&mut self) {
        if !self.flush_noise_draft() {
            return;
        }
        self.refresh_noise();
        let a = Beat::from_grid(self.transport.beat, self.timeline.division).unwrap_or(Beat::ZERO);
        let b = a
            .checked_add(if self.noise.animation {
                self.noise_tick()
            } else {
                Beat::new(4, 0, 1).unwrap()
            })
            .unwrap_or(Beat::ONE);
        if self.noise.animation {
            if let Some(field) = self.noise.selected {
                match noise::new_key_interval(
                    &self.noise.rows[field],
                    self.noise.track,
                    a,
                    b,
                    self.editor.document().tempo(),
                ) {
                    Ok(values) => {
                        let i = self.noise_count();
                        if self.noise_actions(
                            "Add Noise Field key",
                            values
                                .into_iter()
                                .map(|value| Action::Key {
                                    field,
                                    track: self.noise.track,
                                    index: None,
                                    value: Some(value),
                                })
                                .collect(),
                        ) {
                            self.noise.key_selected = Some(i);
                            self.noise.selection = [i].into();
                            self.refresh_noise();
                        }
                    }
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
        } else {
            let index = self.noise.rows.len();
            let mut value = noise::create(
                a,
                b,
                self.noise.format,
                self.noise.subtract,
                self.editor.document().tempo(),
            );
            layout::place(&mut value, index as f64);
            if self.noise_actions("Add Noise Field", vec![Action::Insert { index, value }]) {
                self.noise.selected = Some(index);
                self.noise.selection = [index].into();
                self.refresh_noise();
                if self.timeline.area.is_positive() {
                    let grid = crate::grid_input::geometry(self.timeline.area, self);
                    let col = layout::column(self, grid, layout::View::focus(self), index as f64);
                    if col.left() < grid.rect.left() || col.right() > grid.rect.right() {
                        self.noise.offset = layout::centered_offset(grid, index as f64);
                    }
                }
            }
        }
    }
    pub(crate) fn delete_noise(&mut self) {
        self.refresh_noise();
        let mut ids = self.noise.selection.clone();
        if ids.is_empty()
            && let Some(i) = if self.noise.animation {
                self.noise.key_selected
            } else {
                self.noise.selected
            }
        {
            ids.insert(i);
        }
        let field = self.noise.selected;
        let track = self.noise.track;
        let actions = ids
            .into_iter()
            .rev()
            .filter_map(|index| {
                if self.noise.animation {
                    Some(Action::Key {
                        field: field?,
                        track,
                        index: Some(index),
                        value: None,
                    })
                } else {
                    Some(Action::Delete(index))
                }
            })
            .collect();
        // Deleting also works with invalid, unapplied text in the inspector.
        self.noise.draft = None;
        self.noise.key_draft = None;
        if self.noise_actions("Delete Noise Fields", actions) {
            self.noise.selection.clear();
            if self.noise.animation {
                self.noise.key_selected = None;
            } else {
                self.noise.selected = None;
            }
            self.refresh_noise();
        }
    }
    fn copy_noise(&mut self) -> bool {
        if !self.flush_noise_draft() {
            return false;
        }
        self.refresh_noise();
        let mut ids = self.noise.selection.clone();
        if ids.is_empty()
            && let Some(i) = if self.noise.animation {
                self.noise.key_selected
            } else {
                self.noise.selected
            }
        {
            ids.insert(i);
        }
        let rows = if self.noise.animation {
            self.noise
                .selected
                .and_then(|i| noise::keys(&self.noise.rows[i], self.noise.track).ok())
                .unwrap_or(&[])
        } else {
            &self.noise.rows
        };
        let copied: Vec<_> = ids.iter().filter_map(|i| rows.get(*i)).cloned().collect();
        if copied.is_empty() {
            return false;
        }
        let f = self.noise_format();
        let native = self.noise.animation && self.noise.track.native(f);
        let key = if self.noise.animation {
            if native { "time" } else { "startTime" }
        } else {
            "appearTime"
        };
        let format = if self.noise.animation && !native {
            Format::Next
        } else {
            f
        };
        let origin = copied
            .iter()
            .filter_map(|r| noise::beat(r, key, format, self.editor.document().tempo()).ok())
            .min()
            .unwrap_or(Beat::ZERO);
        self.noise.clipboard = Some(Clipboard {
            format: f,
            track: self.noise.animation.then_some(self.noise.track),
            rows: copied,
            origin,
        });
        true
    }
    pub(crate) fn paste_noise(&mut self, at_cursor: bool) {
        if !self.flush_noise_draft() {
            return;
        }
        self.refresh_noise();
        let Some(clip) = self.noise.clipboard.clone() else {
            return;
        };
        if clip.track != self.noise.animation.then_some(self.noise.track)
            || (self.noise.animation && clip.format != self.noise_format())
        {
            self.error = Some("Paste Noise Field keys into the same track and format".into());
            return;
        }
        let target = if at_cursor {
            Beat::from_grid(self.transport.beat, self.timeline.division).unwrap_or(Beat::ZERO)
        } else if let Some(track) = clip.track {
            let end = if track.native(clip.format) {
                "time"
            } else {
                "endTime"
            };
            let format = if track.native(clip.format) {
                Format::Native
            } else {
                Format::Next
            };
            let end = clip
                .rows
                .iter()
                .filter_map(|v| noise::beat(v, end, format, self.editor.document().tempo()).ok())
                .max()
                .unwrap_or(clip.origin);
            if end > clip.origin {
                end
            } else {
                end.checked_add(Beat::new(0, 1, self.timeline.division).unwrap_or(Beat::ONE))
                    .unwrap_or(end)
            }
        } else {
            clip.origin
        };
        let mut actions = Vec::new();
        let mut selected = BTreeSet::new();
        for (count, mut value) in (self.noise_count()..).zip(clip.rows) {
            if let Some(track) = clip.track {
                match noise::shift_key(
                    &value,
                    track,
                    clip.format,
                    clip.origin,
                    target,
                    self.editor.document().tempo(),
                ) {
                    Ok(v) => value = v,
                    Err(e) => {
                        self.error = Some(e.to_string());
                        return;
                    }
                }
                let Some(field) = self.noise.selected else {
                    return;
                };
                actions.push(Action::Key {
                    field,
                    track,
                    index: None,
                    value: Some(value),
                });
            } else {
                match noise::shifted(&value, clip.origin, target, self.editor.document().tempo()) {
                    Ok(v) => value = v,
                    Err(e) => {
                        self.error = Some(e.to_string());
                        return;
                    }
                }
                actions.push(Action::Insert {
                    index: count,
                    value,
                });
            }
            selected.insert(count);
        }
        if self.noise_actions("Paste Noise Fields", actions) {
            self.noise.selection = selected;
            if self.noise.animation {
                self.noise.key_selected = self.noise.selection.first().copied();
            } else {
                self.noise.selected = self.noise.selection.first().copied();
            }
            self.refresh_noise();
        }
    }
    pub(crate) fn noise_key(&mut self, key: egui::Key, modifiers: egui::Modifiers) -> bool {
        use egui::{Key, Modifiers};
        self.refresh_noise();
        if modifiers.matches(Modifiers::COMMAND | Modifiers::SHIFT) && key == Key::Z {
            self.history_step(true);
            return true;
        }
        if modifiers.matches(Modifiers::COMMAND) {
            match key {
                Key::S => self.save_action(),
                Key::Z => self.history_step(false),
                Key::Y => self.history_step(true),
                Key::C => {
                    self.copy_noise();
                }
                Key::X => {
                    if self.copy_noise() {
                        self.delete_noise();
                    }
                }
                Key::D => {
                    if self.copy_noise() {
                        self.paste_noise(false);
                    }
                }
                Key::V => self.paste_noise(true),
                Key::A => {
                    self.noise.selection = (0..self.noise_count()).collect();
                }
                _ => return false,
            }
        } else if modifiers.matches(Modifiers::NONE) {
            match key {
                Key::Space => {
                    if self.flush_noise_draft() {
                        self.transport.playing = !self.transport.playing;
                    }
                }
                Key::Delete | Key::Backspace => self.delete_noise(),
                Key::Escape => self.back(),
                _ => return false,
            }
        } else {
            return false;
        }
        true
    }
}
