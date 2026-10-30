use super::*;

/// Append consecutive images to one mesh without reordering notes or overlays.
/// This also avoids allocating a separate shape/vertex/index buffer per sprite.
pub(crate) struct NoteBatch<'a> {
    painter: &'a egui::Painter,
    mesh: egui::Mesh,
}

impl<'a> NoteBatch<'a> {
    pub(crate) fn new(painter: &'a egui::Painter) -> Self {
        Self {
            painter,
            mesh: egui::Mesh::default(),
        }
    }
    pub(crate) fn flush(&mut self) {
        if !self.mesh.is_empty() {
            self.painter.add(std::mem::take(&mut self.mesh));
        }
    }
    fn image(
        &mut self,
        texture: phistudio_prpr::SkinTexture,
        rect: Rect,
        uv: [f32; 4],
        tint: Color32,
    ) {
        if !rect.intersects(self.painter.clip_rect()) {
            return;
        }
        let id = egui::TextureId::User(texture.id);
        if self.mesh.texture_id != id {
            self.flush();
            self.mesh.texture_id = id;
        }
        let [x0, y0, x1, y1] = texture.uv;
        let uv = Rect::from_min_max(
            pos2(x0 + uv[0] * (x1 - x0), y0 + uv[1] * (y1 - y0)),
            pos2(x0 + uv[2] * (x1 - x0), y0 + uv[3] * (y1 - y0)),
        );
        self.mesh.add_rect_with_uv(rect, uv, tint);
    }
}

pub(super) fn style_margin(s: &phistudio_prpr::SkinStyle, width: f32) -> f32 {
    let width = width * s.width_scale;
    let aspect = |t: phistudio_prpr::SkinTexture| width * t.size[1] / t.size[0];
    (aspect(s.tap) / 2.)
        .max(aspect(s.flick) / 2.)
        .max(aspect(s.drag) / 2.)
        .max(aspect(s.hold) * (s.head[3] - s.head[1]).max(s.tail[3] - s.tail[1]))
}

pub(crate) fn note_margin(skin: Option<&phistudio_prpr::EditorSkin>, width: f32) -> f32 {
    skin.map_or(24., |s| {
        style_margin(&s.normal, width)
            .max(style_margin(&s.highlighted, width))
            .max(24.)
    }) + 8.
}

pub(super) fn paint_bounds(
    grid: &Grid,
    note: &Note,
    skin: Option<&phistudio_prpr::SkinStyle>,
) -> Rect {
    let Some(skin) = skin else {
        return grid.note_rect(note).expand(24.);
    };
    let width = grid.note_width * skin.width_scale;
    let x = grid.x(note.x);
    let y = grid.y(note.start_time.as_f64());
    let rect = if note.kind == NoteKind::Hold {
        let height = width * skin.hold.size[1] / skin.hold.size[0];
        Rect::from_min_max(
            pos2(
                x - width / 2.,
                grid.y(note.end_time.as_f64()) - height * (skin.tail[3] - skin.tail[1]),
            ),
            pos2(x + width / 2., y + height * (skin.head[3] - skin.head[1])),
        )
    } else {
        let t = match note.kind {
            NoteKind::Tap => skin.tap,
            NoteKind::Flick => skin.flick,
            NoteKind::Drag => skin.drag,
            NoteKind::Hold => unreachable!(),
        };
        Rect::from_center_size(pos2(x, y), vec2(width, width * t.size[1] / t.size[0]))
    };
    // Selection stroke/handles, fake marker and below-line dot remain visible at edges.
    rect.union(Rect::from_center_size(pos2(x + 30., y), vec2(8., 8.)))
        .expand(6.)
}

pub(crate) fn draw_note(
    batch: &mut NoteBatch<'_>,
    grid: &Grid,
    note: &Note,
    selected: bool,
    ghost: bool,
    skin: Option<&phistudio_prpr::SkinStyle>,
) {
    let rect = paint_bounds(grid, note, skin);
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
        draw_texture(batch, grid, note, skin, ghost)
    } else {
        batch.flush();
        let painter = batch.painter;
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
    if !selected && note.is_fake == 0 && note.above == 1 {
        return;
    }
    batch.flush();
    let painter = batch.painter;
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

pub(super) fn draw_texture(
    batch: &mut NoteBatch<'_>,
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
    let clip = batch.painter.clip_rect();
    let mut image = |texture: phistudio_prpr::SkinTexture, rect: Rect, uv: [f32; 4]| {
        bounds = bounds.union(rect);
        batch.image(texture, rect, uv, tint);
    };
    if note.kind == NoteKind::Hold {
        let end = grid.y(note.end_time.as_f64());
        let body_rect = Rect::from_min_max(pos2(x - width / 2., end), pos2(x + width / 2., y));
        if let Some(texture) = skin.body_texture {
            let period = width * texture.size[1] / texture.size[0];
            if let Some(atlas) = skin.body_atlas
                && let Some(tiles) = repeat_span(body_rect, clip, period)
            {
                for tile in tiles {
                    let top = body_rect.top() + tile as f32 * period;
                    let bottom =
                        (body_rect.top() + (tile + 1) as f32 * period).min(body_rect.bottom());
                    image(
                        atlas,
                        Rect::from_min_max(
                            pos2(body_rect.left(), top),
                            pos2(body_rect.right(), bottom),
                        ),
                        [0., 0., 1., (bottom - top) / period],
                    );
                }
            } else {
                image(
                    texture,
                    body_rect,
                    [0., 0., 1., body_rect.height() / period],
                );
            }
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

pub(super) fn repeat_span(rect: Rect, clip: Rect, period: f32) -> Option<std::ops::Range<i32>> {
    if !period.is_finite()
        || period <= 0.
        || rect.top().abs() > 1_000_000.
        || rect.height() / period > 1_000_000.
    {
        return None;
    }
    if !rect.intersects(clip) {
        return Some(0..0);
    }
    let first = ((clip.top().max(rect.top()) - rect.top()) / period).floor() as i32;
    let last = ((clip.bottom().min(rect.bottom()) - rect.top()) / period).ceil() as i32;
    (i64::from(last) - i64::from(first) <= 128).then_some(first..last)
}
