use super::*;
use image::{ImageDecoder, ImageReader, Limits};
use std::io::BufReader;

impl Worker {
    pub(super) fn prepare_image(
        &self,
        call: &HostCall,
        registry: &Mutex<Registry>,
        job: u64,
        file: u64,
    ) -> Result<Value, String> {
        let (path, _, project) = self.asset_file(call, file)?;
        self.check_project(call, project.as_ref())?;
        if let Some(image) = self
            .assets
            .lock()
            .map_err(|_| "Asset registry interrupted")?
            .owned(file, &call.origin)?
            .image
            .as_ref()
        {
            return Ok(json!(AssetImage {
                file,
                width: image.size[0] as u32,
                height: image.size[1] as u32
            }));
        }
        let mut reader = ImageReader::new(BufReader::new(
            std::fs::File::open(path).map_err(|e| e.to_string())?,
        ))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
        let mut limits = Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(MAX_ASSET_IMAGES_BYTES);
        reader.limits(limits);
        let decoder = reader.into_decoder().map_err(|e| e.to_string())?;
        let (width, height) = decoder.dimensions();
        let bytes = u64::from(width)
            .saturating_mul(u64::from(height))
            .saturating_mul(4);
        if width == 0 || height == 0 || bytes > MAX_ASSET_IMAGE_BYTES {
            return Err("Image must contain 1–4,194,304 pixels within 4096 × 4096".into());
        }
        if self
            .assets
            .lock()
            .map_err(|_| "Asset registry interrupted")?
            .image_bytes()
            + bytes
            > MAX_ASSET_IMAGES_BYTES
        {
            return Err("Decoded asset images exceed 64 MiB; close an image first".into());
        }
        let decoded = image::DynamicImage::from_decoder(decoder)
            .map_err(|e| e.to_string())?
            .into_rgba8();
        self.asset_call_active(call, registry, job)?;
        let _registry = registry.lock().map_err(|_| "Plugin registry interrupted")?;
        let mut assets = self
            .assets
            .lock()
            .map_err(|_| "Asset registry interrupted")?;
        check_project(&assets.project, call, project.as_ref())?;
        let snapshot = assets.files.get_mut(&file).ok_or("Asset was closed")?;
        snapshot.image = Some(Arc::new(PreparedImage {
            size: [width as usize, height as usize],
            rgba: decoded.into_raw(),
        }));
        Ok(json!(AssetImage {
            file,
            width,
            height
        }))
    }
}
