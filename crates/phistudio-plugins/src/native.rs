//! Lazy native loading. Installing/discovering a package never executes it.
use phistudio_sdk::{Plugin, Request, Response};
use std::path::{Path, PathBuf};

pub const MAX_NATIVE_BYTES: u64 = 128 * 1024 * 1024;
pub const NATIVE_SUPPORTED: bool = cfg!(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "windows"
));

/// Header/file validation only; native constructors run on explicit activation.
pub fn validate_native_file(path: &Path) -> Result<(), String> {
    use std::io::Read;
    if !NATIVE_SUPPORTED {
        return Err("Native plugins are unavailable on this platform".into());
    }
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_NATIVE_BYTES {
        return Err("Native module is not a file or exceeds 128 MiB".into());
    }
    let mut header = [0_u8; 64];
    file.read_exact(&mut header)
        .map_err(|e| format!("Invalid native module header: {e}"))?;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let machine = if cfg!(target_arch = "aarch64") {
            183
        } else if cfg!(target_arch = "x86_64") {
            62
        } else if cfg!(target_arch = "arm") {
            40
        } else if cfg!(target_arch = "x86") {
            3
        } else {
            0
        };
        let class = if cfg!(target_pointer_width = "64") {
            2
        } else {
            1
        };
        if &header[..4] != b"\x7fELF"
            || header[4] != class
            || header[5] != 1
            || u16::from_le_bytes([header[16], header[17]]) != 3
            || machine == 0
            || u16::from_le_bytes([header[18], header[19]]) != machine
        {
            return Err(
                "Native module must be a shared ELF library for the editor's architecture".into(),
            );
        }
    }
    #[cfg(target_os = "windows")]
    if &header[..2] != b"MZ" {
        return Err("Native module must be a Windows DLL".into());
    }
    #[cfg(target_os = "macos")]
    if !matches!(
        &header[..4],
        b"\xcf\xfa\xed\xfe" | b"\xfe\xed\xfa\xcf" | b"\xca\xfe\xba\xbe" | b"\xca\xfe\xba\xbf"
    ) {
        return Err("Native module must be a Mach-O dynamic library".into());
    }
    Ok(())
}

pub struct NativePlugin {
    path: PathBuf,
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "windows"
    ))]
    instance: Option<supported::Instance>,
    poisoned: bool,
}
impl NativePlugin {
    /// The path must name a confined package snapshot. No library is opened yet.
    pub fn new(path: PathBuf) -> Result<Self, String> {
        validate_native_file(&path)?;
        Ok(Self {
            path,
            #[cfg(any(
                target_os = "linux",
                target_os = "android",
                target_os = "macos",
                target_os = "windows"
            ))]
            instance: None,
            poisoned: false,
        })
    }
}
impl Plugin for NativePlugin {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        if self.poisoned {
            return Err("Native instance failed; reload the plugin before using it again".into());
        }
        #[cfg(any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "windows"
        ))]
        {
            if self.instance.is_none() {
                if !matches!(request, Request::Activate { .. }) {
                    return Err("Activate the native plugin before calling it".into());
                }
                validate_native_file(&self.path)?;
                self.instance = Some(supported::Instance::load(&self.path)?);
            }
            let result = self
                .instance
                .as_mut()
                .ok_or_else(|| "Native instance is unavailable".into())
                .and_then(|instance| instance.exchange(request));
            if result.is_err() {
                self.poisoned = true;
            }
            if result.is_err() || matches!(request, Request::Deactivate) {
                self.instance.take();
            }
            result
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "windows"
        )))]
        {
            let _ = (&self.path, request);
            Err("Native plugins are unavailable on this platform".into())
        }
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "windows"
))]
mod supported {
    use super::*;
    use libloading::Library;
    use phistudio_sdk::{API_VERSION, MAX_MESSAGE_BYTES, native::*};
    use std::{ffi::c_void, ptr::NonNull};

    pub(super) struct Instance {
        // All function pointers and state remain valid until this library drops.
        _library: Library,
        api: NativeApiV1,
        state: NonNull<c_void>,
    }
    // SAFETY: the ABI requires movable state and serialized calls; the owning
    // runtime never invokes a native instance concurrently.
    unsafe impl Send for Instance {}

