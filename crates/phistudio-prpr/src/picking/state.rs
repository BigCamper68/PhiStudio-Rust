//! Owns the picking target and source/visibility indices across native passes.
use super::*;
use std::collections::BTreeSet;
#[derive(Default)]
pub(crate) struct PickingState {
    pub target: Option<(RenderTarget, Material)>,
    pub sources: Vec<Option<NoteId>>,
    pub visible_lines: BTreeSet<usize>,
    pub visible_ids: Vec<usize>,
    visible: Vec<bool>,
}
impl PickingState {
    pub fn new(sources: Vec<Option<NoteId>>) -> Self {
        Self {
            sources,
            ..Self::default()
        }
    }
    pub fn replace_sources(&mut self, sources: Vec<Option<NoteId>>) {
        self.sources = sources;
        self.visible_lines.clear();
        // Release the previous chart's potentially large buffers at the boundary.
        // Reuse capacity between picks within the same parsed chart.
        self.visible_ids = Vec::new();
        self.visible = Vec::new();
    }
    pub fn target(&mut self, size: [u32; 2]) -> Result<(RenderTarget, Material)> {
        if let Some(target) = self.target {
            return Ok(target);
        }
        let material = load_material(VERTEX, FRAGMENT, MaterialParams::default())?;
        let target = (render_target(size[0], size[1]), material);
        self.target = Some(target);
        Ok(target)
    }
    pub fn resize(&mut self, w: u32, h: u32) {
        if let Some((target, _)) = &mut self.target {
            let old = std::mem::replace(target, render_target(w, h));
            old.delete();
        }
    }
    pub fn record_visible(&mut self, pixels: &[u8], count: usize) {
        self.visible.clear();
        self.visible.resize(count, false);
        self.visible_lines.clear();
        self.visible_ids.clear();
        for p in pixels.as_chunks::<4>().0 {
            let id = u32::from(p[0]) | (u32::from(p[1]) << 8) | (u32::from(p[2]) << 16);
            if id > 0
                && let Some(hit) = self.visible.get_mut(id as usize - 1)
            {
                *hit = true;
            }
        }
        self.visible_ids.extend(
            self.visible
                .iter()
                .enumerate()
                .filter_map(|(i, visible)| visible.then_some(i)),
        );
        for i in &self.visible_ids {
            if let Some(Some(note)) = self.sources.get(*i) {
                self.visible_lines.insert(note.line);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visibility_decodes_ids_in_source_order_and_discards_previous_pixels() {
        let mut state = PickingState::new(vec![
            Some(NoteId { line: 2, index: 0 }),
            None,
            Some(NoteId { line: 1, index: 4 }),
        ]);
        state.record_visible(
            &[
                3, 0, 0, 255, 1, 0, 0, 255, 3, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255,
            ],
            3,
        );
        assert_eq!(state.visible_ids, vec![0, 2]);
        assert_eq!(state.visible_lines, [1, 2].into());
        state.record_visible(&[2, 0, 0, 255], 3);
        assert_eq!(state.visible_ids, vec![1]);
        assert!(state.visible_lines.is_empty());
        state.replace_sources(vec![Some(NoteId { line: 7, index: 8 })]);
        assert!(state.visible_ids.is_empty());
        assert!(state.visible_lines.is_empty());
        state.record_visible(&[1, 0, 0, 255], 1);
        assert_eq!(state.visible_lines, [7].into());
        state.record_visible(&[], 0);
        assert!(state.visible_ids.is_empty());
    }
}
