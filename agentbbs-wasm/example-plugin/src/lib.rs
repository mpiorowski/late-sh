//! Example AgentBBS WASM plugin.
//!
//! Implements the agentbbs-wasm host ABI (version 1) with two commands:
//!
//! - `echo`      — returns the incoming `args` payload unchanged.
//! - `uppercase` — uppercases `args.text` (a string) and returns it.
//!
//! Build for the host with:
//!
//! ```sh
//! cargo build --release --target wasm32-unknown-unknown
//! ```
//!
//! This crate has no_std-free dependencies and uses only `core`/`alloc` plus a
//! tiny hand-rolled JSON shim, so it compiles to a small, dependency-free wasm
//! module. It is illustrative; the host's own tests use a WAT module instead.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::alloc::{GlobalAlloc, Layout};

// ---------------------------------------------------------------------------
// A minimal bump allocator so we can use `alloc` types on wasm without pulling
// in a real allocator crate. Never frees; fine for short-lived plugin calls.
// ---------------------------------------------------------------------------

struct BumpAlloc;

static mut HEAP: [u8; 1 << 20] = [0; 1 << 20];
static mut OFFSET: usize = 0;

const HEAP_LEN: usize = 1 << 20;

unsafe impl GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let base = core::ptr::addr_of_mut!(HEAP) as *mut u8;
        let align = layout.align();
        let off = (OFFSET + align - 1) & !(align - 1);
        let next = off + layout.size();
        if next > HEAP_LEN {
            return core::ptr::null_mut();
        }
        OFFSET = next;
        base.add(off)
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOC: BumpAlloc = BumpAlloc;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // Trap on panic; the host will surface this as an error.
    core::arch::wasm32::unreachable()
}

// ---------------------------------------------------------------------------
// Host imports.
// ---------------------------------------------------------------------------

#[link(wasm_import_module = "agentbbs")]
extern "C" {
    fn log(ptr: i32, len: i32);
    fn abi_version() -> i32;
}

fn host_log(s: &str) {
    unsafe { log(s.as_ptr() as i32, s.len() as i32) }
}

// ---------------------------------------------------------------------------
// ABI exports.
// ---------------------------------------------------------------------------

/// Allocate `len` bytes in guest memory and return a pointer the host can
/// write into. Leaks by design (bump allocator).
#[no_mangle]
pub extern "C" fn alloc(len: i32) -> i32 {
    let len = len.max(0) as usize;
    let layout = Layout::from_size_align(len.max(1), 1).unwrap();
    unsafe { ALLOC.alloc(layout) as i32 }
}

/// Plugin entry point. Reads the JSON request at `[ptr, ptr+len)`, dispatches
/// on `kind`, and returns a packed `(out_ptr << 32) | out_len`.
///
/// # Safety
/// The host guarantees `[ptr, ptr+len)` is a valid, initialized request
/// buffer it wrote via `alloc`.
#[no_mangle]
pub unsafe extern "C" fn agentbbs_plugin(ptr: i32, len: i32) -> i64 {
    // Confirm we are talking to a compatible host.
    if abi_version() != 1 {
        return reply(false, "incompatible host ABI", "null");
    }

    let req = core::slice::from_raw_parts(ptr as *const u8, len as usize);
    let req = match core::str::from_utf8(req) {
        Ok(s) => s,
        Err(_) => return reply(false, "request was not valid UTF-8", "null"),
    };

    let kind = json_get_str(req, "kind").unwrap_or_default();
    host_log("example-plugin invoked");

    match kind.as_str() {
        "echo" => {
            // Echo the raw args value back as data.
            let args = json_get_raw(req, "args").unwrap_or_else(|| String::from("null"));
            reply(true, "echo", &args)
        }
        "uppercase" => {
            let text = json_get_str_in(req, "args", "text").unwrap_or_default();
            let mut up = String::new();
            for c in text.chars() {
                up.extend(c.to_uppercase());
            }
            reply_owned(true, &up, "null")
        }
        _ => reply(false, "unknown command", "null"),
    }
}

// ---------------------------------------------------------------------------
// Response construction.
// ---------------------------------------------------------------------------

fn reply(ok: bool, text: &str, data_raw: &str) -> i64 {
    reply_owned(ok, text, data_raw)
}

fn reply_owned(ok: bool, text: &str, data_raw: &str) -> i64 {
    let mut s = String::from("{\"ok\":");
    s.push_str(if ok { "true" } else { "false" });
    s.push_str(",\"text\":");
    json_push_string(&mut s, text);
    s.push_str(",\"data\":");
    s.push_str(data_raw);
    s.push('}');

    let bytes = s.into_bytes();
    let out_ptr = bytes.as_ptr() as u32;
    let out_len = bytes.len() as u32;
    // Leak so the host can read it back; bump allocator never frees anyway.
    core::mem::forget(bytes);
    (((out_ptr as u64) << 32) | (out_len as u64)) as i64
}

// ---------------------------------------------------------------------------
// A deliberately tiny JSON reader. Only enough to pull a few fields out of the
// well-formed requests the host sends. Not a general JSON parser.
// ---------------------------------------------------------------------------

/// Find `"key":"value"` and return the (unescaped-enough) string value.
fn json_get_str(json: &str, key: &str) -> Option<String> {
    let raw = json_get_raw(json, key)?;
    parse_json_string(&raw)
}

/// Find `args` object then a string field inside it.
fn json_get_str_in(json: &str, obj_key: &str, field: &str) -> Option<String> {
    let obj = json_get_raw(json, obj_key)?;
    let inner = json_get_raw(&obj, field)?;
    parse_json_string(&inner)
}

/// Return the raw JSON text of the value for `key` (string, object, etc.).
fn json_get_raw(json: &str, key: &str) -> Option<String> {
    let mut needle = String::from('"');
    needle.push_str(key);
    needle.push_str("\":");
    let idx = json.find(needle.as_str())? + needle.len();
    let rest = &json[idx..];
    let rest = rest.trim_start();
    let bytes = rest.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let end = match bytes[0] {
        b'"' => {
            // string: find closing quote (no escaped-quote handling needed for tests)
            let mut i = 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i + 1
        }
        b'{' | b'[' => {
            let (open, close) = if bytes[0] == b'{' { (b'{', b'}') } else { (b'[', b']') };
            let mut depth = 0i32;
            let mut i = 0;
            loop {
                if i >= bytes.len() {
                    return None;
                }
                if bytes[i] == open {
                    depth += 1;
                } else if bytes[i] == close {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                i += 1;
            }
            i
        }
        _ => {
            // number / bool / null: read until , } ] or whitespace
            let mut i = 0;
            while i < bytes.len()
                && !matches!(bytes[i], b',' | b'}' | b']' | b' ' | b'\n' | b'\t' | b'\r')
            {
                i += 1;
            }
            i
        }
    };
    Some(String::from(&rest[..end]))
}

/// Parse a JSON string literal (with surrounding quotes) into its contents.
fn parse_json_string(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let bytes = raw.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'"' {
        return None;
    }
    let mut out = String::new();
    let inner = &raw[1..raw.len() - 1];
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

/// Append a JSON-escaped string literal (with quotes) to `out`.
fn json_push_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
}

// Silence unused warning for Vec import used transitively by String.
#[allow(dead_code)]
fn _keep(_: Vec<u8>) {}
