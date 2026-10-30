#[cfg(not(any(target_os = "android", target_os = "ios")))]
use super::*;

pub(super) fn display_density() -> f32 {
    #[cfg(target_os = "android")]
    {
        f32::from_bits(crate::android::DENSITY.load(std::sync::atomic::Ordering::Acquire))
    }
    #[cfg(target_os = "ios")]
    {
        crate::ios::density()
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        // Debug-only device-density simulation for the real GL/input regression.
        #[cfg(debug_assertions)]
        if let Some(density) = std::env::var("PHISTUDIO_TEST_DISPLAY_DENSITY")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
        {
            return density;
        }
        unsafe { get_internal_gl() }.quad_context.dpi_scale()
    }
}

pub(super) fn scaled_display_density(density: f32, scale: f32) -> f32 {
    let density = if density.is_finite() && density > 0. {
        density
    } else {
        1.
    };
    let scale = if scale.is_finite() {
        scale.clamp(0.75, 1.5)
    } else {
        1.
    };
    density * scale
}
