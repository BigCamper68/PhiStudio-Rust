//! Reversible edits store one changed element and a permutation, not snapshots.
//! Materialized missing/null containers are restored exactly on undo.
use crate::{Beat, ChartError};
use serde_json::Value;

#[derive(Clone, Debug)]
pub(crate) enum Segment {
    Field(&'static str),
    Index(usize),
}
use Segment::*;

#[derive(Debug)]
enum Restore {
    Value {
        path: Vec<Segment>,
        before: Option<Value>,
    },
    Length {
        path: Vec<Segment>,
        length: usize,
    },
}

#[derive(Debug)]
pub(crate) struct ArrayEdit {
    path: Vec<Segment>,
    index: usize,
    before: Option<Value>,
    after: Option<Value>,
    order: Option<(Vec<usize>, Vec<usize>)>,
    restore: Vec<Restore>,
    omit_empty: bool,
    pub after_index: usize,
}

fn ensure<'a>(root: &'a mut Value, path: &[Segment]) -> &'a mut Value {
    if path.is_empty() {
        return root;
    }
    let next = match path[0] {
        Field(key) => {
            if root.is_null() {
                *root = Value::Object(Default::default());
            }
            root.as_object_mut()
                .unwrap()
                .entry(key)
                .or_insert(Value::Null)
        }
        Index(index) => {
            if root.is_null() {
                *root = Value::Array(Vec::new());
            }
            let list = root.as_array_mut().unwrap();
            list.resize_with(list.len().max(index + 1), || Value::Null);
            &mut list[index]
        }
    };
    ensure(next, &path[1..])
}
fn set(root: &mut Value, path: &[Segment], value: Option<Value>) {
    let parent = ensure(root, &path[..path.len() - 1]);
    match path[path.len() - 1] {
        Field(key) => {
            let object = parent.as_object_mut().unwrap();
            if let Some(value) = value {
                object.insert(key.into(), value);
            } else {
                object.shift_remove(key);
            }
        }
        Index(index) => parent.as_array_mut().unwrap()[index] = value.unwrap_or(Value::Null),
    }
}
fn permute(values: &mut Vec<Value>, order: &[usize]) {
    let mut source = std::mem::take(values);
    *values = order.iter().map(|&i| source[i].take()).collect();
}

impl ArrayEdit {
    pub fn prepare(
        root: &Value,
        path: Vec<Segment>,
        index: usize,
        replacing: bool,
        after: Option<Value>,
        sort: bool,
        omit_empty: bool,
    ) -> Result<Self, ChartError> {
        let bad = || ChartError::Invalid("Invalid event/BPM container".into());
        let mut restore = Vec::new();
        let mut current = Some(root);
        for (i, key) in path.iter().enumerate() {
            let Some(value) = current else {
                break;
            };
            if value.is_null() {
                restore.push(Restore::Value {
                    path: path[..i].to_vec(),
                    before: Some(Value::Null),
                });
                current = None;
                break;
            }
            current = match key {
                Field(key) => {
                    let object = value.as_object().ok_or_else(bad)?;
                    if !object.contains_key(*key) {
                        restore.push(Restore::Value {
                            path: path[..=i].to_vec(),
                            before: None,
                        });
                    }
                    object.get(*key)
                }
                Index(index) => {
                    let list = value.as_array().ok_or_else(bad)?;
                    if *index >= list.len() {
                        restore.push(Restore::Length {
                            path: path[..i].to_vec(),
                            length: list.len(),
                        });
                    }
                    list.get(*index)
                }
            };
        }
        let values = match current {
            Some(Value::Array(list)) => list.as_slice(),
            Some(Value::Null) => {
                restore.push(Restore::Value {
                    path: path.clone(),
                    before: Some(Value::Null),
                });
                &[]
            }
            None => &[],
            _ => return Err(bad()),
        };
        let before = if replacing {
            Some(values.get(index).ok_or_else(bad)?.clone())
        } else {
            if index > values.len() {
                return Err(bad());
            }
            None
        };
        let mut edit = Self {
            path,
            index,
            before,
            after,
            order: None,
            restore,
            omit_empty,
            after_index: index,
        };
        if sort {
            let mut items: Vec<&Value> = values.iter().collect();
            match (&edit.before, &edit.after) {
                (None, Some(value)) => items.insert(index, value),
                (Some(_), Some(value)) => items[index] = value,
                (Some(_), None) => {
                    items.remove(index);
                }
                _ => return Err(bad()),
            }
            let keys: Vec<Beat> = items
                .iter()
                .map(|v| serde_json::from_value(v["startTime"].clone()).map_err(ChartError::from))
                .collect::<Result<_, _>>()?;
            let mut order: Vec<usize> = (0..items.len()).collect();
            order.sort_by_key(|&i| keys[i]);
            let mut inverse = vec![0; order.len()];
            for (new, &old) in order.iter().enumerate() {
                inverse[old] = new;
            }
            if edit.after.is_some() {
                edit.after_index = inverse[index];
            }
            if order.iter().enumerate().any(|(i, &v)| i != v) {
                edit.order = Some((order, inverse));
            }
        }
        Ok(edit)
    }
    pub fn apply(&self, root: &mut Value, forward: bool) {
        let value = ensure(root, &self.path);
        if value.is_null() {
            *value = Value::Array(Vec::new());
        }
        let values = value.as_array_mut().unwrap();
        if !forward && let Some((_, inverse)) = &self.order {
            permute(values, inverse);
        }
        let (before, after) = if forward {
            (&self.before, &self.after)
        } else {
            (&self.after, &self.before)
        };
        match (before, after) {
            (None, Some(v)) => values.insert(self.index, v.clone()),
            (Some(_), Some(v)) => values[self.index] = v.clone(),
            (Some(_), None) => {
                values.remove(self.index);
            }
            _ => unreachable!(),
        }
        if forward && let Some((order, _)) = &self.order {
            permute(values, order);
        }
        if forward && self.omit_empty && values.is_empty() {
            set(root, &self.path, None);
        }
        if !forward {
            for restore in self.restore.iter().rev() {
                match restore {
                    Restore::Value { path, before } => set(root, path, before.clone()),
                    Restore::Length { path, length } => {
                        ensure(root, path).as_array_mut().unwrap().truncate(*length)
                    }
                }
            }
        }
    }
}

