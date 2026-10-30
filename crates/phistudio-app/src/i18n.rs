//! Editor-owned text only. Data, file paths, shader source and stable IDs stay canonical.
use phistudio_core::Language;
use std::{cell::Cell, collections::HashMap, sync::LazyLock};

thread_local! { static CURRENT: Cell<Language> = const { Cell::new(Language::English) }; }
pub(crate) struct Scope(Language);
impl Scope {
    pub(crate) fn enter(language: Language) -> Self {
        Self(CURRENT.replace(language))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.set(self.0);
    }
}

struct Pattern {
    key: String,
    pieces: Vec<String>,
    slots: Vec<usize>,
}
struct Catalogue {
    translations: HashMap<String, String>,
    patterns: Vec<Pattern>,
}
impl Catalogue {
    fn parse(source: &str) -> Self {
        let translations: HashMap<String, String> =
            serde_json::from_str(source).expect("Validated editor catalogue");
        let mut patterns = Vec::new();
        for key in translations.keys() {
            if let Some((pieces, slots)) = pattern_parts(key)
                && !slots.is_empty()
                && pieces.iter().map(String::len).sum::<usize>() >= 3
                && translations.get(key) != Some(key)
            {
                patterns.push(Pattern {
                    key: key.clone(),
                    pieces,
                    slots,
                });
            }
        }
        // Make equally specific patterns deterministic too.
        patterns.sort_by(|a, b| {
            b.pieces
                .iter()
                .map(String::len)
                .sum::<usize>()
                .cmp(&a.pieces.iter().map(String::len).sum())
                .then_with(|| a.key.cmp(&b.key))
        });
        Self {
            translations,
            patterns,
        }
    }
}
static SIMPLIFIED_CHINESE: LazyLock<Catalogue> =
    LazyLock::new(|| Catalogue::parse(include_str!("../../../assets/locales/zh-CN.json")));
static TRADITIONAL_CHINESE: LazyLock<Catalogue> =
    LazyLock::new(|| Catalogue::parse(include_str!("../../../assets/locales/zh-TW.json")));
static KOREAN: LazyLock<Catalogue> =
    LazyLock::new(|| Catalogue::parse(include_str!("../../../assets/locales/ko.json")));
static RUSSIAN: LazyLock<Catalogue> =
    LazyLock::new(|| Catalogue::parse(include_str!("../../../assets/locales/ru.json")));
static SPANISH: LazyLock<Catalogue> =
    LazyLock::new(|| Catalogue::parse(include_str!("../../../assets/locales/es.json")));
fn catalogue(language: Language) -> Option<&'static Catalogue> {
    match language {
        Language::English => None,
        Language::SimplifiedChinese => Some(&SIMPLIFIED_CHINESE),
        Language::TraditionalChinese => Some(&TRADITIONAL_CHINESE),
        Language::Japanese => Some(&JAPANESE),
        Language::Korean => Some(&KOREAN),
        Language::Russian => Some(&RUSSIAN),
        Language::Spanish => Some(&SPANISH),
    }
}
static JAPANESE: LazyLock<Catalogue> =
    LazyLock::new(|| Catalogue::parse(include_str!("../../../assets/locales/ja.json")));

fn pattern_parts(text: &str) -> Option<(Vec<String>, Vec<usize>)> {
    let mut pieces = Vec::new();
    let mut slots = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = text[cursor..].find('{') {
        let start = cursor + relative;
        if text[start..].starts_with("{{") {
            return None;
        }
        let end = start + text[start..].find('}')?;
        let slot = text[start + 1..end].parse::<usize>().ok()?;
        pieces.push(text[cursor..start].into());
        slots.push(slot);
        cursor = end + 1;
    }
    pieces.push(text[cursor..].into());
    Some((pieces, slots))
}
fn interpolate(template: &str, values: &[String]) -> String {
    let mut result = String::with_capacity(template.len());
    let mut pos = 0;
    while pos < template.len() {
        let rest = &template[pos..];
        if rest.starts_with("{{") {
            result.push('{');
            pos += 2;
            continue;
        }
        if rest.starts_with("}}") {
            result.push('}');
            pos += 2;
            continue;
        }
        if let Some(after) = rest.strip_prefix('{')
            && let Some(end) = after.find('}')
            && let Ok(index) = after[..end].parse::<usize>()
            && let Some(value) = values.get(index)
        {
            result.push_str(value);
            pos += end + 2;
            continue;
        }
        let ch = rest.chars().next().unwrap();
        result.push(ch);
        pos += ch.len_utf8();
    }
    result
}

