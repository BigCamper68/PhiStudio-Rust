use crate::{DocumentVersion, valid_file_name};
use serde::{Deserialize, Serialize};

pub const FILE_BEFORE_HOOK: &str = "host.file.before";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAction {
    Open,
    Save,
    Export,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileContent {
    Chart,
    Project,
    History,
}

/// An explicit native file workflow. OS paths and recovery writes stay private.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileIntercept {
    pub action: FileAction,
    pub content: FileContent,
    pub version: DocumentVersion,
    /// Source/destination basename, or None when a private project will be created.
    pub name: Option<String>,
}
impl FileIntercept {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .name
            .as_ref()
            .is_some_and(|name| name.is_empty() || name.len() > 1024)
        {
            return Err("Invalid file workflow name".into());
        }
        Ok(())
    }
    pub fn validate_replacement(&self, original: &Self) -> Result<(), String> {
        self.validate()?;
        if self.action != original.action
            || self.content != original.content
            || self.version != original.version
        {
            return Err("File hooks cannot change the action, content or document version".into());
        }
        if self.name != original.name {
            if self.action != FileAction::Export {
                return Err("Only an export's suggested name can be replaced".into());
            }
            let name = self.name.as_deref().ok_or("Export name is required")?;
            if !valid_file_name(name) {
                return Err("Export name must be a safe basename".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacements_preserve_workflow_identity_and_only_rename_export_suggestions() {
        let original = FileIntercept {
            action: FileAction::Export,
            content: FileContent::Project,
            version: DocumentVersion {
                session: 1,
                revision: 2,
            },
            name: Some("chart.pez".into()),
        };
        let mut replacement = original.clone();
        replacement.name = Some("Reviewed chart.pez".into());
        replacement.validate_replacement(&original).unwrap();
        for name in ["../chart.pez", "a/b.pez", "a\\b.pez", "bad\n.pez", "", "x."] {
            replacement.name = Some(name.into());
            assert!(
                replacement.validate_replacement(&original).is_err(),
                "{name}"
            );
        }
        replacement = original.clone();
        replacement.version.revision += 1;
        assert!(replacement.validate_replacement(&original).is_err());
        replacement = original.clone();
        replacement.content = FileContent::Chart;
        assert!(replacement.validate_replacement(&original).is_err());
        replacement = original.clone();
        replacement.action = FileAction::Save;
        assert!(replacement.validate_replacement(&original).is_err());
        for action in [FileAction::Open, FileAction::Save] {
            let original = FileIntercept {
                action,
                ..original.clone()
            };
            let replacement = FileIntercept {
                name: Some("other.json".into()),
                ..original.clone()
            };
            assert!(replacement.validate_replacement(&original).is_err());
        }
        // An existing platform filename is metadata, not a portable output path.
        let source = FileIntercept {
            action: FileAction::Open,
            name: Some("a\nb.json".into()),
            ..original
        };
        source.validate_replacement(&source).unwrap();
    }
}
