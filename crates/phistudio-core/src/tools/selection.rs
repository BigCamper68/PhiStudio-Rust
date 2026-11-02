use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub notes: Vec<NoteId>,
    pub events: Vec<EventId>,
}

impl Selection {
    pub fn len(&self) -> usize {
        self.notes.len() + self.events.len()
    }
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty() && self.events.is_empty()
    }
    pub fn normalized(mut self) -> Self {
        self.notes.sort_unstable();
        self.notes.dedup();
        self.events.sort_unstable();
        self.events.dedup();
        self
    }
    pub fn checked(&self, doc: &ChartDocument) -> Result<Self, ChartError> {
        let result = self.clone().normalized();
        if result.len() > MAX_OBJECTS {
            return Err(invalid("Select at most 20,000 objects per operation"));
        }
        for id in &result.notes {
            doc.note(id.line, id.index)?;
        }
        for id in &result.events {
            doc.event(*id)?;
        }
        Ok(result)
    }
    pub fn all(
        doc: &ChartDocument,
        line: usize,
        layer: usize,
        notes: bool,
        events: bool,
    ) -> Result<Self, ChartError> {
        let mut selected = Self::default();
        if notes {
            selected.notes = doc
                .iter_notes(line)
                .map(|(index, _)| NoteId { line, index })
                .collect();
        }
        if events {
            for kind in EventType::ALL {
                selected
                    .events
                    .extend(
                        doc.events(line, layer, kind)?
                            .into_iter()
                            .map(|(index, _)| EventId {
                                line,
                                layer,
                                kind,
                                index,
                            }),
                    );
            }
        }
        selected.checked(doc)
    }
}
