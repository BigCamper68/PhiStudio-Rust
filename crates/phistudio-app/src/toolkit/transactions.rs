use super::*;

impl Workbench {
    pub(crate) fn apply_tool(&mut self, edit: Result<ToolEdit, ChartError>) -> bool {
        self.apply_tool_with_memory(edit, true)
    }
    pub(crate) fn apply_tool_with_memory(
        &mut self,
        edit: Result<ToolEdit, ChartError>,
        remember: bool,
    ) -> bool {
        if remember && edit.is_ok() {
            self.remember_tool_parameters();
        }
        let anchor = self.anchor();
        match edit.and_then(|e| {
            if self.options.xy_link {
                e.apply_linked(&mut self.editor)
            } else {
                e.apply(&mut self.editor)
            }
        }) {
            Ok(Some(selection)) => {
                self.remember_anchor(anchor);
                if !selection.notes.iter().any(|id| id.line == self.line)
                    && !selection.events.iter().any(|id| id.line == self.line)
                    && let Some(line) = selection
                        .notes
                        .first()
                        .map(|id| id.line)
                        .or_else(|| selection.events.first().map(|id| id.line))
                {
                    self.line = line;
                }
                if !selection.events.is_empty()
                    && !selection
                        .events
                        .iter()
                        .any(|id| id.line == self.line && id.layer == self.events.layer)
                    && let Some(id) = selection.events.iter().find(|id| id.line == self.line)
                {
                    self.events.layer = id.layer;
                }
                if !selection.is_empty() {
                    self.tools.scope = if selection.notes.is_empty() {
                        Scope::Events
                    } else if selection.events.is_empty() {
                        Scope::Notes
                    } else {
                        Scope::Both
                    };
                }
                self.set_selection(selection);
                self.timeline.cancel_gesture();
                self.transport.playing = false;
                self.error = None;
                self.status = self
                    .editor
                    .undo_label()
                    .unwrap_or("Selection updated")
                    .into();
                true
            }
            Ok(None) => {
                self.error = None;
                self.status = "No changes".into();
                false
            }
            Err(e) => {
                self.error = Some(e.to_string());
                false
            }
        }
    }

    pub(crate) fn transform_selection(&mut self, operation: Transform) {
        if !self.story_selection().is_empty() {
            let result = self.timing_selection().and_then(|normal| {
                phistudio_core::objects::transform(
                    self.editor.document(),
                    &normal,
                    &self.story_selection(),
                    operation,
                )
            });
            self.apply_object_tool(result);
            return;
        }
        let timing = matches!(
            operation,
            Transform::Shift { .. }
                | Transform::Quantize(_)
                | Transform::ReverseTime
                | Transform::AlignTime
                | Transform::ScaleTime(_)
        );
        let selection = if timing {
            self.timing_selection()
        } else {
            Ok(self.selection())
        };
        let edit = selection.and_then(|s| tools::transform(self.editor.document(), &s, operation));
        self.apply_tool(edit);
    }
}
