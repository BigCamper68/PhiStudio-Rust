/// Export the Wasm32 ABI for a stateful plugin factory, e.g.
/// `phistudio_sdk::export_plugin!(|| MyPlugin::default());`.
#[macro_export]
macro_rules! export_plugin {
    ($factory:expr) => {
        #[cfg(target_arch = "wasm32")]
        mod phistudio_exports {
            use super::*;
            static INSTANCE: std::sync::OnceLock<std::sync::Mutex<Box<dyn $crate::Plugin>>> =
                std::sync::OnceLock::new();

            #[unsafe(no_mangle)]
            pub extern "C" fn phistudio_api_version() -> u32 {
                $crate::API_VERSION
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn phistudio_alloc(len: u32) -> u32 {
                $crate::guest::alloc(len)
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn phistudio_free(ptr: u32, len: u32) {
                // SAFETY: the host returns each allocation exactly once.
                unsafe { $crate::guest::free(ptr, len) }
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn phistudio_call(ptr: u32, len: u32) -> u64 {
                let instance =
                    INSTANCE.get_or_init(|| std::sync::Mutex::new(Box::new(($factory)())));
                let Ok(mut plugin) = instance.lock() else {
                    return 0;
                };
                // SAFETY: the host supplies a live guest allocation.
                unsafe { $crate::guest::dispatch(plugin.as_mut(), ptr, len) }
            }
        }
    };
}
