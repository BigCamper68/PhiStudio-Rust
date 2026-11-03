//! Allocation helpers for the Wasm32 protocol. Buffers never borrow host memory.
use crate::{MAX_MESSAGE_BYTES, Plugin, Request};

#[doc(hidden)]
pub fn alloc(len: u32) -> u32 {
    if len as usize > MAX_MESSAGE_BYTES {
        return 0;
    }
    let bytes = vec![0u8; len as usize].into_boxed_slice();
    Box::into_raw(bytes) as *mut u8 as u32
}

/// # Safety
/// `ptr,len` must be a live allocation returned by this module, freed once.
#[doc(hidden)]
pub unsafe fn free(ptr: u32, len: u32) {
    let slice = std::ptr::slice_from_raw_parts_mut(ptr as *mut u8, len as usize);
    // SAFETY: guaranteed by the caller's ownership contract above.
    drop(unsafe { Box::from_raw(slice) });
}

/// # Safety
/// The host must supply a readable buffer of `len` bytes returned by `alloc`.
#[doc(hidden)]
pub unsafe fn dispatch(plugin: &mut dyn Plugin, ptr: u32, len: u32) -> u64 {
    if len as usize > MAX_MESSAGE_BYTES {
        return 0;
    }
    // SAFETY: the host writes into its still-live guest allocation.
    let input = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let response = serde_json::from_slice::<Request>(input)
        .map_err(|e| e.to_string())
        .and_then(|r| plugin.call(&r));
    // A protocol/runtime failure traps the invocation; no effects are committed.
    let Ok(response) = response else {
        return 0;
    };
    let Ok(output) = serde_json::to_vec(&response) else {
        return 0;
    };
    if output.len() > MAX_MESSAGE_BYTES {
        return 0;
    }
    let len = output.len() as u64;
    let ptr = Box::into_raw(output.into_boxed_slice()) as *mut u8 as u32;
    (len << 32) | u64::from(ptr)
}
