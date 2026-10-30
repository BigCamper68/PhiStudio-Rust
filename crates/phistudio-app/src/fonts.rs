//! Retain the Rust UI faces and resolve missing glyphs with Phira's text face.
pub(crate) fn definitions() -> egui::FontDefinitions {
    let mut definitions = egui::FontDefinitions::default();
    definitions.font_data.insert(
        "phira-unicode".into(),
        egui::FontData::from_static(phistudio_prpr::TEXT_FONT_BYTES),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        definitions
            .families
            .entry(family)
            .or_default()
            .push("phira-unicode".into());
    }
    definitions
}

#[cfg(test)]
mod tests {
    #[test]
    fn ui_and_source_editor_resolve_unicode_glyphs_instead_of_replacement_boxes() {
        let ctx = egui::Context::default();
        ctx.set_fonts(super::definitions());
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                let font = egui::FontId::new(16., family);
                for c in "Русский ё Україна Ω Ⅱ ← ↗ ↔ ★ ♯ ∞ 中文 日本語 한글 ไทย 😀 🎵"
                    .chars()
                    .filter(|c| !c.is_whitespace())
                {
                    assert!(
                        ctx.fonts(|fonts| fonts.has_glyph(&font, c)),
                        "Missing {c} in {font:?}"
                    );
                }
            }
        });
    }
}
