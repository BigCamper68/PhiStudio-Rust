//! Transient plugin passes use the original prpr Effect and ping-pong target.
mod cache;
use crate::Preview;
use cache::Programs;
use macroquad::miniquad;
use macroquad::prelude::*;
use phistudio_sdk::{PreviewPass, PreviewShader, ShaderUniform};
use prpr::core::{Effect, Uniform};
use std::{cell::Cell, collections::BTreeMap, rc::Rc};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PreviewFilterKey {
    pub plugin: String,
    pub generation: u64,
    pub contribution: String,
    pub pass: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewFilter {
    pub plugin: String,
    pub generation: u64,
    pub contribution: String,
    pub pass: PreviewPass,
}
impl PreviewFilter {
    fn key(&self) -> PreviewFilterKey {
        PreviewFilterKey {
            plugin: self.plugin.clone(),
            generation: self.generation,
            contribution: self.contribution.clone(),
            pass: self.pass.id.clone(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewFilterFailure {
    pub key: PreviewFilterKey,
    pub message: String,
}

struct Parameter {
    name: String,
    value: Rc<Cell<ShaderUniform>>,
}
impl Uniform for Parameter {
    fn uniform_pair(&self) -> (String, miniquad::UniformType) {
        let kind = match self.value.get() {
            ShaderUniform::Float(_) => miniquad::UniformType::Float1,
            ShaderUniform::Vec2(_) => miniquad::UniformType::Float2,
            ShaderUniform::Vec4(_) => miniquad::UniformType::Float4,
        };
        (self.name.clone(), kind)
    }
    fn set_time(&mut self, _: f64) {}
    fn apply(&self, material: &Material) {
        match self.value.get() {
            ShaderUniform::Float(value) => material.set_uniform(&self.name, value),
            ShaderUniform::Vec2([x, y]) => material.set_uniform(&self.name, vec2(x, y)),
            ShaderUniform::Vec4([r, g, b, a]) => {
                material.set_uniform(&self.name, Color::new(r, g, b, a))
            }
        }
    }
}
pub(crate) struct Compiled {
    effect: Effect,
    values: BTreeMap<String, Rc<Cell<ShaderUniform>>>,
}
fn source(pass: &PreviewPass) -> Result<&str, String> {
    match &pass.shader {
        PreviewShader::Preset { name } => {
            Effect::get_preset(name).ok_or_else(|| "Unknown preview shader preset".into())
        }
        PreviewShader::Fragment { source } => Ok(source),
    }
}
fn validate_uniforms(pass: &PreviewPass, source: &str) -> Result<(), String> {
    let parameters = phistudio_core::shaders::shader_parameters(source);
    for (name, value) in &pass.uniforms {
        let declaration = parameters
            .iter()
            .find(|p| &p.name == name)
            .ok_or_else(|| format!("Preview uniform {name} has no supported declaration"))?;
        let matches = match value {
            ShaderUniform::Float(_) => declaration.default.is_number(),
            ShaderUniform::Vec2(_) => declaration.default.as_array().is_some_and(|a| a.len() == 2),
            ShaderUniform::Vec4(_) => declaration.default.as_array().is_some_and(|a| a.len() == 4),
        };
        if !matches {
            return Err(format!(
                "Preview uniform {name} does not match its GLSL type"
            ));
        }
    }
    Ok(())
}
fn compile(pass: &PreviewPass) -> Result<Compiled, String> {
    pass.validate()?;
    let source = source(pass)?;
    validate_uniforms(pass, source)?;
    let mut values = BTreeMap::new();
    let uniforms = pass
        .uniforms
        .iter()
        .map(|(name, value)| {
            let value = Rc::new(Cell::new(*value));
            values.insert(name.clone(), value.clone());
            Box::new(Parameter {
                name: name.clone(),
                value,
            }) as Box<dyn Uniform>
        })
        .collect();
    let effect = Effect::new(f64::NEG_INFINITY..f64::INFINITY, source, uniforms, true)
        .map_err(|e| e.to_string())?;
    Ok(Compiled { effect, values })
}

#[derive(Default)]
pub(crate) struct ExtensionEffects(Programs<Compiled>);
impl ExtensionEffects {
    pub(crate) fn enabled(&self) -> bool {
        self.0.enabled()
    }
    pub(crate) fn render(&mut self, resource: &mut prpr::core::Resource) {
        for key in &self.0.order {
            let Some(program) = self.0.entries.get_mut(key) else {
                continue;
            };
            let Some(Ok(compiled)) = &mut program.compiled else {
                continue;
            };
            for (name, value) in &program.pass.uniforms {
                if let Some(cell) = compiled.values.get(name) {
                    cell.set(*value);
                }
            }
            compiled.effect.update(resource);
            compiled.effect.render(resource);
        }
    }
}
impl Preview {
    /// Must run on the shared graphics thread. At most one program is compiled
    /// per call; values update without compilation. Invalid programs are cached.
    pub fn set_plugin_effects(&mut self, effects: &[PreviewFilter]) -> Vec<PreviewFilterFailure> {
        let failures = self.extension_effects.0.synchronize(
            effects,
            self.editor_options.preview_shaders,
            compile,
        );
        self.configure_effects();
        failures
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preset_and_custom_uniform_declarations_use_the_original_authoring_types() {
        let mut pass = PreviewPass {
            id: "gray".into(),
            shader: PreviewShader::Preset {
                name: "grayscale".into(),
            },
            uniforms: BTreeMap::from([("factor".into(), ShaderUniform::Float(0.4))]),
        };
        validate_uniforms(&pass, source(&pass).unwrap()).unwrap();
        pass.uniforms
            .insert("factor".into(), ShaderUniform::Vec2([0., 1.]));
        assert!(validate_uniforms(&pass, source(&pass).unwrap()).is_err());
        pass.shader = PreviewShader::Fragment {
            source: "uniform vec2 offset;\nuniform vec4 tint;\n".into(),
        };
        pass.uniforms = BTreeMap::from([
            ("offset".into(), ShaderUniform::Vec2([0.5, 0.])),
            ("tint".into(), ShaderUniform::Vec4([0.2, 1., 0.5, 1.])),
        ]);
        validate_uniforms(&pass, source(&pass).unwrap()).unwrap();
        pass.uniforms
            .insert("missing".into(), ShaderUniform::Float(0.));
        assert!(validate_uniforms(&pass, source(&pass).unwrap()).is_err());
    }
}
