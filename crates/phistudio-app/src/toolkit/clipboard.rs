use super::*;

impl Workbench {
    pub(crate) fn copy_selection(&mut self, cut: bool) {
        self.request_editor_action(if cut {
            phistudio_sdk::EditorAction::Cut
        } else {
            phistudio_sdk::EditorAction::Copy
        });
    }

    pub(crate) fn copy_selection_now(&mut self, cut: bool) {
        let result = self.timing_selection().and_then(|normal| {
            phistudio_core::objects::ObjectClipboard::copy(
                self.editor.document(),
                &normal,
                &self.story_selection(),
            )
        });
        match result {
            Ok(clipboard) => {
                if cut {
                    let edit = self.timing_selection().and_then(|normal| {
                        phistudio_core::objects::delete(
                            self.editor.document(),
                            &normal,
                            &self.story_selection(),
                        )
                    });
                    if !self.apply_object_tool(edit) {
                        return;
                    }
                }
                self.status = if cut {
                    format!("Cut {} objects", clipboard.len())
                } else {
                    format!("Copied {} objects", clipboard.len())
                };
                self.sounds.emit(if cut {
                    phistudio_core::EditorSoundEvent::Delete
                } else {
                    phistudio_core::EditorSoundEvent::Copy
                });
                self.tools.clipboard = Some(Arc::new(clipboard));
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub(crate) fn paste_selection(&mut self, mirrored: bool, original_time: bool) {
        self.request_editor_action(phistudio_sdk::EditorAction::Paste {
            mirrored,
            original_time,
        });
    }

    pub(crate) fn paste_selection_now(&mut self, mirrored: bool, original_time: bool) {
        let Some(clipboard) = self.tools.clipboard.clone() else {
            return;
        };
        let beat = if original_time {
            Ok(clipboard.first)
        } else {
            Beat::from_grid(self.transport.beat, self.timeline.division)
        };
        let edit = beat.and_then(|beat| {
            clipboard.paste(
                self.editor.document(),
                if clipboard.required_lines() > 1 && self.multi_line.enabled {
                    clipboard.origin_line
                } else {
                    self.line
                },
                self.events.layer,
                beat,
                mirrored,
            )
        });
        if self.apply_object_tool(edit) {
            self.sounds.emit(phistudio_core::EditorSoundEvent::Paste);
        }
    }

    pub(crate) fn duplicate_selection(&mut self) {
        self.request_editor_action(phistudio_sdk::EditorAction::Duplicate);
    }

    pub(crate) fn duplicate_selection_now(&mut self) {
        if !self.story_selection().is_empty() {
            let edit = phistudio_core::objects::ObjectClipboard::copy(
                self.editor.document(),
                &self.selection(),
                &self.story_selection(),
            )
            .and_then(|c| {
                let span = c.last.checked_sub(c.first)?.as_f64();
                let spacing = Beat::from_grid(((span + 1e-7) / 4.).ceil().max(1.) * 4., 1)?;
                c.paste(
                    self.editor.document(),
                    c.origin_line,
                    c.origin_layer,
                    c.first.checked_add(spacing)?,
                    false,
                )
            });
            self.apply_object_tool(edit);
            return;
        }
        let selection = match self.timing_selection() {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        let edit = ChartClipboard::copy(self.editor.document(), &selection).and_then(|c| {
            let span = c.last.checked_sub(c.first)?.as_f64();
            let spacing = Beat::from_grid(((span + 1e-7) / 4.).ceil().max(1.) * 4., 1)?;
            tools::repeat(self.editor.document(), &selection, 1, spacing)
        });
        self.apply_tool(edit);
    }
}
