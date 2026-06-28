//! # agentbbs-wasm
//!
//! A sandboxed WebAssembly **plugin host** for AgentBBS. Plugins extend the
//! BBS with board hooks, slash-commands, and agent tools — but they are
//! *untrusted* code. This crate runs them inside the pure-Rust [`wasmi`]
//! interpreter with strict resource limits, so a buggy or hostile plugin can
//! neither hang the node (fuel metering) nor reach outside the capabilities
//! its caller holds.
//!
//! The host enforces [`Caps::PLUGINS`] on every invocation and emits
//! [`EventKind::PluginInvoke`] reports (when a [`Reporter`] is attached) so a
//! sysop can audit plugin activity.
//!
//! ## Host ABI (version [`ABI_VERSION`])
//!
//! A plugin is a WebAssembly module that interoperates with the host through
//! a small, stable C-style ABI over linear memory.
//!
//! ### Exports a plugin MUST provide
//!
//! - `memory` — an exported linear memory named `memory`. All request and
//!   response bytes live here.
//! - `alloc(len: i32) -> i32` — allocate `len` bytes inside the guest and
//!   return a pointer to the start of the region. The host calls this to
//!   obtain a buffer into which it writes the request, and the guest uses the
//!   same allocator for its response buffer. Returning `0` signals
//!   out-of-memory.
//! - `agentbbs_plugin(ptr: i32, len: i32) -> i64` — the entry point. The host
//!   has written a UTF-8 JSON [`PluginRequest`] into guest memory at
//!   `[ptr, ptr + len)`. The plugin parses it, does its work, writes a UTF-8
//!   JSON [`PluginResponse`] somewhere in its memory, and returns an `i64`
//!   that **packs the response location**:
//!
//!   ```text
//!   return value = ((out_ptr as u32 as i64) << 32) | (out_len as u32 as i64)
//!   ```
//!
//!   i.e. the high 32 bits are the response pointer and the low 32 bits are
//!   the response length. The host reads `out_len` bytes at `out_ptr` and
//!   decodes them as a [`PluginResponse`]. See [`pack_ret`] / [`unpack_ret`].
//!
//! ### Imports the host provides (module `"agentbbs"`)
//!
//! - `log(ptr: i32, len: i32)` — log a UTF-8 string from guest memory at
//!   `[ptr, ptr + len)`. Captured by the host (see [`PluginHost::take_logs`]).
//! - `abi_version() -> i32` — returns [`ABI_VERSION`] so a plugin can confirm
//!   it is talking to a compatible host before doing anything.
//!
//! ## Resource limits
//!
//! Every invocation runs with a bounded **fuel** budget
//! ([`PluginHost::with_fuel`], default [`DEFAULT_FUEL`]). Each executed
//! instruction consumes fuel; when the budget is exhausted the interpreter
//! traps and [`PluginHost::invoke`] returns an [`Error`] rather than hanging.
//! This is what makes an infinite-loop plugin safe to load.
//!
//! ## Example
//!
//! A complete example plugin (commands `echo` and `uppercase`) lives under
//! `agentbbs-wasm/example-plugin/` as a standalone `cdylib` crate targeting
//! `wasm32-unknown-unknown`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wasmi::{Caller, Engine, Linker, Memory, Module, Store, TypedFunc};

use agentbbs_core::caps::{self, Caps};
use agentbbs_core::error::{Error, Result};
use agentbbs_core::report::{Event, EventKind, Reporter};

/// The host ABI version. Bumped on any breaking change to the calling
/// convention or to the request/response JSON shapes. Exposed to guests via
/// the imported `agentbbs::abi_version()` host function.
pub const ABI_VERSION: i32 = 1;

/// Default per-invocation fuel budget. Generous enough for real plugins, low
/// enough that a runaway loop terminates quickly. Roughly proportional to the
/// number of executed wasm instructions.
pub const DEFAULT_FUEL: u64 = 10_000_000;

/// A request handed to a plugin. Serialized to JSON and written into guest
/// memory before the plugin's entry point is called.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginRequest {
    /// The command / hook name the plugin should dispatch on (e.g. `"echo"`).
    pub kind: String,
    /// The board this request pertains to, if any (slug).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<String>,
    /// Arbitrary structured arguments for the command.
    #[serde(default)]
    pub args: serde_json::Value,
}

impl PluginRequest {
    /// Convenience constructor for a request with no board and the given args.
    pub fn new(kind: impl Into<String>, args: serde_json::Value) -> Self {
        PluginRequest {
            kind: kind.into(),
            board: None,
            args,
        }
    }
}

/// A response produced by a plugin. Decoded from JSON read back out of guest
/// memory after the plugin's entry point returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginResponse {
    /// Whether the plugin handled the request successfully.
    pub ok: bool,
    /// Human-readable text result (shown to the agent/operator).
    #[serde(default)]
    pub text: String,
    /// Arbitrary structured result payload.
    #[serde(default)]
    pub data: serde_json::Value,
}

