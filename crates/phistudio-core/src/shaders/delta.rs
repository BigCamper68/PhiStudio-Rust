use super::*;

impl ShaderDelta {
    pub fn apply(&self, doc: &mut ChartDocument, forward: bool) {
        self.apply_root(&mut doc.root, forward);
    }

    pub(super) fn apply_root(&self, root: &mut Value, forward: bool) {
        match self {
            Self::Initialize(value) => {
                if forward {
                    root[EXTRA_KEY] = value.clone();
                } else {
                    root.as_object_mut().unwrap().shift_remove(EXTRA_KEY);
                }
            }
            Self::Effect(edit) => edit.apply(root, forward),
            Self::Move { from, to } => {
                let (from, to) = if forward { (*from, *to) } else { (*to, *from) };
                let list = root[EXTRA_KEY]["effects"].as_array_mut().unwrap();
                let value = list.remove(from);
                list.insert(to, value);
            }
            Self::Tempo { before, after } => {
                root[EXTRA_KEY]["bpm"] = if forward { after } else { before }.clone()
            }
            Self::Source {
                path,
                before,
                after,
                container,
            } => {
                if !forward && container.is_none() {
                    root.as_object_mut().unwrap().shift_remove(SOURCES_KEY);
                } else {
                    if !root[SOURCES_KEY].is_object() {
                        root[SOURCES_KEY] = json!({});
                    }
                    let map = root[SOURCES_KEY].as_object_mut().unwrap();
                    if let Some(value) = if forward {
                        Some(after)
                    } else {
                        before.as_ref()
                    } {
                        map.insert(path.clone(), value.clone());
                    } else {
                        map.shift_remove(path);
                    }
                }
            }
        }
    }
}
