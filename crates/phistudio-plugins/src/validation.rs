use phistudio_sdk::{DrawLayer, DrawPrimitive, DrawSpace, Widget, valid_id};
use std::collections::BTreeSet;

pub fn widgets(widgets: &[Widget]) -> Result<(), String> {
    fn walk(
        widgets: &[Widget],
        depth: usize,
        count: &mut usize,
        ids: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        if depth > 32 {
            return Err("Widget tree exceeds 32 levels".into());
        }
        for widget in widgets {
            *count += 1;
            if *count > 2048 {
                return Err("Panel exceeds 2048 widgets".into());
            }
            let id = match widget {
                Widget::Row { children } | Widget::Column { children } => {
                    walk(children, depth + 1, count, ids)?;
                    None
                }
                Widget::Label { .. } => None,
                Widget::Canvas {
                    height,
                    layers: content,
                } => {
                    if !height.is_finite() || !(16. ..=1024.).contains(height) {
                        return Err("Canvas height must be between 16 and 1024 points".into());
                    }
                    layers(content, false)?;
                    None
                }
                Widget::Button { id, .. } | Widget::Bool { id, .. } | Widget::Text { id, .. } => {
                    Some(id)
                }
                Widget::Number {
                    id,
                    value,
                    min,
                    max,
                    ..
                } => {
                    if ![value, min, max].into_iter().all(|v| v.is_finite())
                        || min > max
                        || value < min
                        || value > max
                    {
                        return Err("Invalid numeric widget bounds/value".into());
                    }
                    Some(id)
                }
                Widget::Choice {
                    id, value, options, ..
                } => {
                    if !options.iter().any(|(key, _)| key == value) {
                        return Err("Choice value is not an option".into());
                    }
                    Some(id)
                }
            };
            if let Some(id) = id
                && (!valid_id(id) || !ids.insert(id.clone()))
            {
                return Err("Widget IDs must be valid and unique within a panel".into());
            }
        }
        Ok(())
    }
    walk(widgets, 0, &mut 0, &mut BTreeSet::new())
}

pub fn layers(layers: &[DrawLayer], timeline: bool) -> Result<(), String> {
    if layers.len() > 32 || layers.iter().map(|l| l.primitives.len()).sum::<usize>() > 20_000 {
        return Err("Drawing exceeds 32 layers or 20,000 primitives".into());
    }
    let mut text_bytes = 0;
    for layer in layers {
        if layer.space == DrawSpace::Timeline && !timeline {
            return Err("Timeline coordinates need a timeline viewport".into());
        }
        for p in &layer.primitives {
            if let DrawPrimitive::Text { text, .. } = p {
                text_bytes += text.len();
            }
        }
        drawing(&layer.primitives)?;
    }
    if text_bytes > 256 * 1024 {
        return Err("Drawing exceeds 256 KiB of text".into());
    }
    Ok(())
}

pub fn drawing(primitives: &[DrawPrimitive]) -> Result<(), String> {
    if primitives.len() > 20_000 {
        return Err("Drawing exceeds 20,000 primitives".into());
    }
    for primitive in primitives {
        let valid = match primitive {
            DrawPrimitive::Image { file, min, max, .. } => {
                *file != 0
                    && coordinates(min.iter().chain(max))
                    && min[0] <= max[0]
                    && min[1] <= max[1]
            }
            DrawPrimitive::Line {
                from, to, width, ..
            } => {
                coordinates(from.iter().chain(to))
                    && width.is_finite()
                    && (0. ..=256.).contains(width)
            }
            DrawPrimitive::Rect { min, max, .. } => {
                coordinates(min.iter().chain(max)) && min[0] <= max[0] && min[1] <= max[1]
            }
            DrawPrimitive::Circle { center, radius, .. } => {
                coordinates(center.iter()) && radius.is_finite() && (0. ..=8192.).contains(radius)
            }
            DrawPrimitive::Text {
                position,
                size,
                text,
                ..
            } => {
                coordinates(position.iter())
                    && size.is_finite()
                    && (1. ..=256.).contains(size)
                    && text.len() <= 16_384
            }
        };
        if !valid {
            return Err("Invalid drawing geometry".into());
        }
    }
    Ok(())
}
fn coordinates<'a>(values: impl Iterator<Item = &'a f64>) -> bool {
    values.into_iter().all(|v| v.is_finite() && v.abs() <= 1e12)
}
