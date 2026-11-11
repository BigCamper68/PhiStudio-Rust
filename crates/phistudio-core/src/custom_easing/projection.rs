//! Only playback/export copies expand logical events into portable RPE intervals.
use super::{
    authoring::{Recipe, signature},
    *,
};
use crate::{Beat, EventType, LineEvent, TempoMap};
use serde_json::Value;

pub fn has_bindings(root: &Value) -> bool {
    root["judgeLineList"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|line| {
            line["eventLayers"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|layer| {
                    EventType::ALL.into_iter().any(|kind| {
                        layer[kind.key()]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|raw| raw.get(CURVE_KEY).is_some_and(|v| !v.is_null()))
                    })
                })
        })
}

pub fn expand(
    root: &mut Value,
    tempo: &TempoMap,
    recipes: bool,
    canceled: impl Fn() -> bool,
) -> Result<(), ChartError> {
    let bpm = root["BPMList"].clone();
    let Some(lines) = root.get_mut("judgeLineList").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    let mut total = 0;
    for line in lines {
        let Some(layers) = line.get_mut("eventLayers").and_then(Value::as_array_mut) else {
            continue;
        };
        for layer in layers {
            for kind in crate::EventType::ALL {
                let Some(track) = layer.get_mut(kind.key()).and_then(Value::as_array_mut) else {
                    continue;
                };
                if !track
                    .iter()
                    .any(|v| v.get(CURVE_KEY).is_some_and(|v| !v.is_null()))
                {
                    continue;
                }
                let original = std::mem::take(track);
                for (index, raw) in original.iter().enumerate() {
                    if canceled() {
                        return Err(invalid("Custom easing preparation canceled"));
                    }
                    if raw.get(CURVE_KEY).is_none_or(Value::is_null) {
                        track.push(raw.clone());
                        continue;
                    }
                    let event = LineEvent::read(kind, raw)?;
                    let binding = event
                        .custom_easing
                        .as_ref()
                        .ok_or_else(|| invalid("Invalid custom easing event"))?;
                    let mut next = None;
                    for (i, other) in original.iter().enumerate() {
                        if i == index {
                            continue;
                        }
                        if let Some(start) = other.get("startTime") {
                            let start: Beat = serde_json::from_value(start.clone())?;
                            if start >= event.end_time {
                                next = Some(next.map_or(start, |b: Beat| b.min(start)));
                            }
                        }
                    }
                    let mut pieces = super::bake::bake_bound(raw, kind, tempo, next, &canceled)?;
                    total += pieces.len();
                    if total > 200_000 {
                        return Err(invalid(
                            "The project's custom easing projection exceeds 200,000 intervals",
                        ));
                    }
                    if recipes {
                        let signature = signature(kind, &binding.curve, raw, &bpm, &pieces)?;
                        let recipe = Recipe {
                            version: 1,
                            curve: binding.curve.clone(),
                            source: raw.clone(),
                            count: pieces.len(),
                            signature,
                        };
                        pieces[0][RECIPE_KEY] = serde_json::to_value(recipe)?;
                    }
                    track.extend(pieces);
                }
            }
        }
    }
    Ok(())
}

/// Reconstruct only complete, unchanged compiler-owned groups during import.
/// Edited or invalid hints retain their ordinary native events verbatim.
pub(crate) fn restore(root: &mut Value) -> Result<(), ChartError> {
    let bpm = root["BPMList"].clone();
    let Some(lines) = root.get_mut("judgeLineList").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for line in lines {
        let Some(layers) = line.get_mut("eventLayers").and_then(Value::as_array_mut) else {
            continue;
        };
        for layer in layers {
            for kind in EventType::ALL {
                let Some(track) = layer.get_mut(kind.key()).and_then(Value::as_array_mut) else {
                    continue;
                };
                if !track.iter().any(|v| v.get(RECIPE_KEY).is_some()) {
                    continue;
                }
                let original = std::mem::take(track);
                let mut first = 0;
                while first < original.len() {
                    let restored = (|| -> Option<(usize, Value)> {
                        let raw = original[first].get(RECIPE_KEY)?;
                        let count = raw.get("count")?.as_u64()?;
                        if count == 0 || count > crate::tools::MAX_OBJECTS as u64 {
                            return None;
                        }
                        let end = first
                            .checked_add(count as usize)
                            .filter(|n| *n <= original.len())?;
                        let recipe: Recipe = serde_json::from_value(raw.clone()).ok()?;
                        if recipe.version != 1 || recipe.curve.validate().is_err() {
                            return None;
                        }
                        if signature(
                            kind,
                            &recipe.curve,
                            &recipe.source,
                            &bpm,
                            &original[first..end],
                        )
                        .ok()?
                            != recipe.signature
                        {
                            return None;
                        }
                        let mut source = recipe.source;
                        if source.get(CURVE_KEY).is_none() {
                            source[CURVE_KEY] =
                                serde_json::to_value(Binding::new(recipe.curve)).ok()?;
                        }
                        let event = LineEvent::read(kind, &source).ok()?;
                        event.validate(kind).ok()?;
                        source.as_object_mut()?.shift_remove(RECIPE_KEY);
                        Some((end, source))
                    })();
                    if let Some((end, source)) = restored {
                        track.push(source);
                        first = end;
                    } else {
                        track.push(original[first].clone());
                        first += 1;
                    }
                }
            }
        }
    }
    Ok(())
}
