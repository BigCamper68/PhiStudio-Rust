use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ChartClipboard {
    items: Vec<Object>,
    pub first: Beat,
    pub last: Beat,
    pub(crate) first_line: usize,
    pub(crate) first_layer: usize,
}

impl ChartClipboard {
    pub(crate) fn preview(&self) -> Vec<[f64; 4]> {
        self.items
            .iter()
            .take(200)
            .filter_map(|i| {
                Some([
                    i.start().ok()?.as_f64(),
                    i.extent_end().ok()?.as_f64(),
                    if let Some((_, kind)) = i.event {
                        kind as usize as f64 * 200. - 400.
                    } else {
                        i.value["positionX"].as_f64().unwrap_or(0.)
                    },
                    if i.event.is_some() { 1. } else { 0. },
                ])
            })
            .collect()
    }
    pub(crate) fn last_line(&self) -> usize {
        self.items
            .iter()
            .map(|i| i.line)
            .max()
            .unwrap_or(self.first_line)
    }
    pub(crate) fn custom_hitsounds(&self) -> Result<BTreeSet<String>, ChartError> {
        self.items
            .iter()
            .filter(|i| i.event.is_none())
            .flat_map(|i| {
                [
                    i.value.get("hitsound").and_then(Value::as_str),
                    i.value
                        .get(crate::SOUND_GAIN_KEY)
                        .and_then(|g| g.get("source"))
                        .and_then(Value::as_str),
                ]
                .into_iter()
                .flatten()
            })
            .filter(|s| !crate::builtin_hitsound(s))
            .map(crate::validate_hitsound)
            .collect()
    }
    pub(crate) fn remap_hitsounds(&mut self, names: &BTreeMap<String, String>) {
        fn remap(value: &mut Value, names: &BTreeMap<String, String>) {
            if let Some(old) = value.as_str()
                && let Ok(key) = crate::validate_hitsound(old)
                && let Some(name) = names.get(&key)
            {
                *value = name.clone().into();
            }
        }
        for item in &mut self.items {
            if item.event.is_none() {
                if let Some(v) = item.value.get_mut("hitsound") {
                    remap(v, names);
                }
                if let Some(gain) = item.value.get_mut(crate::SOUND_GAIN_KEY) {
                    for key in ["source", "rendered"] {
                        if let Some(v) = gain.get_mut(key) {
                            remap(v, names);
                        }
                    }
                }
            }
        }
    }
    pub(crate) fn configure(&mut self, factor: Beat, offset: [f64; 2]) -> Result<(), ChartError> {
        for item in &mut self.items {
            item.with_times(
                self.first
                    .checked_add(item.start()?.checked_sub(self.first)?.checked_mul(factor)?)?,
                self.first
                    .checked_add(item.end()?.checked_sub(self.first)?.checked_mul(factor)?)?,
            )?;
            if let Some((_, kind)) = item.event {
                let shift = match kind {
                    EventType::MoveX => offset[0],
                    EventType::MoveY => offset[1],
                    _ => 0.,
                };
                if shift != 0. {
                    let mut event = LineEvent::read(kind, &item.value)?;
                    event.start += shift;
                    event.end += shift;
                    item.value = event.merge(kind, Some(&item.value))?;
                }
            } else if offset[0] != 0. {
                let mut note: Note = serde_json::from_value(item.value.clone())?;
                note.x += offset[0];
                item.value = note.merge_into(Some(&item.value))?;
            }
        }
        self.last = self
            .first
            .checked_add(self.last.checked_sub(self.first)?.checked_mul(factor)?)?;
        Ok(())
    }
    pub fn copy(doc: &ChartDocument, selection: &Selection) -> Result<Self, ChartError> {
        let items = objects(doc, selection)?;
        let (first, last) = bounds(&items)?;
        let first_line = items.iter().map(|i| i.line).min().unwrap();
        let first_layer = items
            .iter()
            .filter_map(|i| i.event.map(|e| e.0))
            .min()
            .unwrap_or(0);
        Ok(Self {
            items,
            first,
            last,
            first_line,
            first_layer,
        })
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn paste(
        &self,
        doc: &ChartDocument,
        line: usize,
        layer: usize,
        beat: Beat,
        mirrored: bool,
    ) -> Result<ToolEdit, ChartError> {
        let delta = beat.checked_sub(self.first)?;
        let mut items = self.items.clone();
        for item in &mut items {
            item.line = item
                .line
                .checked_sub(self.first_line)
                .and_then(|offset| line.checked_add(offset))
                .ok_or_else(|| invalid("Invalid destination line"))?;
            item.with_times(
                item.start()?.checked_add(delta)?,
                item.end()?.checked_add(delta)?,
            )?;
            if let Some((old, kind)) = item.event {
                item.event = Some((
                    old.checked_sub(self.first_layer)
                        .and_then(|offset| layer.checked_add(offset))
                        .ok_or_else(|| invalid("Invalid destination layer"))?,
                    kind,
                ));
            }
            if mirrored {
                mirror(item, 0.)?;
            }
        }
        plan(doc, &Selection::default(), items, "Paste selection")
    }
}
