//! Resolve source JSON locations without requiring a valid authoring projection.
use crate::{
    ChartDocument, ChartError, EventId, EventType, NoteId, StoryKind, story_tools::StoryId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceLocation {
    Note(NoteId),
    Event(EventId),
    Story(StoryId),
    Line(usize),
    Bpm,
    Noise(usize),
    Shader(usize),
    Chart,
}

impl ChartDocument {
    pub fn source_location(&self, path: &str) -> Result<SourceLocation, ChartError> {
        if path.len() > 4096
            || (!path.is_empty() && !path.starts_with('/'))
            || self.as_json().pointer(path).is_none()
        {
            return Err(ChartError::Invalid("Source location does not exist".into()));
        }
        let parts: Vec<_> = path.split('/').skip(1).collect();
        let index = |at: usize| parts.get(at).and_then(|p| p.parse::<usize>().ok());
        if parts.first() == Some(&"judgeLineList")
            && let Some(line) = index(1)
        {
            if parts.get(2) == Some(&"notes")
                && let Some(index) = index(3)
            {
                return Ok(SourceLocation::Note(NoteId { line, index }));
            }
            if parts.get(2) == Some(&"eventLayers")
                && let (Some(layer), Some(index)) = (index(3), index(5))
                && let Some(kind) = EventType::ALL
                    .into_iter()
                    .find(|k| Some(&k.key()) == parts.get(4))
            {
                return Ok(SourceLocation::Event(EventId {
                    line,
                    layer,
                    kind,
                    index,
                }));
            }
            if parts.get(2) == Some(&"extended")
                && let Some(index) = index(4)
                && let Some(kind) = StoryKind::EDITABLE
                    .into_iter()
                    .find(|k| Some(&k.key()) == parts.get(3))
            {
                return Ok(SourceLocation::Story(StoryId { line, kind, index }));
            }
            return Ok(SourceLocation::Line(line));
        }
        Ok(match (parts.first().copied(), index(1)) {
            (Some("BPMList"), _) => SourceLocation::Bpm,
            (Some("blockAreaList"), Some(index)) => SourceLocation::Noise(index),
            (Some("effects" | "shaderEvents"), Some(index)) => SourceLocation::Shader(index),
            (Some("phiStudioExtra"), _)
                if parts.get(1) == Some(&"effects") && index(2).is_some() =>
            {
                SourceLocation::Shader(index(2).unwrap())
            }
            _ => SourceLocation::Chart,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_objects_still_have_a_source_location() {
        let document = ChartDocument::from_value(serde_json::json!({
            "META":{}, "BPMList":[{"bpm":120,"startTime":[0,0,1]}],
            "judgeLineList":[{"notes":[],"eventLayers":[{"alphaEvents":[null]}]}]
        }))
        .unwrap();
        assert!(matches!(
            document.source_location("/judgeLineList/0/eventLayers/0/alphaEvents/0"),
            Ok(SourceLocation::Event(_))
        ));
        assert!(document.source_location("/judgeLineList/1").is_err());
    }
}
