//! Editor-only atlas. Original resource-pack textures and native rendering stay intact.
use crate::{SkinStyle, SkinTexture};
use macroquad::prelude::{Image, Texture2D};
use prpr::{core::ResourcePack, ext::SafeTexture};

const GUTTER: usize = 1;
const MAX_SIDE: usize = 4096;
const MAX_PIXELS: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
struct Slot {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}
#[derive(Clone)]
pub struct GridAtlas {
    pub normal: SkinStyle,
    pub highlighted: SkinStyle,
    // SafeTexture retires these on the rendering thread, including skin switches.
    textures: Vec<SafeTexture>,
}
impl std::fmt::Debug for GridAtlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GridAtlas")
            .field("pages", &self.textures.len())
            .finish()
    }
}

fn layout(sizes: &[[usize; 2]], side: usize) -> Option<(Vec<Slot>, [usize; 2])> {
    layout_with_fit(sizes, side, false).or_else(|| layout_with_fit(sizes, side, true))
}
fn layout_with_fit(
    sizes: &[[usize; 2]],
    side: usize,
    best_fit: bool,
) -> Option<(Vec<Slot>, [usize; 2])> {
    let mut order: Vec<_> = (0..sizes.len()).collect();
    order.sort_unstable_by_key(|&i| std::cmp::Reverse((sizes[i][1], sizes[i][0])));
    let mut free = vec![Slot {
        x: 0,
        y: 0,
        w: side,
        h: side,
    }];
    let mut slots = vec![
        Slot {
            x: 0,
            y: 0,
            w: 0,
            h: 0
        };
        sizes.len()
    ];
    let mut used = [0, 0];
    for i in order {
        let [w, h] = sizes[i].map(|v| v + 2 * GUTTER);
        let at = if best_fit {
            free.iter()
                .enumerate()
                .filter(|(_, r)| r.w >= w && r.h >= h)
                .min_by_key(|(_, r)| r.w * r.h)
                .map(|(i, _)| i)?
        } else {
            free.iter().position(|r| r.w >= w && r.h >= h)?
        };
        let r = free.remove(at);
        slots[i] = Slot {
            x: r.x + GUTTER,
            y: r.y + GUTTER,
            w: w - 2 * GUTTER,
            h: h - 2 * GUTTER,
        };
        used[0] = used[0].max(r.x + w);
        used[1] = used[1].max(r.y + h);
        // Keep a full-height column beside tall Holds; smaller sprites stack below.
        if r.w > w {
            free.push(Slot {
                x: r.x + w,
                y: r.y,
                w: r.w - w,
                h: r.h,
            });
        }
        if r.h > h {
            free.push(Slot {
                x: r.x,
                y: r.y + h,
                w,
                h: r.h - h,
            });
        }
    }
    (used[0] * used[1] <= MAX_PIXELS).then_some((slots, used))
}
fn copy_with_gutter(out: &mut [u8], width: usize, source: &[u8], slot: Slot, repeat: bool) {
    for y in 0..slot.h + 2 * GUTTER {
        let sy = if repeat {
            (y + slot.h - GUTTER) % slot.h
        } else {
            y.saturating_sub(GUTTER).min(slot.h - 1)
        };
        for x in 0..slot.w + 2 * GUTTER {
            let sx = if repeat {
                (x + slot.w - GUTTER) % slot.w
            } else {
                x.saturating_sub(GUTTER).min(slot.w - 1)
            };
            let from = (sy * slot.w + sx) * 4;
            let to = ((slot.y + y - GUTTER) * width + slot.x + x - GUTTER) * 4;
            out[to..to + 4].copy_from_slice(&source[from..from + 4]);
        }
    }
}
fn groups(sizes: &[[usize; 2]], limit: usize) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = vec![];
    for i in 0..sizes.len() {
        if layout(&[sizes[i]], limit).is_none() {
            continue;
        }
        if let Some(group) = groups.iter_mut().find(|group| {
            let mut proposed: Vec<_> = group.iter().map(|&j| sizes[j]).collect();
            proposed.push(sizes[i]);
            layout(&proposed, limit).is_some()
        }) {
            group.push(i);
        } else {
            groups.push(vec![i]);
        }
    }
    groups
}
impl GridAtlas {
    pub(crate) fn new(
        pack: &ResourcePack,
        normal: &SkinStyle,
        highlighted: &SkinStyle,
    ) -> Option<Self> {
        let mut limit = 0;
        unsafe {
            macroquad::miniquad::gl::glGetIntegerv(
                macroquad::miniquad::gl::GL_MAX_TEXTURE_SIZE,
                &mut limit,
            );
        }
        let limit = (limit as usize).min(MAX_SIDE);
        let mut styles = [normal.clone(), highlighted.clone()];
        let mut sources = vec![
            &pack.note_style.click,
            &pack.note_style.hold,
            &pack.note_style.flick,
            &pack.note_style.drag,
            &pack.note_style_mh.click,
            &pack.note_style_mh.hold,
            &pack.note_style_mh.flick,
            &pack.note_style_mh.drag,
        ];
        let mut body_indices = vec![];
        for (i, style) in [&pack.note_style, &pack.note_style_mh]
            .into_iter()
            .enumerate()
        {
            if let Some(body) = &style.hold_body {
                body_indices.push((sources.len(), i));
                sources.push(body);
            }
        }
        let sizes: Vec<_> = sources
            .iter()
            .map(|s| [s.width() as usize, s.height() as usize])
            .collect();
        let groups = groups(&sizes, limit);
        let mut textures = vec![];
        let mut total_pixels = 0;
        for group in groups {
            // Single/oversized images keep their original texture. This bounds
            // atlas memory even for imported packs with very large resources.
            if group.len() < 2 {
                continue;
            }
            let (slots, size) =
                layout(&group.iter().map(|&i| sizes[i]).collect::<Vec<_>>(), limit)?;
            if total_pixels + size[0] * size[1] > MAX_PIXELS {
                continue;
            }
            total_pixels += size[0] * size[1];
            let mut pixels = vec![0; size[0] * size[1] * 4];
            for (&i, &slot) in group.iter().zip(&slots) {
                copy_with_gutter(
                    &mut pixels,
                    size[0],
                    &sources[i].get_texture_data().bytes,
                    slot,
                    i >= 8,
                );
            }
            let texture: SafeTexture = Texture2D::from_image(&Image {
                width: size[0] as u16,
                height: size[1] as u16,
                bytes: pixels,
            })
            .into();
            texture.set_filter(macroquad::prelude::FilterMode::Linear);
            let id = texture.raw_miniquad_texture_handle().gl_internal_id() as u64;
            for (&i, r) in group.iter().zip(&slots) {
                let sprite = if i < 8 {
                    let style = &mut styles[i / 4];
                    match i % 4 {
                        0 => &mut style.tap,
                        1 => &mut style.hold,
                        2 => &mut style.flick,
                        _ => &mut style.drag,
                    }
                } else {
                    let style = body_indices
                        .iter()
                        .find(|(index, _)| *index == i)
                        .unwrap()
                        .1;
                    let native = styles[style].body_texture.unwrap();
                    styles[style].body_atlas.get_or_insert(native)
                };
                *sprite = SkinTexture {
                    id,
                    size: sprite.size,
                    uv: [
                        r.x as f32 / size[0] as f32,
                        r.y as f32 / size[1] as f32,
                        (r.x + r.w) as f32 / size[0] as f32,
                        (r.y + r.h) as f32 / size[1] as f32,
                    ],
                };
            }
            textures.push(texture);
        }
        // Native body textures remain available when subpixel repeats would
        // otherwise require an excessive number of atlas quads.
        (!textures.is_empty()).then(|| Self {
            normal: styles[0].clone(),
            highlighted: styles[1].clone(),
            textures,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packs_original_sprites_without_resizing_or_overlapping() {
        let sizes = [
            [989, 100],
            [989, 2000],
            [989, 200],
            [989, 60],
            [1089, 200],
            [1056, 1997],
            [1089, 300],
            [1089, 160],
        ];
        let (slots, size) = layout(&sizes, 4096).unwrap();
        assert!(size[0] * size[1] < 7 * 1024 * 1024);
        for (i, r) in slots.iter().enumerate() {
            assert_eq!([r.w, r.h], sizes[i]);
            assert!(r.x >= 1 && r.y >= 1 && r.x + r.w < size[0] && r.y + r.h < size[1]);
            for s in &slots[..i] {
                assert!(
                    r.x + r.w + 2 <= s.x
                        || s.x + s.w + 2 <= r.x
                        || r.y + r.h + 2 <= s.y
                        || s.y + s.h + 2 <= r.y
                );
            }
        }
        assert!(layout(&sizes, 2048).is_none());
        let mut repeated = sizes.to_vec();
        repeated.extend([[989, 1900], [1056, 1852]]);
        assert!(layout(&repeated, 4096).is_some());
        let pages = groups(&sizes, 2048);
        assert!(pages.len() > 1);
        let mut covered: Vec<_> = pages.iter().flatten().copied().collect();
        covered.sort_unstable();
        assert_eq!(covered, (0..8).collect::<Vec<_>>());
        for page in pages {
            assert!(layout(&page.iter().map(|&i| sizes[i]).collect::<Vec<_>>(), 2048).is_some());
        }
    }
    #[test]
    fn gutter_preserves_straight_alpha_pixels_and_clamped_edges() {
        let pixels = [
            255, 40, 90, 0, 10, 200, 255, 127, 60, 20, 90, 255, 40, 200, 100, 12,
        ];
        let mut out = vec![0; 4 * 4 * 4];
        copy_with_gutter(
            &mut out,
            4,
            &pixels,
            Slot {
                x: 1,
                y: 1,
                w: 2,
                h: 2,
            },
            false,
        );
        for y in 0usize..4 {
            for x in 0usize..4 {
                let from = (y.saturating_sub(1).min(1) * 2 + x.saturating_sub(1).min(1)) * 4;
                assert_eq!(
                    &out[(y * 4 + x) * 4..(y * 4 + x + 1) * 4],
                    &pixels[from..from + 4]
                );
            }
        }
        let mut repeated = vec![0; 4 * 4 * 4];
        copy_with_gutter(
            &mut repeated,
            4,
            &pixels,
            Slot {
                x: 1,
                y: 1,
                w: 2,
                h: 2,
            },
            true,
        );
        assert_eq!(&repeated[0..4], &pixels[12..16]);
        assert_eq!(&repeated[12..16], &pixels[8..12]);
    }
}