pub(crate) fn format(key: &str, values: &[String]) -> String {
    let template = catalogue(CURRENT.get())
        .and_then(|catalogue| catalogue.translations.get(key))
        .map_or(key, String::as_str);
    interpolate(template, values)
}
pub(crate) fn tr(text: impl AsRef<str>) -> String {
    let text = text.as_ref();
    let Some(catalogue) = catalogue(CURRENT.get()) else {
        return text.to_owned();
    };
    if let Some(value) = catalogue.translations.get(text) {
        return value.clone();
    }
    // Canonical status/error strings may arrive from workers. Only known,
    // anchored message patterns are translated; substituted values stay verbatim.
    for pattern in &catalogue.patterns {
        if !text.starts_with(&pattern.pieces[0]) {
            continue;
        }
        let mut cursor = pattern.pieces[0].len();
        let mut values = vec![String::new(); pattern.slots.iter().copied().max().unwrap() + 1];
        let mut matched = true;
        for (index, slot) in pattern.slots.iter().enumerate() {
            let next = &pattern.pieces[index + 1];
            let end = if index + 1 == pattern.slots.len() {
                if !text[cursor..].ends_with(next) {
                    matched = false;
                    break;
                }
                text.len() - next.len()
            } else if next.is_empty() {
                matched = false;
                break;
            } else if let Some(at) = text[cursor..].find(next) {
                cursor + at
            } else {
                matched = false;
                break;
            };
            values[*slot] = text[cursor..end].into();
            cursor = end + next.len();
        }
        if matched && cursor == text.len() {
            return format(&pattern.key, &values);
        }
    }
    text.to_owned()
}

pub(crate) fn window<'a>(title: impl AsRef<str>) -> crate::window::Window<'a> {
    let title = title.as_ref();
    crate::window::Window::new(tr(title), egui::Id::new(title))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn catalogues_have_equal_keys_placeholders_and_supported_glyphs() {
        let english: BTreeMap<String, String> =
            serde_json::from_str(include_str!("../../../assets/locales/en.json")).unwrap();
        let mut glyphs = BTreeSet::new();
        for language in Language::ALL {
            let Some(catalogue) = catalogue(language) else {
                continue;
            };
            assert_eq!(
                english.keys().collect::<BTreeSet<_>>(),
                catalogue.translations.keys().collect::<BTreeSet<_>>(),
                "Incomplete {} catalogue",
                language.code()
            );
            for name in crate::authoring_ui::EASINGS
                .into_iter()
                .chain(["Bezier", "Cubic Bezier"])
            {
                assert_eq!(
                    catalogue.translations.get(name).map(String::as_str),
                    Some(name),
                    "Easing names stay English in {}: {name}",
                    language.code()
                );
            }
            for (key, translation) in &catalogue.translations {
                assert_eq!(&english[key], key, "English is the canonical source");
                assert!(!translation.is_empty(), "Empty translation for {key}");
                let slots = |s: &str| {
                    let mut result = pattern_parts(s).map(|(_, slots)| slots).unwrap_or_default();
                    result.sort_unstable();
                    result
                };
                assert_eq!(
                    slots(key),
                    slots(translation),
                    "Placeholder mismatch: {key}"
                );
                glyphs.extend(translation.chars().filter(|c| !c.is_whitespace()));
            }
            glyphs.extend(language.native_name().chars());
        }
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::definitions());
        let _ = ctx.run(Default::default(), |ctx| {
            ctx.fonts(|fonts| {
                let font = egui::FontId::proportional(16.);
                for glyph in &glyphs {
                    assert!(fonts.has_glyph(&font, *glyph), "Missing UI glyph {glyph}");
                }
            });
        });
    }

    #[test]
    fn translation_scopes_restore_language_and_leave_substitutions_opaque() {
        assert_eq!(tr("Save"), "Save");
        {
            let _language = Scope::enter(Language::SimplifiedChinese);
            assert_eq!(tr("Save"), "保存");
            assert_eq!(tr("Source ↗"), "源码 ↗");
            let data = "Settings / 中文 / 😀 / {0}.glsl";
            assert_eq!(
                format("Audio playback failed: {0}", &[data.into()]),
                format!("音频播放失败：{data}")
            );
            assert_eq!(tr("Audio playback failed: Save"), "音频播放失败：Save");
            assert_eq!(tr(data), data);
            assert_eq!(
                interpolate("{{{0}}} {1}", &["{1}".into(), "Save".into()]),
                "{{1}} Save"
            );
            {
                let _english = Scope::enter(Language::English);
                assert_eq!(tr("Save"), "Save");
            }
            assert_eq!(tr("Settings"), "设置");
            assert_eq!(std::thread::spawn(|| tr("Save")).join().unwrap(), "Save");
        }
        assert_eq!(tr("Save"), "Save");
    }

    #[test]
    fn clipboard_statuses_translate_without_an_english_verb_substitution() {
        for language in Language::ALL {
            let _language = Scope::enter(language);
            for verb in ["Copied", "Cut"] {
                let key = std::format!("{verb} {{0}} objects");
                let rendered = format(&key, &["3".into()]);
                assert_eq!(tr(std::format!("{verb} 3 objects")), rendered);
                if language != Language::English {
                    assert_ne!(rendered, std::format!("{verb} 3 objects"));
                }
            }
        }
    }

    #[test]
    fn translated_window_retains_its_egui_identity() {
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::definitions());
        let mut ids = Vec::new();
        for language in Language::ALL {
            let _language = Scope::enter(language);
            let _ = ctx.run(Default::default(), |ctx| {
                ids.push(
                    window("Settings")
                        .show(ctx, |ui| ui.label(tr("Language")))
                        .unwrap()
                        .response
                        .id,
                );
            });
        }
        assert!(ids.iter().all(|id| *id == ids[0]));
    }
}
