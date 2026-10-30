//! Package/conversion provenance is applied as data to the original prpr model.
//! Anim, keyframe interpolation, note evaluation and rendering remain upstream.
use anyhow::Result;
use phistudio_core::{AttachedUi, ChartDocument, pec_height_signature, rotation_inheritance};
use prpr::core::{AnimFloat, Chart, JudgeLineCache, Keyframe, NoteKind};

pub(crate) fn render_source(document: &ChartDocument) -> Result<String> {
    let mut value = document.render_value()?;
    // The unchanged RPE deserializer requires an i32 JSON token. Old editor
    // files used floats even for 150.0. Fractional precision is restored below
    // on the native Chart; only this parser copy uses whole milliseconds.
    if let Some(ms) = value["META"]["offset"]
        .as_f64()
        .filter(|ms| (i32::MIN as f64..=i32::MAX as f64).contains(ms))
    {
        value["META"]["offset"] = serde_json::json!(ms as i32);
    }
    // Java accepts numeric rotation flags and several spellings of HUD names.
    // Normalize the renderer's copy; preserve authored data in the document.
    if let Some(lines) = value["judgeLineList"].as_array_mut() {
        for line in lines {
            if let Some(flag) = line.get("rotateWithFather") {
                line["rotateWithFather"] = serde_json::json!(rotation_inheritance(flag));
            }
            if let Some(binding) = AttachedUi::parse(&line["attachUI"]) {
                line["attachUI"] = serde_json::json!(binding.key());
            }
        }
    }
    if value["META"]["phiPecAlphaExtension"].as_bool() == Some(true)
        && let Some(lines) = value["judgeLineList"].as_array_mut()
    {
        for line in lines {
            if let Some(layers) = line["eventLayers"].as_array_mut() {
                for layer in layers {
                    if let Some(events) = layer["alphaEvents"].as_array_mut() {
                        for event in events {
                            for key in ["start", "end"] {
                                if let Some(n) = event[key].as_f64().filter(|n| *n < 0.) {
                                    event[key] = serde_json::json!(n * 255.);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(serde_json::to_string(&value)?)
}
pub(crate) fn apply(document: &ChartDocument, chart: &mut Chart) {
    let meta = &document.as_json()["META"];
    if meta["offset"].as_f64().is_some_and(|ms| ms.fract() != 0.) {
        chart.offset = document.offset_seconds() as f32;
    }
    chart.settings.pe_alpha_extension = meta["phiPecAlphaExtension"].as_bool().unwrap_or(false);
    let Some(signature) = meta["phiPecHeights"]["signature"].as_str() else {
        return;
    };
    if pec_height_signature(document.as_json()).as_deref() != Some(signature) {
        return;
    }
    let Some(lines) = meta["phiPecHeights"]["lines"]
        .as_array()
        .filter(|v| v.len() == chart.lines.len())
    else {
        return;
    };
    for (line, source) in chart.lines.iter_mut().zip(lines) {
        let Some(frames) = source.as_array().filter(|f| f.len() >= 2) else {
            continue;
        };
        let keyframes: Option<Vec<_>> = frames
            .iter()
            .enumerate()
            .map(|(i, frame)| {
                let frame = frame.as_array().filter(|v| v.len() == 2)?;
                let time = frame[0].as_f64()?;
                let height = frame[1].as_f64()?;
                if !time.is_finite() || !height.is_finite() || height as f32 as f64 != height {
                    return None;
                }
                Some(Keyframe::new(
                    time,
                    height as f32,
                    if i + 1 == frames.len() { 0 } else { 2 },
                ))
            })
            .collect();
        let Some(keyframes) = keyframes else {
            continue;
        };
        line.height = AnimFloat::new(keyframes);
        for note in &mut line.notes {
            line.height.set_time(note.time);
            note.height = line.height.now() as f64;
            if let NoteKind::Hold {
                end_time,
                end_height,
            } = &mut note.kind
            {
                line.height.set_time(*end_time);
                *end_height = line.height.now() as f64;
            }
        }
        line.height.set_time(0.);
        line.cache = JudgeLineCache::new(&mut line.notes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prpr::core::ChartExtra;
    #[test]
    fn deleting_last_events_leaves_each_line_parseable_by_original_prpr() {
        let mut editor =
            phistudio_core::Editor::new(ChartDocument::empty("Empty tracks", 120.).unwrap());
        for kind in phistudio_core::EventType::ALL {
            editor
                .execute(phistudio_core::Command::DeleteEvent(
                    phistudio_core::EventId {
                        line: 0,
                        layer: 0,
                        kind,
                        index: 0,
                    },
                ))
                .unwrap();
            let mut fs = crate::assets::ChartAssets::new(None).unwrap();
            let source = render_source(editor.document()).unwrap();
            let chart = pollster::block_on(prpr::parse::parse_rpe(
                &source,
                &mut fs,
                ChartExtra::default(),
                false,
            ))
            .unwrap();
            assert_eq!(chart.lines.len(), 1);
        }
    }
    #[test]
    fn java_hud_aliases_and_numeric_rotation_parse_without_changing_source() {
        for alias in ["comboNumber", " COMBO-NUMBER ", "combo_number"] {
            let mut raw = ChartDocument::empty("Bindings", 120.)
                .unwrap()
                .as_json()
                .clone();
            raw["judgeLineList"][0]["attachUI"] = serde_json::json!(alias);
            raw["judgeLineList"][0]["rotateWithFather"] = serde_json::json!(1);
            let document = ChartDocument::from_value(raw.clone()).unwrap();
            let mut fs = crate::assets::ChartAssets::new(None).unwrap();
            let source = render_source(&document).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&source).unwrap();
            assert_eq!(parsed["judgeLineList"][0]["attachUI"], "combonumber");
            assert_eq!(parsed["judgeLineList"][0]["rotateWithFather"], true);
            let chart = pollster::block_on(prpr::parse::parse_rpe(
                &source,
                &mut fs,
                ChartExtra::default(),
                false,
            ))
            .unwrap();
            assert_eq!(chart.lines.len(), 1);
            assert_eq!(document.as_json(), &raw);
        }
    }
    fn converted(source: &str) -> (ChartDocument, Chart) {
        let document = phistudio_core::decode_chart(source).unwrap().document;
        let mut fs = crate::assets::ChartAssets::new(None).unwrap();
        let mut chart = pollster::block_on(prpr::parse::parse_rpe(
            &render_source(&document).unwrap(),
            &mut fs,
            ChartExtra::default(),
            false,
        ))
        .unwrap();
        apply(&document, &mut chart);
        (document, chart)
    }
    #[test]
    fn converted_pec_uses_original_prpr_alpha_and_floor_animation() {
        let source = "150\nbp 0 120\nbp 8 180\ncp 0 0 1024 700\ncd 0 0 0\nca 0 0 -2\ncv 0 0 7\ncv 0 4 1.17\nn1 0 1 512 1 0\nn2 0 3 6 -256 1 0\n";
        let (_, mut imported) = converted(source);
        let mut native = prpr::parse::parse_pec(source, ChartExtra::default()).unwrap();
        assert!(imported.settings.pe_alpha_extension);
        for time in [0., 0.1, 0.5, 1., 1.5, 2., 2.7, 3.] {
            let a = &mut imported.lines[0];
            let b = &mut native.lines[0];
            a.height.set_time(time);
            b.height.set_time(time);
            assert_eq!(a.height.now(), b.height.now(), "Floor at {time}");
            a.object.alpha.set_time(time);
            b.object.alpha.set_time(time);
            assert_eq!(
                a.object.alpha.now(),
                b.object.alpha.now(),
                "Alpha at {time}"
            );
        }
        let mut a: Vec<_> = imported.lines[0]
            .notes
            .iter()
            .map(|n| (n.time, n.height))
            .collect();
        let mut b: Vec<_> = native.lines[0]
            .notes
            .iter()
            .map(|n| (n.time, n.height))
            .collect();
        a.sort_by(|a, b| a.0.total_cmp(&b.0));
        b.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(a, b);
    }
    #[test]
    fn stale_floor_provenance_is_not_applied_after_a_timing_edit() {
        let source =
            "150\nbp 0 120\ncp 0 0 1024 700\ncd 0 0 0\nca 0 0 255\ncv 0 0 7\nn1 0 1 0 1 0\n";
        let (document, _) = converted(source);
        let mut value = document.as_json().clone();
        value["judgeLineList"][0]["notes"][0]["startTime"] = serde_json::json!([2, 0, 1]);
        value["META"]["phiPecHeights"]["lines"][0] = serde_json::json!([[0., 999.], [10., 999.]]);
        let document = ChartDocument::from_value(value).unwrap();
        let mut fs = crate::assets::ChartAssets::new(None).unwrap();
        let mut chart = pollster::block_on(prpr::parse::parse_rpe(
            &render_source(&document).unwrap(),
            &mut fs,
            ChartExtra::default(),
            false,
        ))
        .unwrap();
        let height = chart.lines[0].notes[0].height;
        apply(&document, &mut chart);
        assert_eq!(chart.lines[0].notes[0].height, height);
        assert_ne!(height, 999.);
    }
}
