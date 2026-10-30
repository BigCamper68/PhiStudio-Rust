use super::*;
pub(super) fn animation_draft(effect: &Value, name: &str, index: usize) -> Option<AnimationDraft> {
    let raw = effect["vars"][name].as_array()?.get(index)?.clone();
    let start: Beat = serde_json::from_value(raw["startTime"].clone()).ok()?;
    let end: Beat = serde_json::from_value(raw["endTime"].clone()).ok()?;
    let curve = LineEvent {
        start_time: start,
        end_time: end,
        start: 0.,
        end: 1.,
        easing_type: raw["easingType"].as_i64().unwrap_or(1) as i32,
        easing_left: raw["easingLeft"].as_f64().unwrap_or(0.),
        easing_right: raw["easingRight"].as_f64().unwrap_or(1.),
        ..Default::default()
    };
    Some(AnimationDraft {
        name: name.into(),
        index,
        raw,
        start: start.to_string(),
        end: end.to_string(),
        curve,
    })
}