    impl Instance {
        pub(super) fn load(path: &Path) -> Result<Self, String> {
            // SAFETY: explicit native activation authorizes module execution.
            // Native code is trusted and must satisfy the SDK's C ABI contract.
            let library = unsafe { Library::new(path) }
                .map_err(|e| format!("Cannot open native plugin: {e}"))?;
            // SAFETY: the required symbol has this documented C signature.
            let entry = unsafe {
                library.get::<unsafe extern "C" fn() -> *const NativeApiV1>(NATIVE_ENTRY)
            }
            .map_err(|e| format!("Missing native plugin entry: {e}"))?;
            // SAFETY: a conforming entry returns a static API table.
            let ptr = unsafe { entry() };
            if ptr.is_null() || !ptr.is_aligned() {
                return Err("Native plugin returned a null or misaligned API table".into());
            }
            // SAFETY: check the fixed ABI header before reading function slots.
            let header = unsafe { ptr.cast::<NativeHeader>().read() };
            if header.abi_version != NATIVE_ABI_VERSION
                || header.api_version != API_VERSION
                || header.struct_size < std::mem::size_of::<NativeApiV1>() as u32
            {
                return Err("Native plugin ABI/API version or table size is incompatible".into());
            }
            // SAFETY: the validated header declares a full version-1 table.
            let api = unsafe { ptr.read() };
            let create = api.create.ok_or("Native plugin is missing create")?;
            if api.destroy.is_none() || api.call.is_none() || api.free.is_none() {
                return Err("Native plugin API table is incomplete".into());
            }
            // SAFETY: create's returned state is owned until destroy below.
            let state = NonNull::new(unsafe { create() }).ok_or("Native plugin factory failed")?;
            Ok(Self {
                _library: library,
                api,
                state,
            })
        }
        pub(super) fn exchange(&mut self, request: &Request) -> Result<Response, String> {
            let input = serde_json::to_vec(request).map_err(|e| e.to_string())?;
            if input.len() > MAX_MESSAGE_BYTES {
                return Err("Native request exceeds message limit".into());
            }
            let call = self.api.call.ok_or("Native call function is unavailable")?;
            let free = self.api.free.ok_or("Native free function is unavailable")?;
            // SAFETY: state is live and input remains borrowed until return.
            let output = unsafe { call(self.state.as_ptr(), input.as_ptr(), input.len()) };
            let start = output.data as usize;
            let end = start.checked_add(output.len);
            let input_start = input.as_ptr() as usize;
            let input_end = input_start + input.len();
            // An oversized claimed range can cross unrelated allocations.
            // Only bounded ranges diagnose overlap; always protect input pointers.
            if (input_start..input_end).contains(&start)
                || (output.len <= MAX_MESSAGE_BYTES
                    && end.is_some_and(|end| start < input_end && input_start < end))
            {
                // Never give a plugin a host allocation through its free API.
                return Err("Native output must own a separate allocation".into());
            }
            let _buffer = Buffer {
                data: output.data,
                len: output.len,
                free,
            };
            if output.data.is_null() || output.len == 0 || output.len > MAX_MESSAGE_BYTES {
                return Err("Native output is null, empty or exceeds message limit".into());
            }
            end.ok_or("Native output range overflowed")?;
            // SAFETY: a conforming native module returns a readable owned buffer.
            // Native addresses cannot be sandboxed or memory-checked by this ABI.
            let bytes = unsafe { std::slice::from_raw_parts(output.data, output.len) };
            match output.status {
                0 => serde_json::from_slice(bytes)
                    .map_err(|e| format!("Invalid native response: {e}")),
                1 => Err(std::str::from_utf8(bytes)
                    .map_err(|_| "Native error is not UTF-8".to_owned())?
                    .to_owned()),
                _ => Err("Native output has an invalid status".into()),
            }
        }
    }
    impl Drop for Instance {
        fn drop(&mut self) {
            if let Some(destroy) = self.api.destroy {
                // SAFETY: state is returned exactly once before library unload.
                unsafe { destroy(self.state.as_ptr()) };
            }
        }
    }
    struct Buffer {
        data: *mut u8,
        len: usize,
        free: unsafe extern "C" fn(*mut u8, usize),
    }
    impl Drop for Buffer {
        fn drop(&mut self) {
            if !self.data.is_null() {
                // SAFETY: return this module's output allocation exactly once.
                unsafe { (self.free)(self.data, self.len) };
            }
        }
    }
}
