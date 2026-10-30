//! Unchanged 1.18.0 note painter (e8f02a1), used only by the desktop GL probe.
use crate::{grid_input::Grid, theme::*};
use egui::{Align2, Color32, FontId, Rect, Shape, Stroke, pos2, vec2};
use phistudio_core::{Note, NoteKind};
pub(super) fn draw_note(
    painter: &egui::Painter,
    grid: &Grid,
    note: &Note,
    selected: bool,
    ghost: bool,
    skin: Option<&phistudio_prpr::SkinStyle>,
) {
    let rect = grid.note_rect(note);
    if !rect.intersects(grid.rect) {
        return;
    }
    let color = match note.kind {
        NoteKind::Tap => tap(),
        NoteKind::Hold => hold(),
        NoteKind::Flick => flick(),
        NoteKind::Drag => drag(),
    };
    let fill = if ghost {
        color.gamma_multiply(0.6)
    } else {
        color
    };
    let x = grid.x(note.x);
    let y = grid.y(note.start_time.as_f64());
    let visual_rect = if let Some(skin) = skin {
        draw_texture(painter, grid, note, skin, ghost)
    } else {
        if note.kind == NoteKind::Hold {
            let body = Rect::from_min_max(
                pos2(x - 9., grid.y(note.end_time.as_f64())),
                pos2(x + 9., y),
            );
            painter.rect_filled(body, 4., color.gamma_multiply(0.55));
            painter.rect_filled(
                Rect::from_center_size(pos2(x, body.top()), vec2(40., 9.)),
                3.,
                fill,
            );
        }
        if note.kind == NoteKind::Drag {
            painter.add(Shape::convex_polygon(
                vec![
                    pos2(x - 23., y),
                    pos2(x, y - 8.),
                    pos2(x + 23., y),
                    pos2(x, y + 8.),
                ],
                fill,
                Stroke::NONE,
            ));
        } else {
            painter.rect_filled(Rect::from_center_size(pos2(x, y), vec2(38., 9.)), 3., fill);
        }
        if note.kind == NoteKind::Flick {
            painter.line_segment(
                [pos2(x - 7., y - 12.), pos2(x, y - 18.)],
                Stroke::new(2_f32, color),
            );
            painter.line_segment(
                [pos2(x, y - 18.), pos2(x + 7., y - 12.)],
                Stroke::new(2_f32, color),
            );
        }
        match note.kind {
            NoteKind::Hold => Rect::from_min_max(
                pos2(x - 20., grid.y(note.end_time.as_f64()) - 4.5),
                pos2(x + 20., y + 4.5),
            ),
            NoteKind::Drag => Rect::from_center_size(pos2(x, y), vec2(46., 16.)),
            NoteKind::Flick => Rect::from_min_max(pos2(x - 19., y - 18.), pos2(x + 19., y + 4.5)),
            NoteKind::Tap => Rect::from_center_size(pos2(x, y), vec2(38., 9.)),
        }
    };
    if selected {
        painter.rect_stroke(visual_rect.expand(1.5), 1., Stroke::new(1_f32, accent()));
        if note.kind == NoteKind::Hold {
            for y in [
                grid.y(note.start_time.as_f64()),
                grid.y(note.end_time.as_f64()),
            ] {
                painter.circle_filled(pos2(x, y), 3.5, accent());
                painter.circle_stroke(pos2(x, y), 3.5, Stroke::new(1_f32, base()));
            }
        }
    }
    if note.is_fake != 0 {
        painter.text(
            pos2(x, y),
            Align2::CENTER_CENTER,
            crate::i18n::tr("F"),
            FontId::monospace(11.),
            base(),
        );
    }
    if note.above != 1 {
        painter.circle_filled(pos2(x + 30., y), 3., muted());
    }
}

fn draw_texture(
    painter: &egui::Painter,
    grid: &Grid,
    note: &Note,
    skin: &phistudio_prpr::SkinStyle,
    ghost: bool,
) -> Rect {
    let x = grid.x(note.x);
    let y = grid.y(note.start_time.as_f64());
    let width = grid.note_width * skin.width_scale;
    let tint = if ghost {
        Color32::from_white_alpha(145)
    } else {
        Color32::WHITE
    };
    let mut bounds = Rect::NOTHING;
    let mut image = |texture: phistudio_prpr::SkinTexture, rect: Rect, uv: [f32; 4]| {
        bounds = bounds.union(rect);
        painter.image(
            egui::TextureId::User(texture.id),
            rect,
            Rect::from_min_max(pos2(uv[0], uv[1]), pos2(uv[2], uv[3])),
            tint,
        );
    };
    if note.kind == NoteKind::Hold {
        let end = grid.y(note.end_time.as_f64());
        let body_rect = Rect::from_min_max(pos2(x - width / 2., end), pos2(x + width / 2., y));
        if let Some(texture) = skin.body_texture {
            let repeats = body_rect.height() / (width * texture.size[1] / texture.size[0]);
            image(texture, body_rect, [0., 0., 1., repeats]);
        } else {
            image(skin.hold, body_rect, skin.body);
        }
        let mut cap = |uv: [f32; 4], at: f32, tail: bool| {
            let height = width * skin.hold.size[1] / skin.hold.size[0] * (uv[3] - uv[1]);
            let rect = if tail {
                Rect::from_min_size(pos2(x - width / 2., at - height), vec2(width, height))
            } else {
                Rect::from_min_size(pos2(x - width / 2., at), vec2(width, height))
            };
            image(skin.hold, rect, uv);
        };
        cap(skin.tail, end, true);
        cap(skin.head, y, false);
    } else {
        let texture = match note.kind {
            NoteKind::Tap => skin.tap,
            NoteKind::Flick => skin.flick,
            NoteKind::Drag => skin.drag,
            NoteKind::Hold => unreachable!(),
        };
        image(
            texture,
            Rect::from_center_size(
                pos2(x, y),
                vec2(width, width * texture.size[1] / texture.size[0]),
            ),
            [0., 0., 1., 1.],
        );
    }
    bounds
}
