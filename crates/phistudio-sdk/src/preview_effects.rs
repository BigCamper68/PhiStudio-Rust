//! Transient post-processing of the shared editor preview, after chart effects.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_PREVIEW_PASSES: usize = 8;
pub const MAX_ACTIVE_PREVIEW_PASSES: usize = 16;
pub const MAX_PREVIEW_SHADER_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreviewShader {
    Preset { name: String },
    Fragment { source: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ShaderUniform {
    Float(f32),
    Vec2([f32; 2]),
    Vec4([f32; 4]),
}
impl ShaderUniform {
    pub fn components(&self) -> &[f32] {
        match self {
            Self::Float(value) => std::slice::from_ref(value),
            Self::Vec2(value) => value,
            Self::Vec4(value) => value,
        }
    }
    pub fn same_kind(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewPass {
    pub id: String,
    pub shader: PreviewShader,
    #[serde(default)]
    pub uniforms: BTreeMap<String, ShaderUniform>,
}
impl PreviewPass {
    pub fn validate(&self) -> Result<(), String> {
        if !crate::valid_id(&self.id) || self.uniforms.len() > 32 {
            return Err("Preview passes need a valid ID and at most 32 uniforms".into());
        }
        match &self.shader {
            PreviewShader::Preset { name }
                if name.is_empty()
                    || name.len() > 128
                    || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') =>
            {
                return Err("Use a named preview shader preset".into());
            }
            PreviewShader::Fragment { source }
                if source.trim().is_empty()
                    || source.len() > MAX_PREVIEW_SHADER_BYTES
                    || source.contains('\0') =>
            {
                return Err("Preview fragment shaders must contain 1 byte–64 KiB of GLSL".into());
            }
            _ => {}
        }
        for (name, value) in &self.uniforms {
            if name.is_empty()
                || name.len() > 128
                || !name.bytes().enumerate().all(|(i, c)| {
                    c == b'_' || c.is_ascii_alphabetic() || (i != 0 && c.is_ascii_digit())
                })
                || name.starts_with("gl_")
                || [
                    "time",
                    "screenSize",
                    "UVScale",
                    "screenTexture",
                    "Model",
                    "Projection",
                    "Texture",
                ]
                .contains(&name.as_str())
                || value.components().iter().any(|v| !v.is_finite())
            {
                return Err("Invalid or reserved preview shader uniform".into());
            }
        }
        Ok(())
    }
}
pub fn validate_preview_passes(passes: &[PreviewPass]) -> Result<(), String> {
    if passes.len() > MAX_PREVIEW_PASSES {
        return Err("Preview contribution exceeds eight passes".into());
    }
    let mut ids = BTreeSet::new();
    for pass in passes {
        pass.validate()?;
        if !ids.insert(&pass.id) {
            return Err("Preview pass IDs must be distinct within a contribution".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn pass() -> PreviewPass {
        PreviewPass {
            id: "grade".into(),
            shader: PreviewShader::Preset {
                name: "grayscale".into(),
            },
            uniforms: BTreeMap::from([("factor".into(), ShaderUniform::Float(0.5))]),
        }
    }
    #[test]
    fn effect_descriptions_validate_names_sources_unique_passes_and_uniform_values() {
        let original = pass();
        original.validate().unwrap();
        let restored: PreviewPass =
            serde_json::from_value(serde_json::to_value(&original).unwrap()).unwrap();
        assert_eq!(restored, original);
        assert!(validate_preview_passes(&[original.clone(), original.clone()]).is_err());
        for name in [
            "time",
            "UVScale",
            "screenSize",
            "screenTexture",
            "Model",
            "gl_custom",
            "1bad",
            "bad.name",
        ] {
            let mut bad = pass();
            bad.uniforms.insert(name.into(), ShaderUniform::Float(1.));
            assert!(bad.validate().is_err(), "{name}");
        }
        let mut bad = pass();
        bad.uniforms
            .insert("factor".into(), ShaderUniform::Float(f32::NAN));
        assert!(bad.validate().is_err());
        for source in [
            String::new(),
            " ".into(),
            "\0".into(),
            "x".repeat(MAX_PREVIEW_SHADER_BYTES + 1),
        ] {
            let mut bad = pass();
            bad.shader = PreviewShader::Fragment { source };
            assert!(bad.validate().is_err());
        }
        let mut value = json!({"id":"grade","shader":{"kind":"fragment","source":"void main() {}"},"uniforms":{}});
        value["path"] = json!("/outside");
        assert!(serde_json::from_value::<PreviewPass>(value).is_err());
    }
}