/// One track transaction: changed values plus one final permutation. Work and
/// memory are linear in the track size, rather than sorting once per insertion.
#[derive(Debug)]
pub(crate) struct ArrayBatch {
    path: Vec<Segment>,
    removed: Vec<(usize, Value)>,
    added: Vec<Value>,
    order: Option<(Vec<usize>, Vec<usize>)>,
    restore: Vec<Restore>,
    omit_empty: bool,
    pub added_indices: Vec<usize>,
}
impl ArrayBatch {
    pub fn prepare(
        root: &Value,
        path: Vec<Segment>,
        remove: &[usize],
        added: Vec<Value>,
        sort: bool,
        omit_empty: bool,
    ) -> Result<Self, ChartError> {
        let bad = || ChartError::Invalid("Invalid batch track or selection".into());
        let first = if let Some(index) = remove.first() {
            ArrayEdit::prepare(root, path.clone(), *index, true, None, false, omit_empty)?
        } else {
            let mut current = root;
            for key in &path {
                current = match key {
                    Field(key) => &current[*key],
                    Index(index) => &current[*index],
                };
            }
            let length = current.as_array().map_or(0, Vec::len);
            ArrayEdit::prepare(
                root,
                path.clone(),
                length,
                false,
                Some(added.first().ok_or_else(bad)?.clone()),
                false,
                omit_empty,
            )?
        };
        let mut current = root;
        for key in &path {
            current = match key {
                Field(key) => &current[*key],
                Index(index) => &current[*index],
            };
        }
        let values = current.as_array().map_or(&[][..], Vec::as_slice);
        let mut indices = remove.to_vec();
        indices.sort_unstable();
        indices.dedup();
        let removed = indices
            .iter()
            .map(|i| Ok((*i, values.get(*i).ok_or_else(bad)?.clone())))
            .collect::<Result<Vec<_>, ChartError>>()?;
        let mut kept = Vec::with_capacity(values.len() - removed.len() + added.len());
        let mut next = indices.iter().peekable();
        for (i, value) in values.iter().enumerate() {
            if next.peek().is_some_and(|n| **n == i) {
                next.next();
            } else {
                kept.push(value);
            }
        }
        let old_length = kept.len();
        kept.extend(added.iter());
        let mut order: Vec<usize> = (0..kept.len()).collect();
        if sort {
            let keys: Vec<Beat> = kept
                .iter()
                .map(|v| serde_json::from_value(v["startTime"].clone()).map_err(ChartError::from))
                .collect::<Result<_, _>>()?;
            order.sort_by_key(|i| keys[*i]);
        }
        let mut inverse = vec![0; order.len()];
        for (new, old) in order.iter().enumerate() {
            inverse[*old] = new;
        }
        let added_indices = (old_length..kept.len()).map(|i| inverse[i]).collect();
        let permutation = order
            .iter()
            .enumerate()
            .any(|(i, j)| i != *j)
            .then_some((order, inverse));
        Ok(Self {
            path,
            removed,
            added,
            order: permutation,
            restore: first.restore,
            omit_empty,
            added_indices,
        })
    }
    pub fn apply(&self, root: &mut Value, forward: bool) {
        let value = ensure(root, &self.path);
        if value.is_null() {
            *value = Value::Array(Vec::new());
        }
        let values = value.as_array_mut().unwrap();
        if forward {
            for (index, _) in self.removed.iter().rev() {
                values.remove(*index);
            }
            values.extend(self.added.iter().cloned());
            if let Some((order, _)) = &self.order {
                permute(values, order);
            }
            if self.omit_empty && values.is_empty() {
                set(root, &self.path, None);
            }
        } else {
            if let Some((_, inverse)) = &self.order {
                permute(values, inverse);
            }
            values.truncate(values.len() - self.added.len());
            for (index, value) in &self.removed {
                values.insert(*index, value.clone());
            }
            for restore in self.restore.iter().rev() {
                match restore {
                    Restore::Value { path, before } => set(root, path, before.clone()),
                    Restore::Length { path, length } => {
                        ensure(root, path).as_array_mut().unwrap().truncate(*length)
                    }
                }
            }
        }
    }
}
