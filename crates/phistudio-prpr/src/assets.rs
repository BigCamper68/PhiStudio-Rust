use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use prpr::fs::FileSystem;
use std::{
    any::Any,
    path::{Component, Path, PathBuf},
};

#[derive(Clone)]
pub(crate) struct ChartAssets {
    root: Option<PathBuf>,
    sources: serde_json::Map<String, serde_json::Value>,
}
impl ChartAssets {
    pub fn new(root: Option<&Path>) -> Result<Self> {
        Ok(Self {
            root: root.map(Path::canonicalize).transpose()?,
            sources: Default::default(),
        })
    }
    pub fn with_sources(mut self, sources: serde_json::Map<String, serde_json::Value>) -> Self {
        self.sources = sources;
        self
    }
    fn resolve(&self, name: &str) -> Result<PathBuf> {
        // ZIP import normalizes Windows separators; resolve authored RPE paths
        // the same way without changing their saved JSON strings.
        let normalized = name.replace('\\', "/");
        let name = Path::new(&normalized);
        if name
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            bail!("Asset path must stay inside the chart directory");
        }
        let root = self
            .root
            .as_ref()
            .context("Open the chart from its asset directory to load textures and hitsounds")?;
        let path = root.join(name).canonicalize()?;
        if !path.starts_with(root) {
            bail!("Asset symlink leaves the chart directory");
        }
        Ok(path)
    }
}

#[async_trait]
impl FileSystem for ChartAssets {
    async fn load_file(&mut self, path: &str) -> Result<Vec<u8>> {
        if let Some(source) = self
            .sources
            .get(&path.replace('\\', "/"))
            .and_then(serde_json::Value::as_str)
        {
            return Ok(source.as_bytes().to_vec());
        }
        let path = self.resolve(path)?;
        if path.metadata()?.len() > 64 * 1024 * 1024 {
            bail!("Preview asset exceeds 64 MiB");
        }
        Ok(std::fs::read(path)?)
    }
    async fn exists(&mut self, path: &str) -> Result<bool> {
        Ok(self.resolve(path).is_ok())
    }
    fn list_root(&self) -> Result<Vec<String>> {
        let Some(root) = &self.root else {
            return Ok(Vec::new());
        };
        Ok(std::fs::read_dir(root)?
            .filter_map(|p| p.ok()?.file_name().into_string().ok())
            .collect())
    }
    fn clone_box(&self) -> Box<dyn FileSystem> {
        Box::new(self.clone())
    }
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_asset_paths_resolve_within_the_same_private_boundary() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("assets");
        std::fs::create_dir_all(root.join("textures")).unwrap();
        std::fs::write(root.join("textures/line.png"), b"asset bytes").unwrap();
        std::fs::write(temporary.path().join("outside.png"), b"outside").unwrap();
        let mut assets = ChartAssets::new(Some(&root)).unwrap();
        assert_eq!(
            pollster::block_on(assets.load_file("textures\\line.png")).unwrap(),
            b"asset bytes"
        );
        for name in ["../outside.png", "..\\outside.png", "/outside.png"] {
            assert!(pollster::block_on(assets.load_file(name)).is_err());
        }
    }
}

#[cfg(test)]
mod shader_tests {
    use super::*;
    #[test]
    fn working_glsl_overlay_is_used_without_touching_disk() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("custom.glsl"), b"original").unwrap();
        let sources = serde_json::json!({"custom.glsl":"working"})
            .as_object()
            .unwrap()
            .clone();
        let mut assets = ChartAssets::new(Some(root.path()))
            .unwrap()
            .with_sources(sources);
        assert_eq!(
            pollster::block_on(assets.load_file("custom.glsl")).unwrap(),
            b"working"
        );
        assert_eq!(
            std::fs::read(root.path().join("custom.glsl")).unwrap(),
            b"original"
        );
    }
}
