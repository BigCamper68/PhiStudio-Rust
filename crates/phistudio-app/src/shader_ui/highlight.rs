use super::*;
pub(super) fn glsl_layout(source: &str) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let mut rest = source;
    while !rest.is_empty() {
        let (length, color) = if rest.starts_with("//") {
            (rest.find('\n').unwrap_or(rest.len()), muted())
        } else if rest.starts_with("/*") {
            (rest.find("*/").map_or(rest.len(), |i| i + 2), muted())
        } else if rest.starts_with('#') {
            (rest.find('\n').unwrap_or(rest.len()), hold())
        } else if rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
            let length = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '.')
                .unwrap_or(rest.len());
            let word = &rest[..length];
            let color = if [
                "uniform",
                "varying",
                "attribute",
                "precision",
                "mediump",
                "highp",
                "lowp",
                "float",
                "int",
                "vec2",
                "vec3",
                "vec4",
                "sampler2D",
                "void",
                "if",
                "else",
                "for",
                "return",
                "const",
            ]
            .contains(&word)
            {
                accent()
            } else if word.starts_with(|c: char| c.is_ascii_digit()) {
                hold()
            } else {
                text()
            };
            (length, color)
        } else {
            (rest.chars().next().unwrap().len_utf8(), text())
        };
        job.append(
            &rest[..length],
            0.,
            egui::TextFormat {
                font_id: FontId::monospace(13.),
                color,
                ..Default::default()
            },
        );
        rest = &rest[length..];
    }
    job
}
