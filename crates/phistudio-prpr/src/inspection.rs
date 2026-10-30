use super::*;

impl Preview {
    pub fn resource_warning(&self) -> Option<&str> {
        self.resource_warning.as_deref()
    }

    pub fn line_count(&self) -> usize {
        self.anchors.source_lines
    }

    pub fn note_count(&self) -> usize {
        self.chart.lines.iter().map(|x| x.notes.len()).sum()
    }

    /// GL texture for ordinary egui image composition, including clipping and
    /// overlapping floating windows. prpr still renders the complete chart.
    pub fn texture_id(&self) -> u64 {
        self.target
            .texture
            .raw_miniquad_texture_handle()
            .gl_internal_id() as u64
    }

    pub fn editor_skin(&self) -> EditorSkin {
        EditorSkin::new(&self.resource.res_pack)
    }

    pub fn highlighted_notes(&self) -> std::collections::BTreeSet<phistudio_core::NoteId> {
        self.chart
            .lines
            .iter()
            .flat_map(|l| l.notes.iter())
            .enumerate()
            .filter(|(_, n)| n.multiple_hint)
            .filter_map(|(i, _)| self.source_ids.get(i).copied().flatten())
            .collect()
    }

    pub fn skin_report(&self) -> serde_json::Value {
        let info = &self.resource.res_pack.info;
        let rgba = |c: Color| [c.r, c.g, c.b, c.a];
        serde_json::json!({"name":info.name,"author":info.author,"line_color":rgba(self.resource.judge_line_color),
            "perfect":rgba(info.color_perfect()),"good":rgba(info.color_good()),"hit_fx":info.hit_fx,
            "hit_fx_duration":info.hit_fx_duration,"hit_fx_scale":info.hit_fx_scale,
            "hold_repeat":info.hold_repeat,"hide_particles":info.hide_particles,
            "hit_fx_rotate":info.hit_fx_rotate,"hit_fx_tinted":info.hit_fx_tinted})
    }

    pub fn snapshot(&self) -> Image {
        self.target.texture.get_texture_data()
    }

    pub fn render_texture(&self) -> Texture2D {
        self.target.texture
    }
}
