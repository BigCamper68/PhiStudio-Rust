use super::{PreviewFilter, PreviewFilterFailure, PreviewFilterKey};
use phistudio_sdk::{MAX_ACTIVE_PREVIEW_PASSES, PreviewPass};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Program<T> {
    pub pass: PreviewPass,
    pub compiled: Option<Result<T, String>>,
    pending: u64,
}
pub(super) struct Programs<T> {
    pub entries: BTreeMap<PreviewFilterKey, Program<T>>,
    pub order: Vec<PreviewFilterKey>,
    sequence: u64,
}
impl<T> Default for Programs<T> {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            order: vec![],
            sequence: 0,
        }
    }
}
fn same_program(a: &PreviewPass, b: &PreviewPass) -> bool {
    a.shader == b.shader
        && a.uniforms.len() == b.uniforms.len()
        && a.uniforms
            .iter()
            .zip(&b.uniforms)
            .all(|((an, av), (bn, bv))| an == bn && av.same_kind(bv))
}
impl<T> Programs<T> {
    pub fn synchronize(
        &mut self,
        filters: &[PreviewFilter],
        compile: bool,
        mut create: impl FnMut(&PreviewPass) -> Result<T, String>,
    ) -> Vec<PreviewFilterFailure> {
        let mut live = BTreeSet::new();
        self.order.clear();
        for filter in filters.iter().take(MAX_ACTIVE_PREVIEW_PASSES) {
            let key = filter.key();
            if !live.insert(key.clone()) {
                continue;
            }
            self.order.push(key.clone());
            self.sequence = self.sequence.saturating_add(1);
            match self.entries.get_mut(&key) {
                Some(program) if same_program(&program.pass, &filter.pass) => {
                    program.pass.uniforms.clone_from(&filter.pass.uniforms);
                }
                _ => {
                    self.entries.insert(
                        key,
                        Program {
                            pass: filter.pass.clone(),
                            compiled: None,
                            pending: self.sequence,
                        },
                    );
                }
            }
        }
        // Drop retired GPU resources before allocating another program.
        self.entries.retain(|key, _| live.contains(key));
        if compile {
            // Source churn cannot starve an older uncompiled pass from another mod.
            let oldest = self
                .entries
                .iter()
                .filter(|(_, p)| p.compiled.is_none())
                .min_by_key(|(_, p)| p.pending)
                .map(|(key, _)| key.clone());
            if let Some(key) = oldest
                && let Some(program) = self.entries.get_mut(&key)
            {
                program.compiled =
                    Some(create(&program.pass).map_err(|e| e.chars().take(4096).collect()));
            }
        }
        self.entries
            .iter()
            .filter_map(|(key, program)| {
                if let Some(Err(message)) = &program.compiled {
                    Some(PreviewFilterFailure {
                        key: key.clone(),
                        message: message.clone(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn enabled(&self) -> bool {
        self.entries
            .values()
            .any(|p| matches!(p.compiled, Some(Ok(_))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phistudio_sdk::{PreviewShader, ShaderUniform};
    use std::{cell::Cell, rc::Rc};
    fn filter(id: &str) -> PreviewFilter {
        PreviewFilter {
            plugin: "test.effects".into(),
            generation: 1,
            contribution: "chain".into(),
            pass: PreviewPass {
                id: id.into(),
                shader: PreviewShader::Preset {
                    name: "grayscale".into(),
                },
                uniforms: BTreeMap::from([("factor".into(), ShaderUniform::Float(0.5))]),
            },
        }
    }
    struct Owned(Rc<Cell<usize>>);
    impl Drop for Owned {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    #[test]
    fn uniform_values_reuse_programs_while_source_layout_owner_changes_and_clear_release_them() {
        let drops = Rc::new(Cell::new(0));
        let calls = Cell::new(0);
        let mut cache = Programs::default();
        let mut spec = filter("first");
        let create = |_: &PreviewPass| {
            calls.set(calls.get() + 1);
            Ok(Owned(drops.clone()))
        };
        cache.synchronize(std::slice::from_ref(&spec), true, create);
        spec.pass
            .uniforms
            .insert("factor".into(), ShaderUniform::Float(0.8));
        cache.synchronize(std::slice::from_ref(&spec), true, create);
        assert_eq!(calls.get(), 1);
        assert_eq!(drops.get(), 0);
        spec.pass
            .uniforms
            .insert("factor".into(), ShaderUniform::Vec2([0., 1.]));
        cache.synchronize(std::slice::from_ref(&spec), true, create);
        assert_eq!(calls.get(), 2);
        assert_eq!(drops.get(), 1);
        spec.generation += 1;
        cache.synchronize(&[spec], true, create);
        assert_eq!(calls.get(), 3);
        assert_eq!(drops.get(), 2);
        cache.synchronize(&[], true, create);
        assert_eq!(drops.get(), 3);
        assert!(!cache.enabled());
    }
    #[test]
    fn compilation_is_fair_bounded_and_failures_are_cached_until_the_program_changes() {
        let mut cache = Programs::<()>::default();
        let calls = Cell::new(0);
        let create = |_: &PreviewPass| {
            calls.set(calls.get() + 1);
            Err("Compile failure".into())
        };
        let mut first = filter("first");
        let second = filter("second");
        assert_eq!(
            cache
                .synchronize(&[first.clone(), second.clone()], true, create)
                .len(),
            1
        );
        first.pass.shader = PreviewShader::Fragment {
            source: "changed source".into(),
        };
        let failures = cache.synchronize(&[first.clone(), second.clone()], true, create);
        assert_eq!(calls.get(), 2);
        assert_eq!(failures[0].key.pass, "second");
        cache.synchronize(&[first.clone(), second.clone()], true, create);
        assert_eq!(calls.get(), 3);
        cache.synchronize(&[first, second], true, create);
        assert_eq!(calls.get(), 3);
        cache.synchronize(&[filter("new")], false, create);
        assert_eq!(calls.get(), 3);
        assert!(!cache.enabled());
        let many: Vec<_> = (0..32).map(|i| filter(&format!("pass-{i}"))).collect();
        cache.synchronize(&many, true, create);
        assert_eq!(calls.get(), 4);
        assert_eq!(cache.entries.len(), MAX_ACTIVE_PREVIEW_PASSES);
        assert_eq!(cache.order.len(), MAX_ACTIVE_PREVIEW_PASSES);
    }
}
