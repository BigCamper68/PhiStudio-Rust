//! Mixed note/event/Storyboard clipboard and atomic sparse transactions.
use crate::{
    Beat, ChartDocument, ChartError, Editor,
    story_tools::{self, StoryClipboard, StoryEdit, StoryId},
    tools::{self, ChartClipboard, Selection, ToolEdit, Transform},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObjectClipboard {
    pub normal: Option<ChartClipboard>,
    pub story: Option<StoryClipboard>,
    pub first: Beat,
    pub last: Beat,
    pub origin_line: usize,
    pub origin_layer: usize,
}
impl ObjectClipboard {
    pub fn preview(&self) -> Vec<[f64; 4]> {
        let mut points = self.normal.as_ref().map_or_else(Vec::new, |n| n.preview());
        if let Some(story) = &self.story {
            for o in story.objects.iter().take(200 - points.len()) {
                if let Ok(event) = serde_json::from_value::<crate::StoryEvent>(o.value.clone()) {
                    points.push([
                        event.start_time.as_f64(),
                        event.end_time.as_f64(),
                        o.kind as usize as f64 * 150. - 450.,
                        2.,
                    ]);
                }
            }
        }
        points
    }
    pub fn required_lines(&self) -> usize {
        self.normal
            .as_ref()
            .map(|c| c.last_line())
            .into_iter()
            .chain(
                self.story
                    .iter()
                    .flat_map(|c| c.objects.iter().map(|o| o.line)),
            )
            .max()
            .unwrap_or(self.origin_line)
            .checked_sub(self.origin_line)
            .and_then(|v| v.checked_add(1))
            .unwrap_or(usize::MAX)
    }
    pub fn custom_hitsounds(&self) -> Result<std::collections::BTreeSet<String>, ChartError> {
        self.normal
            .as_ref()
            .map_or_else(|| Ok(Default::default()), |c| c.custom_hitsounds())
    }
    pub fn remap_hitsounds(&mut self, names: &std::collections::BTreeMap<String, String>) {
        if let Some(c) = &mut self.normal {
            c.remap_hitsounds(names);
        }
    }
    pub fn configured(&self, factor: Beat, offset: [f64; 2]) -> Result<Self, ChartError> {
        if factor <= Beat::ZERO || offset.iter().any(|v| !v.is_finite()) {
            return Err(ChartError::Invalid(
                "Use a positive time scale and finite offsets".into(),
            ));
        }
        let mut result = self.clone();
        if let Some(normal) = &mut result.normal {
            normal.configure(factor, offset)?;
        }
        if let Some(story) = &mut result.story {
            for object in &mut story.objects {
                let mut event: crate::StoryEvent = serde_json::from_value(object.value.clone())?;
                event.start_time = self.first.checked_add(
                    event
                        .start_time
                        .checked_sub(self.first)?
                        .checked_mul(factor)?,
                )?;
                event.end_time = self.first.checked_add(
                    event
                        .end_time
                        .checked_sub(self.first)?
                        .checked_mul(factor)?,
                )?;
                object.value = event.merge(object.kind, Some(&object.value))?;
            }
            story.last = self
                .first
                .checked_add(story.last.checked_sub(self.first)?.checked_mul(factor)?)?;
        }
        result.last = self
            .first
            .checked_add(self.last.checked_sub(self.first)?.checked_mul(factor)?)?;
        Ok(result)
    }
    pub fn copy(
        doc: &ChartDocument,
        normal: &Selection,
        stories: &[StoryId],
    ) -> Result<Self, ChartError> {
        let mut normal = if normal.is_empty() {
            None
        } else {
            Some(ChartClipboard::copy(doc, normal)?)
        };
        let mut story = if stories.is_empty() {
            None
        } else {
            Some(StoryClipboard::copy(doc, stories)?)
        };
        let first = normal
            .as_ref()
            .map(|c| c.first)
            .into_iter()
            .chain(story.as_ref().map(|c| c.first))
            .min()
            .ok_or_else(|| ChartError::Invalid("Select objects first".into()))?;
        let last = normal
            .as_ref()
            .map(|c| c.last)
            .into_iter()
            .chain(story.as_ref().map(|c| c.last))
            .max()
            .unwrap();
        let origin_line = normal
            .as_ref()
            .map(|c| c.first_line)
            .into_iter()
            .chain(story.as_ref().map(|c| c.origin_line))
            .min()
            .unwrap();
        let origin_layer = normal.as_ref().map_or(0, |c| c.first_layer);
        if let Some(c) = &mut normal {
            c.first = first;
            c.first_line = origin_line;
        }
        if let Some(c) = &mut story {
            c.first = first;
            c.origin_line = origin_line;
        }
        let result = Self {
            normal,
            story,
            first,
            last,
            origin_line,
            origin_layer,
        };
        if result.len() > tools::MAX_OBJECTS {
            return Err(ChartError::Invalid("Select at most 20,000 objects".into()));
        }
        Ok(result)
    }
    pub fn len(&self) -> usize {
        self.normal.as_ref().map_or(0, |c| c.len())
            + self.story.as_ref().map_or(0, |c| c.objects.len())
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn paste(
        &self,
        doc: &ChartDocument,
        line: usize,
        layer: usize,
        beat: Beat,
        mirrored: bool,
    ) -> Result<ObjectEdit, ChartError> {
        if self.is_empty() || self.len() > tools::MAX_OBJECTS {
            return Err(ChartError::Invalid(
                "Pattern must contain 1–20,000 objects".into(),
            ));
        }
        Ok(ObjectEdit {
            normal: self
                .normal
                .as_ref()
                .map(|c| c.paste(doc, line, layer, beat, mirrored))
                .transpose()?,
            story: self
                .story
                .as_ref()
                .map(|c| c.paste(doc, line, beat))
                .transpose()?,
            label: "Paste objects",
        })
    }
}
pub struct ObjectEdit {
    pub normal: Option<ToolEdit>,
    pub story: Option<StoryEdit>,
    pub label: &'static str,
}
#[derive(Default, Debug)]
pub struct ObjectSelection {
    pub normal: Selection,
    pub story: Vec<StoryId>,
}
impl ObjectEdit {
    pub fn apply(self, editor: &mut Editor) -> Result<Option<ObjectSelection>, ChartError> {
        editor.execute_objects(self)
    }
}
pub fn transform(
    doc: &ChartDocument,
    normal: &Selection,
    stories: &[StoryId],
    operation: Transform,
) -> Result<ObjectEdit, ChartError> {
    let clipboard = ObjectClipboard::copy(doc, normal, stories)?;
    let bounds = Some((clipboard.first, clipboard.last));
    let story_applies = matches!(
        operation,
        Transform::Shift { .. }
            | Transform::ScaleTime(_)
            | Transform::ReverseTime
            | Transform::Quantize(_)
            | Transform::AlignTime
            | Transform::ReverseValues
            | Transform::Easing(_)
            | Transform::Curve { .. }
    );
    Ok(ObjectEdit {
        normal: if normal.is_empty() {
            None
        } else {
            Some(tools::transform_bounds(
                doc,
                normal,
                operation.clone(),
                bounds,
            )?)
        },
        story: if stories.is_empty() || !story_applies {
            None
        } else {
            Some(story_tools::transform_bounds(
                doc, stories, operation, bounds,
            )?)
        },
        label: "Transform objects",
    })
}
pub fn delete(
    doc: &ChartDocument,
    normal: &Selection,
    stories: &[StoryId],
) -> Result<ObjectEdit, ChartError> {
    Ok(ObjectEdit {
        normal: if normal.is_empty() {
            None
        } else {
            Some(tools::delete(doc, normal)?)
        },
        story: if stories.is_empty() {
            None
        } else {
            Some(story_tools::delete(doc, stories)?)
        },
        label: "Delete objects",
    })
}