/// Pack a `(ptr, len)` pair into the `i64` the plugin entry point returns.
///
/// The high 32 bits carry the pointer, the low 32 bits the length.
#[inline]
pub fn pack_ret(ptr: u32, len: u32) -> i64 {
    (((ptr as u64) << 32) | (len as u64)) as i64
}

/// Inverse of [`pack_ret`]: split a returned `i64` into `(ptr, len)`.
#[inline]
pub fn unpack_ret(packed: i64) -> (u32, u32) {
    let bits = packed as u64;
    ((bits >> 32) as u32, (bits & 0xffff_ffff) as u32)
}

/// Per-invocation host state held in the wasmi [`Store`]. Used by host
/// functions (e.g. `log`) to communicate back to the host.
#[derive(Default)]
struct HostState {
    /// Log lines emitted by the guest via `agentbbs::log`.
    logs: Vec<String>,
}

/// A loaded, instantiated, sandboxed plugin ready to be invoked.
///
/// Construct one with [`PluginHost::load_from_bytes`]. Each [`PluginHost`]
/// owns its own wasmi [`Store`] and instance; it is single-threaded by design
/// (`&mut self` on [`invoke`](PluginHost::invoke)).
pub struct PluginHost {
    store: Store<HostState>,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    entry: TypedFunc<(i32, i32), i64>,
    fuel: u64,
    reporter: Option<Arc<dyn Reporter>>,
}

impl PluginHost {
    /// Validate, compile, and instantiate a plugin from raw wasm bytes.
    ///
    /// Fuel metering is enabled on the engine and the required exports
    /// (`memory`, `alloc`, `agentbbs_plugin`) are resolved up front, so a
    /// structurally invalid plugin is rejected here rather than at call time.
    pub fn load_from_bytes(wasm: &[u8]) -> Result<PluginHost> {
        let mut config = wasmi::Config::default();
        // Enable fuel metering so we can bound execution per invocation.
        config.consume_fuel(true);
        let engine = Engine::new(&config);

        let module = Module::new(&engine, wasm)
            .map_err(|e| Error::malformed("wasm module", e))?;

        let mut store = Store::new(&engine, HostState::default());

        let mut linker: Linker<HostState> = Linker::new(&engine);
        Self::register_host_funcs(&mut linker)?;

        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| Error::Other(format!("plugin instantiation failed: {e}")))?
            .start(&mut store)
            .map_err(|e| Error::Other(format!("plugin start failed: {e}")))?;

        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| Error::malformed("wasm plugin", "missing exported `memory`"))?;

        let alloc = instance
            .get_typed_func::<i32, i32>(&store, "alloc")
            .map_err(|e| Error::malformed("wasm plugin", format!("bad `alloc` export: {e}")))?;

        let entry = instance
            .get_typed_func::<(i32, i32), i64>(&store, "agentbbs_plugin")
            .map_err(|e| {
                Error::malformed("wasm plugin", format!("bad `agentbbs_plugin` export: {e}"))
            })?;

        Ok(PluginHost {
            store,
            memory,
            alloc,
            entry,
            fuel: DEFAULT_FUEL,
            reporter: None,
        })
    }

    /// Register the imported host functions in module `"agentbbs"`.
    fn register_host_funcs(linker: &mut Linker<HostState>) -> Result<()> {
        linker
            .func_wrap(
                "agentbbs",
                "log",
                |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| {
                    let msg = read_guest_string(&mut caller, ptr, len);
                    caller.data_mut().logs.push(msg);
                },
            )
            .map_err(|e| Error::Other(format!("failed to define agentbbs::log: {e}")))?;

        linker
            .func_wrap("agentbbs", "abi_version", || -> i32 { ABI_VERSION })
            .map_err(|e| Error::Other(format!("failed to define agentbbs::abi_version: {e}")))?;

        Ok(())
    }

    /// Override the per-invocation fuel budget (default [`DEFAULT_FUEL`]).
    pub fn with_fuel(mut self, fuel: u64) -> Self {
        self.fuel = fuel;
        self
    }

    /// Attach a [`Reporter`] so each invocation emits a
    /// [`EventKind::PluginInvoke`] event.
    pub fn with_reporter(mut self, reporter: Arc<dyn Reporter>) -> Self {
        self.reporter = Some(reporter);
        self
    }

    /// Drain and return the log lines emitted by the guest so far.
    pub fn take_logs(&mut self) -> Vec<String> {
        std::mem::take(&mut self.store.data_mut().logs)
    }

    /// Invoke the plugin with `req`, enforcing capabilities and fuel.
    ///
    /// Requires the caller to hold [`Caps::PLUGINS`]; otherwise returns
    /// [`Error::PermissionDenied`]. On success returns the decoded
    /// [`PluginResponse`]. A plugin that loops forever exhausts its fuel and
    /// yields an [`Error`] rather than hanging.
    pub fn invoke(&mut self, caps: Caps, req: &PluginRequest) -> Result<PluginResponse> {
        caps::require(caps, Caps::PLUGINS, "PLUGINS")?;

        // Emit an audit event for this invocation (best-effort).
        if let Some(reporter) = &self.reporter {
            let _ = reporter.report(
                Event::now(EventKind::PluginInvoke, req.kind.clone()).with(serde_json::json!({
                    "board": req.board,
                })),
            );
        }

        // Reset the fuel budget for this invocation.
        self.store
            .set_fuel(self.fuel)
            .map_err(|e| Error::Other(format!("failed to set fuel: {e}")))?;

        // Serialize the request and copy it into guest memory.
        let payload = serde_json::to_vec(req)?;
        let len = i32::try_from(payload.len())
            .map_err(|_| Error::malformed("plugin request", "request too large"))?;

        let in_ptr = self
            .alloc
            .call(&mut self.store, len)
            .map_err(|e| map_trap("alloc", e))?;
        if in_ptr == 0 {
            return Err(Error::Other("plugin alloc returned null".into()));
        }

        self.memory
            .write(&mut self.store, in_ptr as usize, &payload)
            .map_err(|e| Error::Other(format!("failed to write request to guest memory: {e}")))?;

        // Call the entry point.
        let packed = self
            .entry
            .call(&mut self.store, (in_ptr, len))
            .map_err(|e| map_trap("agentbbs_plugin", e))?;

        let (out_ptr, out_len) = unpack_ret(packed);
        if out_len == 0 {
            return Err(Error::malformed(
                "plugin response",
                "plugin returned a zero-length response",
            ));
        }

        // Read the response bytes back out of guest memory.
        let mut buf = vec![0u8; out_len as usize];
        self.memory
            .read(&self.store, out_ptr as usize, &mut buf)
            .map_err(|e| {
                Error::malformed("plugin response", format!("out-of-bounds response: {e}"))
            })?;

        let resp: PluginResponse = serde_json::from_slice(&buf)?;
        Ok(resp)
    }
}

