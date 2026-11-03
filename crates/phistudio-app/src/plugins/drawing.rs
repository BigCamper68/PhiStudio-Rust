use phistudio_sdk::{
    DrawColor, DrawLayer, DrawPrimitive, DrawSpace, ThemeColor, TimelineViewport, Viewport,
};

fn color(color: DrawColor) -> egui::Color32 {
    use crate::theme;
    match color {
        DrawColor::Rgba([r, g, b, a]) => egui::Color32::from_rgba_unmultiplied(r, g, b, a),
        DrawColor::Theme(token) => match token {
            ThemeColor::Base => theme::base(),
            ThemeColor::Panel => theme::panel(),
            ThemeColor::Raised => theme::raised(),
            ThemeColor::Border => theme::border(),
            ThemeColor::Muted => theme::muted(),
            ThemeColor::Text => theme::text(),
            ThemeColor::Accent => theme::accent(),
            ThemeColor::Tap => theme::tap(),
            ThemeColor::Hold => theme::hold(),
            ThemeColor::Flick => theme::flick(),
            ThemeColor::Drag => theme::drag(),
        },
    }
}
fn point(
    point: [f64; 2],
    space: DrawSpace,
    rect: egui::Rect,
    viewport: &Viewport,
) -> Option<egui::Pos2> {
    let [x, y] = match space {
        DrawSpace::Points => point,
        DrawSpace::Normalized => [
            point[0] * f64::from(rect.width()),
            point[1] * f64::from(rect.height()),
        ],
        DrawSpace::Timeline => {
            let t = viewport.timeline.as_ref()?;
            [
                (point[0] - t.x_min) / (t.x_max - t.x_min) * f64::from(rect.width()),
                f64::from(t.baseline) - (point[1] - t.beat) * f64::from(t.pixels_per_beat),
            ]
        }
    };
    let p = rect.min + egui::vec2(x as f32, y as f32);
    p.is_finite().then_some(p)
}
pub(super) fn paint(
    painter: &egui::Painter,
    rect: egui::Rect,
    viewport: &Viewport,
    layers: &[DrawLayer],
    images: &super::images::Textures,
) {
    let clip = rect.intersect(painter.clip_rect());
    if !clip.is_positive() {
        return;
    }
    let p = painter.with_clip_rect(clip);
    for layer in layers {
        for primitive in &layer.primitives {
            match primitive {
                DrawPrimitive::Image {
                    file,
                    min,
                    max,
                    tint,
                } => {
                    if let (Some(texture), Some(a), Some(b)) = (
                        images.get(file),
                        point(*min, layer.space, rect, viewport),
                        point(*max, layer.space, rect, viewport),
                    ) {
                        let full = egui::Rect::from_two_pos(a, b);
                        let clipped = full.intersect(p.clip_rect());
                        if full.is_positive() && clipped.is_positive() {
                            let uv = egui::Rect::from_min_max(
                                egui::pos2(
                                    (clipped.left() - full.left()) / full.width(),
                                    (clipped.top() - full.top()) / full.height(),
                                ),
                                egui::pos2(
                                    (clipped.right() - full.left()) / full.width(),
                                    (clipped.bottom() - full.top()) / full.height(),
                                ),
                            );
                            p.image(*texture, clipped, uv, color(*tint));
                        }
                    }
                }
                DrawPrimitive::Line {
                    from,
                    to,
                    color: c,
                    width,
                } => {
                    if let (Some(a), Some(b)) = (
                        point(*from, layer.space, rect, viewport),
                        point(*to, layer.space, rect, viewport),
                    ) {
                        p.line_segment([a, b], egui::Stroke::new(*width, color(*c)));
                    }
                }
                DrawPrimitive::Rect { min, max, color: c } => {
                    if let (Some(a), Some(b)) = (
                        point(*min, layer.space, rect, viewport),
                        point(*max, layer.space, rect, viewport),
                    ) {
                        let fill = egui::Rect::from_two_pos(a, b).intersect(p.clip_rect());
                        if fill.is_positive() {
                            p.rect_filled(fill, 0., color(*c));
                        }
                    }
                }
                DrawPrimitive::Circle {
                    center,
                    radius,
                    color: c,
                } => {
                    if let Some(center) = point(*center, layer.space, rect, viewport)
                        && egui::Rect::from_center_size(center, egui::Vec2::splat(*radius * 2.))
                            .intersects(p.clip_rect())
                    {
                        p.circle_filled(center, *radius, color(*c));
                    }
                }
                DrawPrimitive::Text {
                    position,
                    text,
                    size,
                    color: c,
                } => {
                    if let Some(position) = point(*position, layer.space, rect, viewport)
                        && p.clip_rect().expand(256.).contains(position)
                    {
                        p.text(
                            position,
                            egui::Align2::LEFT_TOP,
                            text,
                            egui::FontId::monospace(*size),
                            color(*c),
                        );
                    }
                }
            }
        }
    }
}
pub(super) fn paint_clock(painter: &egui::Painter, rect: egui::Rect, t: &TimelineViewport) {
    let p = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let first = (t.beat + f64::from(t.baseline - rect.height()) / f64::from(t.pixels_per_beat))
        .floor() as i64;
    let count = (rect.height() / t.pixels_per_beat).ceil().clamp(0., 2048.) as i64 + 2;
    for beat in first..first.saturating_add(count) {
        let y = rect.top() + t.baseline - ((beat as f64 - t.beat) as f32) * t.pixels_per_beat;
        p.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1_f32, crate::theme::border()),
        );
        p.text(
            egui::pos2(rect.left() + 4., y - 14.),
            egui::Align2::LEFT_TOP,
            beat.to_string(),
            egui::FontId::monospace(11.),
            crate::theme::muted(),
        );
    }
    let y = rect.top() + t.baseline;
    p.line_segment(
        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
        egui::Stroke::new(1.5_f32, crate::theme::accent()),
    );
}
