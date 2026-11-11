use super::*;
use std::collections::BTreeSet;

pub const MAX_LIBRARY_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_TEMPLATES: usize = 512;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub curve: Curve,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CurveLibrary {
    pub templates: Vec<Template>,
}
#[derive(Serialize, Deserialize)]
struct File {
    format: String,
    version: u32,
    templates: Vec<Template>,
}
impl CurveLibrary {
    pub fn validate(&self) -> Result<(), ChartError> {
        if self.templates.len() > MAX_TEMPLATES {
            return Err(invalid("The easing library supports up to 512 templates"));
        }
        let mut ids = BTreeSet::new();
        for t in &self.templates {
            if t.id.is_empty()
                || t.id.chars().count() > 100
                || !ids.insert(&t.id)
                || t.name.trim().is_empty()
                || t.name.chars().count() > 160
            {
                return Err(invalid(
                    "Easing templates need unique identifiers and nonempty names up to 160 characters",
                ));
            }
            t.curve.validate()?;
        }
        Ok(())
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, ChartError> {
        if bytes.len() > MAX_LIBRARY_BYTES {
            return Err(invalid("The easing library exceeds 8 MiB"));
        }
        let file: File = serde_json::from_slice(bytes)?;
        if file.version != 1
            || !["phistudio-animation-curves", "rpen-animation-curves"]
                .contains(&file.format.as_str())
        {
            return Err(invalid("Unsupported easing library format"));
        }
        let result = Self {
            templates: file.templates,
        };
        result.validate()?;
        Ok(result)
    }
    pub fn export(&self) -> Result<Vec<u8>, ChartError> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(&File {
            format: "phistudio-animation-curves".into(),
            version: 1,
            templates: self.templates.clone(),
        })?;
        if bytes.len() > MAX_LIBRARY_BYTES {
            return Err(invalid("The easing library exceeds 8 MiB"));
        }
        Ok(bytes)
    }
    /// Name collisions produce a new name; imports never overwrite saved work.
    pub fn save(
        &mut self,
        name: &str,
        curve: Curve,
        replace: Option<&str>,
    ) -> Result<String, ChartError> {
        self.validate()?;
        curve.validate()?;
        let base = name.trim().chars().take(140).collect::<String>();
        let base = if base.is_empty() {
            "Curve".to_owned()
        } else {
            base
        };
        let replaced = replace.and_then(|id| self.templates.iter().position(|t| t.id == id));
        if replaced.is_none() && self.templates.len() >= MAX_TEMPLATES {
            return Err(invalid("The easing library supports up to 512 templates"));
        }
        let mut name = base.clone();
        let mut suffix = 2;
        while self
            .templates
            .iter()
            .enumerate()
            .any(|(i, t)| Some(i) != replaced && t.name == name)
        {
            name = format!("{base} ({suffix})");
            suffix += 1;
        }
        let id = if let Some(i) = replaced {
            self.templates[i].id.clone()
        } else {
            let mut number = 1;
            while self
                .templates
                .iter()
                .any(|t| t.id == format!("curve-{number}"))
            {
                number += 1;
            }
            format!("curve-{number}")
        };
        let mut curve = curve;
        curve.name = name.clone();
        let entry = Template {
            id: id.clone(),
            name,
            curve,
        };
        if let Some(i) = replaced {
            self.templates[i] = entry;
        } else {
            self.templates.push(entry);
        }
        Ok(id)
    }
    pub fn merge(&mut self, other: &Self) -> Result<(), ChartError> {
        self.validate()?;
        other.validate()?;
        if self.templates.len() + other.templates.len() > MAX_TEMPLATES {
            return Err(invalid("The easing library supports up to 512 templates"));
        }
        let mut next = self.clone();
        for t in &other.templates {
            next.save(&t.name, t.curve.clone(), None)?;
        }
        *self = next;
        Ok(())
    }
}