/// Read a UTF-8 string out of the caller's guest memory, lossily. Used by the
/// `log` host function, where a malformed pointer should not abort the guest.
fn read_guest_string(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> String {
    let memory = match caller.get_export("memory") {
        Some(wasmi::Extern::Memory(m)) => m,
        _ => return String::new(),
    };
    let data = memory.data(&*caller);
    let start = ptr as usize;
    let end = start.saturating_add(len.max(0) as usize);
    match data.get(start..end) {
        Some(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        None => String::new(),
    }
}

/// Map a wasmi call error into an [`Error`], distinguishing fuel exhaustion
/// (a runaway plugin) so it is recognizable to callers.
fn map_trap(func: &str, err: wasmi::Error) -> Error {
    let text = err.to_string();
    if text.contains("fuel") || text.contains("OutOfFuel") || text.contains("out of fuel") {
        Error::Other(format!(
            "plugin exceeded its fuel budget while running `{func}` (possible infinite loop)"
        ))
    } else {
        Error::Other(format!("plugin trap in `{func}`: {text}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal, hand-written guest module in WAT implementing the host ABI.
    ///
    /// Memory layout / behavior:
    /// - It exports `memory`, `alloc`, and `agentbbs_plugin`.
    /// - `alloc` is a trivial bump allocator starting at offset 1024 (the
    ///   first 1024 bytes are reserved as scratch for the canned response).
    /// - `agentbbs_plugin` ignores the request and returns a fixed JSON
    ///   response `{"ok":true,"text":"echo","data":null}` that is stored as a
    ///   data segment at offset 16. It also calls `agentbbs::log` once.
    const ECHO_WAT: &str = r#"
        (module
          (import "agentbbs" "log" (func $log (param i32 i32)))
          (import "agentbbs" "abi_version" (func $abi (result i32)))
          (memory (export "memory") 1)
          ;; The canned response JSON, placed at offset 16.
          (data (i32.const 16) "{\"ok\":true,\"text\":\"echo\",\"data\":null}")
          ;; A short log message at offset 200.
          (data (i32.const 200) "hello from guest")
          ;; Bump allocator pointer, starts at 1024.
          (global $next (mut i32) (i32.const 1024))
          (func $alloc (export "alloc") (param $len i32) (result i32)
            (local $p i32)
            (local.set $p (global.get $next))
            (global.set $next (i32.add (global.get $next) (local.get $len)))
            (local.get $p))
          (func $agentbbs_plugin (export "agentbbs_plugin")
              (param $ptr i32) (param $len i32) (result i64)
            ;; Touch abi_version so the import is exercised.
            (drop (call $abi))
            ;; Log a message.
            (call $log (i32.const 200) (i32.const 16))
            ;; Return packed (ptr=16, len=37). The JSON above is 37 bytes.
            (i64.or
              (i64.shl (i64.const 16) (i64.const 32))
              (i64.const 37)))
        )
    "#;

    /// A guest whose entry point loops forever — used to verify fuel limits.
    const LOOP_WAT: &str = r#"
        (module
          (memory (export "memory") 1)
          (global $next (mut i32) (i32.const 1024))
          (func $alloc (export "alloc") (param $len i32) (result i32)
            (local $p i32)
            (local.set $p (global.get $next))
            (global.set $next (i32.add (global.get $next) (local.get $len)))
            (local.get $p))
          (func $agentbbs_plugin (export "agentbbs_plugin")
              (param $ptr i32) (param $len i32) (result i64)
            (loop $forever
              (br $forever))
            (i64.const 0))
        )
    "#;

    fn echo_host() -> PluginHost {
        let wasm = wat::parse_str(ECHO_WAT).expect("ECHO_WAT should compile");
        PluginHost::load_from_bytes(&wasm).expect("echo plugin should load")
    }

    #[test]
    fn loads_valid_module_and_invokes() {
        let mut host = echo_host();
        let req = PluginRequest::new("echo", serde_json::json!({"msg": "hi"}));
        let resp = host
            .invoke(Caps::all(), &req)
            .expect("invoke should succeed");
        assert!(resp.ok);
        assert_eq!(resp.text, "echo");
        // The guest logged a line via the host `log` import.
        let logs = host.take_logs();
        assert_eq!(logs, vec!["hello from guest".to_string()]);
    }

    #[test]
    fn rejects_garbage_blob() {
        let garbage = b"this is definitely not a wasm module \x00\x01\x02";
        match PluginHost::load_from_bytes(garbage) {
            Ok(_) => panic!("garbage must be rejected"),
            Err(err) => assert!(matches!(err, Error::Malformed { .. }), "got: {err:?}"),
        }
    }

    #[test]
    fn requires_plugins_capability() {
        let mut host = echo_host();
        let req = PluginRequest::new("echo", serde_json::Value::Null);
        // Caps::default() is READ|POST|EDIT_OWN — no PLUGINS.
        let err = host
            .invoke(Caps::default(), &req)
            .expect_err("should be denied without PLUGINS");
        assert!(
            matches!(err, Error::PermissionDenied("PLUGINS")),
            "got: {err:?}"
        );
    }

    #[test]
    fn fuel_limit_terminates_runaway() {
        let wasm = wat::parse_str(LOOP_WAT).expect("LOOP_WAT should compile");
        let mut host = PluginHost::load_from_bytes(&wasm)
            .expect("loop plugin should load")
            .with_fuel(100_000);
        let req = PluginRequest::new("spin", serde_json::Value::Null);
        let err = host
            .invoke(Caps::PLUGINS, &req)
            .expect_err("infinite loop must be terminated by fuel exhaustion");
        // It must be an error, not a hang, and mention fuel.
        let msg = format!("{err}");
        assert!(
            msg.contains("fuel"),
            "expected a fuel-exhaustion error, got: {msg}"
        );
    }

    #[test]
    fn request_response_json_roundtrip() {
        let req = PluginRequest {
            kind: "uppercase".into(),
            board: Some("general".into()),
            args: serde_json::json!({"text": "hi"}),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: PluginRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);

        let resp = PluginResponse {
            ok: true,
            text: "HI".into(),
            data: serde_json::json!({"len": 2}),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: PluginResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, back);

        // board omitted when None.
        let bare = PluginRequest::new("echo", serde_json::Value::Null);
        let json = serde_json::to_string(&bare).unwrap();
        assert!(!json.contains("board"), "board should be skipped: {json}");
    }

    #[test]
    fn pack_unpack_roundtrip() {
        for (p, l) in [(0u32, 0u32), (16, 37), (0xdead_beef, 0x0102_0304)] {
            let (rp, rl) = unpack_ret(pack_ret(p, l));
            assert_eq!((rp, rl), (p, l));
        }
    }

    #[test]
    fn reporter_receives_plugin_invoke_event() {
        use agentbbs_core::report::MemoryReporter;
        let reporter = Arc::new(MemoryReporter::new(8));
        let mut host = echo_host().with_reporter(reporter.clone());
        let req = PluginRequest::new("echo", serde_json::Value::Null);
        host.invoke(Caps::PLUGINS, &req).unwrap();
        let events = reporter.snapshot();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::PluginInvoke);
        assert_eq!(events[0].subject, "echo");
    }
}
