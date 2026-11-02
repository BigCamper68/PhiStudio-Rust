//! A borrowed timeline view overlays only staged edits, never copies whole tracks.
use super::*;
use std::borrow::Cow;

pub(super) struct Rows<'a> {
    app: &'a Workbench,
    name: Option<&'a str>,
}
impl<'a> Rows<'a> {
    pub fn new(app: &'a Workbench, name: Option<&'a str>) -> Self {
        Self { app, name }
    }
    pub fn iter(&self) -> impl Iterator<Item = (usize, Cow<'a, Value>)> {
        let name = self.name;
        let app = self.app;
        let edit = match &app.timeline.drag {
            Some(Gesture::Shader(d)) => match &**d {
                ShaderGesture::Edit(d) => Some(&**d),
                _ => None,
            },
            _ => None,
        };
        let primary = edit.filter(|d| d.keyframe.as_ref().map(|(name, _)| name.as_str()) == name);
        let source = rows_for(app, name);
        let created = primary.filter(|d| d.creating).map(|d| &d.current);
        // Groups retain source order within each parameter; merge in one pass.
        let mut moving = edit
            .filter(|d| d.group.len() > 1)
            .into_iter()
            .flat_map(|d| d.group.iter())
            .filter(move |(id, _)| id.parameter.as_deref() == name)
            .peekable();
        source
            .iter()
            .chain(created)
            .enumerate()
            .map(move |(index, source)| {
                if moving.peek().is_some_and(|(id, _)| id.index == index)
                    && let Some((id, raw)) = moving.next()
                    && let Some(edit) = edit
                {
                    return (index, Cow::Owned(selection::shifted(edit, id, raw)));
                }
                if let Some(d) = primary
                    && d.keyframe.as_ref().map_or(d.effect, |(_, index)| *index) == index
                {
                    return (index, Cow::Borrowed(&d.current));
                }
                if let Some(a) = &app.shaders.animation
                    && name == Some(a.name.as_str())
                    && index == a.index
                {
                    return (index, Cow::Borrowed(&a.raw));
                }
                (index, Cow::Borrowed(source))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phistudio_core::ChartDocument;
    #[test]
    fn ordinary_and_staged_rows_borrow_source_without_materializing_an_override() {
        let mut app = Workbench::new(ChartDocument::empty("Borrowed rows", 120.).unwrap(), None);
        let key = json!({"startTime":[0,0,1],"endTime":[1,0,1],"start":0.,"end":1.,"easingType":1,"custom":"retained"});
        let effect = json!({"start":[0,0,1],"end":[4,0,1],"shader":"chromatic","vars":{"power":[key.clone(),key]}});
        app.shaders.source = json!({"bpm":120.,"effects":[effect.clone()]});
        app.shaders.draft = Some(EffectDraft {
            index: 0,
            raw: effect,
            start: "0".into(),
            end: "4".into(),
        });
        let before = app.shaders.source.clone();
        assert!(
            Rows::new(&app, None)
                .iter()
                .all(|(_, row)| matches!(row, Cow::Borrowed(_)))
        );
        app.shaders.animation =
            animation_draft(&app.shaders.draft.as_ref().unwrap().raw, "power", 1);
        app.shaders.animation.as_mut().unwrap().raw["end"] = json!(2.);
        let view = Rows::new(&app, Some("power"));
        let rows = view.iter().collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].1["end"], 1.);
        assert_eq!(rows[1].1["end"], 2.);
        assert!(rows.iter().all(|(_, row)| matches!(row, Cow::Borrowed(_))));
        assert_eq!(app.shaders.source, before);
        assert!(!app.editor.is_dirty());
    }
}
