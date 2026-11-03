//! Explicit native C ABI. Native modules run with the process's full privileges.
use std::ffi::c_void;

pub const NATIVE_ABI_VERSION: u32 = 1;
pub const NATIVE_ENTRY: &[u8] = b"phistudio_plugin_v1\0";

/// A plugin-owned buffer. Status 0 contains Response JSON; status 1 contains a
/// UTF-8 error. The host returns every valid separate output to free once.
#[repr(C)]
pub struct NativeBuffer {
    pub data: *mut u8,
    pub len: usize,
    pub status: u32,
}

/// The first three fields must retain this layout across future ABI versions.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativeHeader {
    pub abi_version: u32,
    pub api_version: u32,
    pub struct_size: u32,
}

/// Calls are serialized. State may move between threads between calls. No
/// input pointer can be retained, and no function may unwind across this ABI.
/// destroy must stop and join any plugin-owned work before returning.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativeApiV1 {
    pub header: NativeHeader,
    pub create: Option<unsafe extern "C" fn() -> *mut c_void>,
    pub destroy: Option<unsafe extern "C" fn(state: *mut c_void)>,
    pub call: Option<
        unsafe extern "C" fn(state: *mut c_void, data: *const u8, len: usize) -> NativeBuffer,
    >,
    pub free: Option<unsafe extern "C" fn(data: *mut u8, len: usize)>,
}

#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub mod guest {
    use super::*;
    use crate::{MAX_MESSAGE_BYTES, Plugin, Request};
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::Mutex,
    };

    struct Instance(Mutex<Box<dyn Plugin>>);
    fn output(bytes: Vec<u8>, status: u32) -> NativeBuffer {
        let len = bytes.len();
        NativeBuffer {
            data: Box::into_raw(bytes.into_boxed_slice()) as *mut u8,
            len,
            status,
        }
    }
    fn error(mut message: String) -> NativeBuffer {
        let mut end = message.len().min(16_384);
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        output(message.into_bytes(), 1)
    }

    pub fn create(factory: impl FnOnce() -> Box<dyn Plugin>) -> *mut c_void {
        catch_unwind(AssertUnwindSafe(|| {
            Box::into_raw(Box::new(Instance(Mutex::new(factory())))).cast()
        }))
        .unwrap_or(std::ptr::null_mut())
    }

    /// # Safety
    /// State is a live result of create and input is readable for len bytes.
    pub unsafe extern "C" fn call(state: *mut c_void, data: *const u8, len: usize) -> NativeBuffer {
        if state.is_null() || data.is_null() || len == 0 || len > MAX_MESSAGE_BYTES {
            return error("Invalid native request buffer".into());
        }
        let result = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: the ABI caller keeps state and borrowed input alive.
            let (instance, input) = unsafe {
                (
                    &*state.cast::<Instance>(),
                    std::slice::from_raw_parts(data, len),
                )
            };
            let request: Request = serde_json::from_slice(input).map_err(|e| e.to_string())?;
            let mut plugin = instance
                .0
                .lock()
                .map_err(|_| "Native plugin instance is poisoned".to_owned())?;
            let response = plugin.call(&request)?;
            let bytes = serde_json::to_vec(&response).map_err(|e| e.to_string())?;
            if bytes.len() > MAX_MESSAGE_BYTES {
                return Err("Native response exceeds message limit".into());
            }
            Ok(bytes)
        }));
        match result {
            Ok(Ok(bytes)) => output(bytes, 0),
            Ok(Err(message)) => error(message),
            Err(_) => error("Native plugin panicked".into()),
        }
    }

    /// # Safety
    /// State must be a live result of create and destroyed exactly once.
    pub unsafe extern "C" fn destroy(state: *mut c_void) {
        if !state.is_null() {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                // SAFETY: ownership is returned once, after the last callback.
                drop(unsafe { Box::from_raw(state.cast::<Instance>()) });
            }));
        }
    }

    /// # Safety
    /// data/len is a plugin output buffer returned once after the host copies it.
    pub unsafe extern "C" fn free(data: *mut u8, len: usize) {
        if !data.is_null() {
            // SAFETY: every result owns its separate boxed slice allocation.
            drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(data, len)) });
        }
    }
}

/// Export a C-compatible native entry point for a Rust Plugin factory.
#[macro_export]
macro_rules! export_native_plugin {
    ($factory:expr) => {
        #[cfg(not(target_arch = "wasm32"))]
        mod phistudio_native_exports {
            use super::*;
            unsafe extern "C" fn create() -> *mut std::ffi::c_void {
                $crate::native::guest::create(|| Box::new(($factory)()))
            }
            static API: $crate::native::NativeApiV1 = $crate::native::NativeApiV1 {
                header: $crate::native::NativeHeader {
                    abi_version: $crate::native::NATIVE_ABI_VERSION,
                    api_version: $crate::API_VERSION,
                    struct_size: std::mem::size_of::<$crate::native::NativeApiV1>() as u32,
                },
                create: Some(create),
                destroy: Some($crate::native::guest::destroy),
                call: Some($crate::native::guest::call),
                free: Some($crate::native::guest::free),
            };
            #[unsafe(no_mangle)]
            pub extern "C" fn phistudio_plugin_v1() -> *const $crate::native::NativeApiV1 {
                &API
            }
        }
    };
}
