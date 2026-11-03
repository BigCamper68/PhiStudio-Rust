//! Read-only project metadata and bounded native-preview inspection.
use crate::{DocumentVersion, ProjectAssetsVersion};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectInfoQuery {
    pub version: DocumentVersion,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectInfo {
    pub version: DocumentVersion,
    pub assets: ProjectAssetsVersion,
    pub has_assets: bool,
    pub chart_name: Option<String>,
    pub music: Option<String>,
    pub illustration: Option<String>,
    pub music_duration: Option<f64>,
    pub music_offset: f64,
    pub locale: String,
    pub plugins: Vec<ProjectPluginInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPluginInfo {
    pub id: String,
    pub version: crate::Version,
}

/// A source JSON location to reveal through the editor's existing selection UI.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorReveal {
    pub version: DocumentVersion,
    pub path: String,
    pub beat: Option<f64>,
}

pub const MAX_PREVIEW_INSPECTION_POINTS: usize = 64;

/// Offsets are seconds relative to this source note's actual native judgment time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewInspection {
    pub version: DocumentVersion,
    pub line: usize,
    pub note: usize,
    pub offsets: Vec<f64>,
}
impl PreviewInspection {
    pub fn validate(&self) -> Result<(), String> {
        if self.offsets.is_empty()
            || self.offsets.len() > MAX_PREVIEW_INSPECTION_POINTS
            || self
                .offsets
                .iter()
                .any(|v| !v.is_finite() || !(-3600. ..=3600.).contains(v))
        {
            return Err("Select 1–64 finite preview offsets within one hour of the note".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewInspectionPoint {
    pub offset: f64,
    /// Native normalized preview coordinates; None denotes non-finite geometry.
    /// The center is (0.5, 0.5).
    pub position: Option<[f64; 2]>,
    pub inside: bool,
    pub visible: bool,
    pub width: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewInspectionResult {
    pub version: DocumentVersion,
    pub line: usize,
    pub note: usize,
    pub judgment_time: f64,
    pub end_time: f64,
    pub aspect_ratio: f64,
    pub points: Vec<PreviewInspectionPoint>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_inspection_has_explicit_bounds_and_portable_invalid_geometry() {
        let mut request = PreviewInspection {
            version: DocumentVersion::default(),
            line: 0,
            note: 0,
            offsets: vec![0., -0.3],
        };
        request.validate().unwrap();
        for offsets in [
            vec![],
            vec![0.; 65],
            vec![f64::NAN],
            vec![f64::INFINITY],
            vec![-3600.1],
        ] {
            request.offsets = offsets;
            assert!(request.validate().is_err());
        }
        let point = PreviewInspectionPoint {
            offset: 0.,
            position: None,
            inside: false,
            visible: false,
            width: None,
        };
        let value = serde_json::to_value(&point).unwrap();
        assert_eq!(
            serde_json::from_value::<PreviewInspectionPoint>(value).unwrap(),
            point
        );
    }
}
