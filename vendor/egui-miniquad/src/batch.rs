//! Ordered texture batches for egui meshes. Native PNGs retain straight alpha;
//! font/color atlas samples retain premultiplied alpha in the same draw.
use egui::{
    Color32, PaintCallback, Pos2, Rect, TextureId,
    epaint::{ClippedPrimitive, Primitive},
};

pub const TEXTURE_SLOTS: usize = 4;
const MAX_VERTICES: usize = u16::MAX as usize;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub pos: Pos2,
    pub uv: Pos2,
    pub color: Color32,
    pub material: [f32; 2],
}

pub struct Batch {
    pub clip_rect: Rect,
    pub textures: Vec<TextureId>,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}
pub enum Item {
    Mesh(Batch),
    Callback {
        clip_rect: Rect,
        callback: PaintCallback,
    },
}

pub fn prepare(primitives: Vec<ClippedPrimitive>) -> Vec<Item> {
    let mut output = vec![];
    let mut pending: Option<Batch> = None;
    for clipped in primitives {
        let mesh = match clipped.primitive {
            Primitive::Mesh(mesh) => mesh,
            Primitive::Callback(callback) => {
                if let Some(batch) = pending.take() {
                    output.push(Item::Mesh(batch));
                }
                output.push(Item::Callback {
                    clip_rect: clipped.clip_rect,
                    callback,
                });
                continue;
            }
        };
        if mesh.is_empty() {
            continue;
        }
        for mesh in mesh.split_to_u16() {
            let flush = pending.as_ref().is_some_and(|batch| {
                batch.clip_rect != clipped.clip_rect
                    || batch.vertices.len() + mesh.vertices.len() > MAX_VERTICES
                    || (batch.textures.len() == TEXTURE_SLOTS
                        && !batch.textures.contains(&mesh.texture_id))
            });
            if flush {
                output.push(Item::Mesh(pending.take().unwrap()));
            }
            let batch = pending.get_or_insert_with(|| Batch {
                clip_rect: clipped.clip_rect,
                textures: Vec::with_capacity(TEXTURE_SLOTS),
                vertices: vec![],
                indices: vec![],
            });
            let slot = batch
                .textures
                .iter()
                .position(|id| *id == mesh.texture_id)
                .unwrap_or_else(|| {
                    batch.textures.push(mesh.texture_id);
                    batch.textures.len() - 1
                });
            let base = batch.vertices.len() as u16;
            batch
                .indices
                .extend(mesh.indices.into_iter().map(|i| i + base));
            let material = [
                slot as f32,
                if matches!(mesh.texture_id, TextureId::User(_)) {
                    1.
                } else {
                    0.
                },
            ];
            batch
                .vertices
                .extend(mesh.vertices.into_iter().map(|v| Vertex {
                    pos: v.pos,
                    uv: v.uv,
                    color: v.color,
                    material,
                }));
        }
    }
    if let Some(batch) = pending {
        output.push(Item::Mesh(batch));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Mesh, pos2};
    fn quad(texture: TextureId, x: f32, clip: Rect) -> ClippedPrimitive {
        let mut mesh = Mesh::with_texture(texture);
        mesh.add_rect_with_uv(
            Rect::from_min_size(pos2(x, 0.), egui::vec2(10., 10.)),
            Rect::from_min_max(pos2(0., 0.), pos2(1., 1.)),
            Color32::WHITE,
        );
        ClippedPrimitive {
            clip_rect: clip,
            primitive: Primitive::Mesh(mesh),
        }
    }
    #[test]
    fn generated_fake_notes_and_selection_overlays_keep_order_in_one_batch() {
        let clip = Rect::EVERYTHING;
        let mut input = vec![];
        for i in 0..200 {
            input.push(quad(TextureId::User(17), i as f32, clip));
            input.push(quad(TextureId::default(), i as f32 + 0.1, clip));
            input.push(quad(TextureId::default(), i as f32 + 0.2, clip));
        }
        let output = prepare(input);
        assert_eq!(output.len(), 1);
        assert_eq!(std::mem::size_of::<Vertex>(), 28);
        let Item::Mesh(batch) = &output[0] else {
            panic!()
        };
        assert_eq!(
            batch.textures,
            vec![TextureId::User(17), TextureId::default()]
        );
        assert_eq!(batch.vertices.len(), 2400);
        assert_eq!(batch.indices.len(), 3600);
        for (i, v) in batch.vertices.as_chunks::<12>().0.iter().enumerate() {
            assert_eq!(v[0].pos.x, i as f32);
            assert_eq!(v[4].pos.x, i as f32 + 0.1);
            assert_eq!(v[0].material, [0., 1.]);
            assert_eq!(v[4].material, [1., 0.]);
        }
        assert_eq!(&batch.indices[6..12], &[4, 5, 6, 6, 5, 7]);
    }
    #[test]
    fn clipping_texture_capacity_callbacks_and_index_capacity_form_barriers() {
        let clip = Rect::from_min_max(pos2(0., 0.), pos2(100., 100.));
        let mut input: Vec<_> = (0..5)
            .map(|id| quad(TextureId::User(id), id as f32, clip))
            .collect();
        input.push(quad(TextureId::User(4), 6., clip.shrink(1.)));
        input.push(ClippedPrimitive {
            clip_rect: clip,
            primitive: Primitive::Callback(PaintCallback {
                rect: clip,
                callback: std::sync::Arc::new(()),
            }),
        });
        input.push(quad(TextureId::User(4), 7., clip));
        let output = prepare(input);
        assert_eq!(output.len(), 5);
        assert!(matches!(&output[3], Item::Callback { .. }));
        let output = prepare(
            (0..20000)
                .map(|i| quad(TextureId::User(1), i as f32, clip))
                .collect(),
        );
        assert_eq!(output.len(), 2);
        let mut vertices = 0;
        let mut indices = 0;
        for item in output {
            let Item::Mesh(batch) = item else { panic!() };
            assert!(batch.vertices.len() <= MAX_VERTICES);
            assert!(
                batch
                    .indices
                    .iter()
                    .all(|&i| usize::from(i) < batch.vertices.len())
            );
            vertices += batch.vertices.len();
            indices += batch.indices.len();
        }
        assert_eq!(vertices, 80000);
        assert_eq!(indices, 120000);
    }
}
