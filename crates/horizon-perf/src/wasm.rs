use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::slice;

use crate::{read_bytes, Capture, summary};

unsafe fn input_bytes<'a>(ptr: *const u8, len: usize) -> Option<&'a [u8]> {
    if ptr.is_null() {
        return None;
    }
    Some(slice::from_raw_parts(ptr, len))
}

fn to_c(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Read a capture from bytes and return its summary as a NUL-terminated string.
/// Returns NULL on failure.
#[no_mangle]
pub extern "C" fn horizon_perf_summary(ptr: *const u8, len: usize) -> *mut c_char {
    let bytes = match unsafe { input_bytes(ptr, len) } {
        Some(b) => b,
        None => return std::ptr::null_mut(),
    };
    match read_bytes(bytes) {
        Ok(cap) => to_c(summary(&cap)),
        Err(e) => to_c(format!("error: {e}")),
    }
}

/// Run a query over a capture. `op` and `args` are NUL-terminated UTF-8.
/// Returns JSON text (caller frees with `horizon_perf_free`).
#[no_mangle]
pub extern "C" fn horizon_perf_query(
    ptr: *const u8,
    len: usize,
    op: *const c_char,
    args: *const c_char,
) -> *mut c_char {
    let bytes = match unsafe { input_bytes(ptr, len) } {
        Some(b) => b,
        None => return std::ptr::null_mut(),
    };
    let op = unsafe {
        if op.is_null() {
            ""
        } else {
            CStr::from_ptr(op).to_str().unwrap_or("")
        }
    };
    let args = unsafe {
        if args.is_null() {
            ""
        } else {
            CStr::from_ptr(args).to_str().unwrap_or("")
        }
    };
    match read_bytes(bytes) {
        Ok(cap) => to_c(crate::query::query_json(&cap, op, args)),
        Err(e) => to_c(format!("{{\"ok\":false,\"error\":\"{e}\"}}")),
    }
}

/// Free a string returned by the functions above.
#[no_mangle]
pub extern "C" fn horizon_perf_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}

// Provide query_json helper expectation? Spec says json. Add a passthrough query function name matching op "spans".
// We delegate to string-based query output if available. Since horizon_perf::query has no json serializer, expose
// a simple spans query JSON via serde_json-style manual.