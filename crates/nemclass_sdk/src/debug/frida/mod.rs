//! Frida-backed debugger/instrumentation backend (feature `frida`).
//!
//! This backend drives a target through the `frida` crate (frida-gum/core)
//! rather than raw `ptrace`. A small JavaScript agent is injected into the
//! target and exposes `rpc.exports` for memory read/write, thread enumeration,
//! and `Interceptor` breakpoint management. Each `Interceptor` hook buffers a
//! hit record inside the agent; [`Debugger::wait`] drains those buffered hits
//! through an RPC poll and turns them into [`DebugEvent`]s.
//!
//! ### Why RPC-drain (and not `send()` + a `ScriptHandler`)
//! In `frida` 0.17 a `Message::Send`'s `payload` deserializes into a *fixed*
//! `SendPayload` schema (`type`/`id`/`result`/`returns`), so an arbitrary
//! `send({t:"bp", …})` object does not round-trip into the handler — it lands in
//! `Message::Other`. Buffering hits in the agent and polling them via
//! `rpc.exports` sidesteps that entirely and keeps the wire contract simple.
//!
//! ### Why an owner thread (the `Send` problem)
//! The `Debugger` trait requires `Send`, but `frida::Script` holds
//! `Rc<RefCell<…>>` and raw pointers, and `frida::Session` holds a raw pointer —
//! both are `!Send`, and the crate adds no `unsafe impl Send`. So the frida
//! handles cannot live directly inside a `Send` type. We confine them to a
//! dedicated **owner thread**: it performs `attach`, owns the `Session`/`Script`
//! for their whole lifetime, and services commands over channels.
//! [`FridaDebugger`] itself holds only `Send` channel endpoints plus scalar
//! bookkeeping, so it is `Send` without any `unsafe` impls and without touching
//! the trait. All frida FFI thus happens on a single, fixed thread — which is
//! also what frida-gum wants.
//!
//! ## Semantics vs. the ptrace backend
//! Frida does not stop the world: the target keeps running and our hooks fire on
//! its own threads. So [`Debugger::cont`] is a semantic no-op (the target is
//! never suspended by us), "software breakpoints" are inline `Interceptor` hooks
//! (not real `int3` patches), and register access is only meaningful *inside* a
//! hook (captured from the hit's `CpuContext`). Hardware watchpoints,
//! single-step and register writes are not offered by this backend and return
//! [`SdkError::Unsupported`].
//!
//! ## Lifetimes
//! `Session`/`Script`/`Device` each borrow their parent in the frida crate. We
//! leak the top of the chain — the process-wide [`frida::Frida`] handle and its
//! [`frida::DeviceManager`] — to `'static`. `Frida::obtain()` is documented as a
//! once-per-process runtime init, so leaking it is correct and idiomatic; every
//! child then derives a `'static` lifetime and is storable on the owner thread.
//!
//! ## Runtime prerequisites
//! Local `attach(pid)` uses the in-process gum injected via `ptrace` under the
//! hood — no separate `frida-server` is required (that is only for remote/USB
//! devices). It still needs permission to inject: same-user process, or root /
//! `CAP_SYS_PTRACE`, and a sufficiently relaxed
//! `/proc/sys/kernel/yama/ptrace_scope`. Target architecture must match.

use crate::debug::{
    BpId, DebugEvent, Debugger, Registers, StopReason, ThreadId, WatchKind, WatchSize,
};
use crate::error::{Result, SdkError};

use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::OnceLock;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use frida::{DeviceManager, Frida, Message, Script, ScriptHandler, ScriptOption, ScriptRuntime, Session};
use serde_json::{json, Value};

/// Minimal script message handler.
///
/// The agent buffers breakpoint hits and we drain them via the `pollBp` RPC, so
/// non-RPC messages are ignored. But connecting *a* handler is REQUIRED:
/// `Script::exports.call` delivers its reply through the script's "message"
/// signal, so without `handle_message` connected every RPC blocks forever on its
/// internal reply channel. A ZST handler also makes the frida crate's non-RPC
/// dispatch path (which reinterprets the handler pointer) a harmless no-op.
struct RpcHandler;

impl ScriptHandler for RpcHandler {
    fn on_message(&mut self, _message: Message, _data: Option<Vec<u8>>) {}
}

// GLib main-loop functions from the statically-linked frida devkit. A running
// GMainLoop on frida's main context is REQUIRED for async RPC replies:
// `Script::exports.call` posts the request asynchronously and then blocks on its
// reply channel; the reply is only delivered while a loop iterates the context.
// The devkit bundles a private glib whose symbols are prefixed `_frida_` to avoid
// clashing with a system glib, so we link against the prefixed names.
extern "C" {
    fn frida_get_main_context() -> *mut std::ffi::c_void;
    #[link_name = "_frida_g_main_loop_new"]
    fn g_main_loop_new(context: *mut std::ffi::c_void, is_running: i32) -> *mut std::ffi::c_void;
    #[link_name = "_frida_g_main_loop_run"]
    fn g_main_loop_run(l: *mut std::ffi::c_void);
    #[link_name = "_frida_g_main_loop_quit"]
    fn g_main_loop_quit(l: *mut std::ffi::c_void);
    #[link_name = "_frida_g_main_loop_unref"]
    fn g_main_loop_unref(l: *mut std::ffi::c_void);
}

/// A `*mut GMainLoop`, sendable to the loop-runner thread. The pointer is only
/// used with the thread-safe `g_main_loop_*` calls.
#[derive(Clone, Copy)]
struct GLoop(*mut std::ffi::c_void);
unsafe impl Send for GLoop {}

impl GLoop {
    /// Runs the loop (blocks until quit). Takes `self` by value so a spawning
    /// closure captures the whole `Send` wrapper, not the bare pointer field.
    fn run(self) {
        unsafe { g_main_loop_run(self.0) }
    }
}

/// The injected agent (QuickJS). Exposes `rpc.exports` for memory/thread ops and
/// `Interceptor`-based breakpoints.
///
/// Wire formats (all values are JSON, driven through `script.exports.call`):
/// - `readMem(addrStr, len)` -> `Array` of byte values (decoded as ints).
/// - `writeMem(addrStr, [bytes])` -> number of bytes written.
/// - `enumThreads()` -> `Array` of thread ids.
/// - `addBp(addrStr)` -> integer listener id.
/// - `delBp(id)` -> `true`.
/// - `pollBp()` -> the oldest buffered hit `{id, addr, tid, ctx:{rip,rsp,…}}` or
///   `null` when the buffer is empty.
///
/// Register values are marshalled as decimal strings so 64-bit quantities
/// survive JSON without precision loss.
const AGENT_SRC: &str = r#"
'use strict';
const listeners = {};
const hits = [];
let nextId = 1;

function ctxToRegs(ctx) {
    const names = ['rax','rbx','rcx','rdx','rsi','rdi','rbp','rsp','rip',
                   'r8','r9','r10','r11','r12','r13','r14','r15'];
    const out = {};
    for (const n of names) {
        try { if (ctx[n] !== undefined) out[n] = ctx[n].toString(); }
        catch (e) { /* register not present on this arch */ }
    }
    return out;
}

// Every export is wrapped in try/catch and returns a sentinel on failure rather
// than throwing: a thrown error makes frida send an *error*-form RPC reply (a
// 7-element array) that the frida crate's deserializer can't parse, so the reply
// is never routed to the call() channel and the caller hangs. Returning a normal
// value keeps every reply in the parseable 4-element "ok" form.
//
// Uses the modern NativePointer methods (`ptr(x).readByteArray(len)`); the old
// `Memory.readByteArray(ptr, len)` was removed from frida-gum.
rpc.exports = {
    readMem: function (addrStr, len) {
        try {
            const buf = ptr(addrStr).readByteArray(len);
            return buf ? Array.from(new Uint8Array(buf)) : null;
        } catch (e) { return null; }
    },
    writeMem: function (addrStr, bytes) {
        try {
            ptr(addrStr).writeByteArray(bytes);
            return bytes.length;
        } catch (e) { return 0; }
    },
    enumThreads: function () {
        try {
            return Process.enumerateThreads().map(function (t) { return t.id; });
        } catch (e) { return []; }
    },
    addBp: function (addrStr) {
        try {
            const id = nextId++;
            listeners[id] = Interceptor.attach(ptr(addrStr), {
                onEnter: function (args) {
                    hits.push({
                        id: id,
                        addr: addrStr,
                        tid: this.threadId,
                        ctx: ctxToRegs(this.context)
                    });
                }
            });
            return id;
        } catch (e) { return null; }
    },
    delBp: function (id) {
        try {
            const l = listeners[id];
            if (l) { l.detach(); delete listeners[id]; }
        } catch (e) {}
        return true;
    },
    pollBp: function () {
        return hits.length ? hits.shift() : null;
    }
};
"#;

/// A command sent to the frida owner thread. All payloads are `Send`.
enum Cmd {
    /// Call an agent `rpc.exports` function; reply with the JSON result.
    Rpc {
        name: &'static str,
        args: Option<Value>,
        reply: Sender<Result<Option<Value>>>,
    },
    /// Unload the agent, detach the session, and terminate the owner thread.
    Detach { reply: Sender<Result<()>> },
}

/// Frida-backed [`Debugger`].
///
/// Holds only `Send` state: the command channel to the owner thread (which owns
/// the `!Send` frida handles), the owner thread's join handle, and scalar
/// breakpoint bookkeeping. This is what makes the type satisfy the trait's
/// `Send` bound without any `unsafe` impls.
pub struct FridaDebugger {
    pid: u32,
    /// Command channel to the frida owner thread.
    tx: Sender<Cmd>,
    /// Owner-thread join handle (joined on `detach`).
    worker: Option<JoinHandle<()>>,
    /// `BpId` -> agent listener id, for `clear_breakpoint`.
    listeners: HashMap<u64, u64>,
    /// agent listener id -> `BpId`, to attribute polled hits to a handle.
    by_listener: HashMap<u64, u64>,
    /// `BpId` -> breakpoint address (bookkeeping).
    bps: HashMap<u64, usize>,
    /// Monotonic `BpId` allocator.
    next_bp: u64,
    /// Last-seen registers per thread (from the most recent drained hit).
    last_regs: HashMap<u32, Registers>,
}

/// Process-wide leaked `Frida` handle (`obtain()` is a once-per-process init).
///
/// Only the `Frida` handle is shared across threads via a static — it is a unit
/// struct, hence trivially `Send + Sync`. The `DeviceManager` (which holds a raw
/// pointer and is `!Send`/`!Sync`) is deliberately **not** placed in a static;
/// it is built as a local on the owner thread that consumes it, so no `!Sync`
/// value ever needs to cross the static boundary.
static FRIDA: OnceLock<&'static Frida> = OnceLock::new();

/// Returns the leaked, `'static` process-wide `Frida` handle.
fn frida() -> &'static Frida {
    FRIDA.get_or_init(|| {
        // SAFETY: `Frida::obtain()` initializes global C FFI state and is meant
        // to be called (at least) once per process; repeat calls are no-ops.
        Box::leak(Box::new(unsafe { Frida::obtain() }))
    })
}

/// Owns the `!Send` frida handles on a fixed thread and services [`Cmd`]s.
///
/// Runs `attach` + agent injection, reports the outcome over `ready`, then loops
/// on `cmds` until a [`Cmd::Detach`] (or a dropped sender) tears it down.
fn owner_thread(
    pid: u32,
    ready: Sender<Result<()>>,
    cmds: std::sync::mpsc::Receiver<Cmd>,
) {
    // --- attach + inject, all on this thread ---
    // The `DeviceManager` is `!Send`; build and keep it as a stack local here so
    // it lives for the whole thread and never crosses the static boundary. Its
    // parameter lifetime is left to inference (bound to this stack frame): it
    // borrows the leaked `'static` `Frida` (`'static: 'a` holds for any `'a`), so
    // `get_local_device(&'a self)` can borrow the local without demanding a
    // `'static` self-borrow.
    let manager = DeviceManager::obtain(frida());
    let device = match manager.get_local_device() {
        Ok(d) => d,
        Err(e) => {
            let _ = ready.send(Err(backend(format!("get_local_device: {e:?}"))));
            return;
        }
    };
    // No `'static` annotation: `create_script(&'a self)` ties the returned
    // `Script`'s lifetime to the `Session`'s own parameter, and `Session` here is
    // a stack local — inference binds both to this stack frame, which is exactly
    // as long as we need (they never leave the owner thread).
    let session = match device.attach(pid) {
        Ok(s) => s,
        Err(e) => {
            let _ = ready.send(Err(backend(format!("attach(pid={pid}): {e:?}"))));
            return;
        }
    };
    // NB: `ScriptOption::set_name` in frida 0.17.2 passes a non-NUL-terminated
    // `&str` to the C API (`name.as_ptr()`), so frida reads past the name into
    // adjacent memory and the resulting D-Bus message fails to decode
    // (ScriptCreationError). We don't need a custom name — omit it and let frida
    // auto-generate one.
    let mut opts = ScriptOption::new().set_runtime(ScriptRuntime::QJS);
    let mut script = match session.create_script(AGENT_SRC, &mut opts) {
        Ok(s) => s,
        Err(e) => {
            let _ = ready.send(Err(backend(format!("create_script: {e:?}"))));
            return;
        }
    };
    // Connect the "message" signal BEFORE using RPC — `exports.call` routes its
    // reply through this handler, so without it every RPC call hangs.
    if let Err(e) = script.handle_message(RpcHandler) {
        let _ = ready.send(Err(backend(format!("handle_message: {e:?}"))));
        return;
    }
    if let Err(e) = script.load() {
        let _ = ready.send(Err(backend(format!("script.load: {e:?}"))));
        return;
    }
    // Start a GMainLoop on frida's main context on its own thread so async RPC
    // replies dispatch while `exports.call` blocks. Only start it AFTER the
    // `_sync` setup above (which ran their own temporary loops); a persistent
    // loop must not iterate the context concurrently with a `_sync` call.
    let ctx = unsafe { frida_get_main_context() };
    let gloop = GLoop(unsafe { g_main_loop_new(ctx, 0) });
    let mut loop_thread = std::thread::Builder::new()
        .name(format!("frida-loop-{pid}"))
        .spawn(move || gloop.run())
        .ok();

    if ready.send(Ok(())).is_err() {
        let _ = frida_teardown(gloop, &script, &session, &mut loop_thread);
        return; // caller gave up before we finished attaching
    }

    // --- command loop ---
    while let Ok(cmd) = cmds.recv() {
        match cmd {
            Cmd::Rpc { name, args, reply } => {
                let res = script
                    .exports
                    .call(name, args)
                    .map_err(|e| backend(format!("rpc {name}: {e:?}")));
                let _ = reply.send(res);
            }
            Cmd::Detach { reply } => {
                let res = frida_teardown(gloop, &script, &session, &mut loop_thread);
                let _ = reply.send(res);
                return;
            }
        }
    }
    // Sender dropped without an explicit Detach.
    let _ = frida_teardown(gloop, &script, &session, &mut loop_thread);
}

/// Stops the RPC main loop (and joins its thread) before unloading the agent and
/// detaching the session — the `_sync` teardown must not run while the loop
/// iterates frida's main context.
fn frida_teardown(
    gloop: GLoop,
    script: &Script,
    session: &Session,
    loop_thread: &mut Option<JoinHandle<()>>,
) -> Result<()> {
    unsafe { g_main_loop_quit(gloop.0) };
    if let Some(t) = loop_thread.take() {
        let _ = t.join();
    }
    let _ = script.unload();
    let res = session
        .detach()
        .map_err(|e| backend(format!("session.detach: {e:?}")));
    unsafe { g_main_loop_unref(gloop.0) };
    res
}

impl FridaDebugger {
    /// Attaches Frida to `pid` on a dedicated owner thread and injects the agent.
    pub fn attach(pid: u32) -> Result<Self> {
        let (tx, cmds) = channel::<Cmd>();
        let (ready_tx, ready_rx) = channel::<Result<()>>();

        let worker = std::thread::Builder::new()
            .name(format!("frida-owner-{pid}"))
            .spawn(move || owner_thread(pid, ready_tx, cmds))
            .map_err(|e| backend(format!("spawn owner thread: {e}")))?;

        // Wait for attach + injection to complete (or fail) on the owner thread.
        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = worker.join();
                return Err(e);
            }
            Err(_) => {
                let _ = worker.join();
                return Err(backend("owner thread exited before ready".into()));
            }
        }

        Ok(Self {
            pid,
            tx,
            worker: Some(worker),
            listeners: HashMap::new(),
            by_listener: HashMap::new(),
            bps: HashMap::new(),
            next_bp: 1,
            last_regs: HashMap::new(),
        })
    }

    /// Calls an agent `rpc.exports` function on the owner thread and waits for
    /// the JSON result.
    fn rpc(&self, name: &'static str, args: Option<Value>) -> Result<Option<Value>> {
        let (reply, resp) = channel();
        self.tx
            .send(Cmd::Rpc { name, args, reply })
            .map_err(|_| backend("owner thread is gone".into()))?;
        resp.recv()
            .map_err(|_| backend("owner thread dropped rpc reply".into()))?
    }

    /// Polls one buffered breakpoint hit from the agent, if any.
    ///
    /// Records the hit's registers (keyed by tid) so a subsequent
    /// [`Debugger::registers`] for that thread can answer, and returns the
    /// [`DebugEvent`] the hit maps to.
    fn poll_hit(&mut self) -> Result<Option<DebugEvent>> {
        let ret = self.rpc("pollBp", None)?;
        let hit = match ret {
            None => return Ok(None),
            Some(v) if v.is_null() => return Ok(None),
            Some(v) => v,
        };
        let listener_id = hit.get("id").and_then(Value::as_u64).unwrap_or(0);
        let bp_id = self
            .by_listener
            .get(&listener_id)
            .copied()
            .unwrap_or(listener_id);
        let addr = hit
            .get("addr")
            .and_then(Value::as_str)
            .and_then(parse_uint)
            .unwrap_or(0) as usize;
        let tid = hit.get("tid").and_then(Value::as_u64).unwrap_or(0) as u32;
        if let Some(ctx) = hit.get("ctx") {
            self.last_regs.insert(tid, regs_from_ctx(ctx));
        }
        Ok(Some(DebugEvent {
            tid: ThreadId(tid),
            reason: StopReason::Breakpoint {
                id: BpId(bp_id),
                addr,
            },
        }))
    }
}

impl Debugger for FridaDebugger {
    fn pid(&self) -> u32 {
        self.pid
    }

    fn threads(&self) -> Result<Vec<ThreadId>> {
        // Reading the LWP list from procfs is cheap, needs no extra privilege,
        // and returns the same set of tids as the agent's `enumThreads` — and it
        // works from `&self` without round-tripping to the owner thread.
        let dir = format!("/proc/{}/task", self.pid);
        let entries = std::fs::read_dir(&dir).map_err(|e| backend(format!("read {dir}: {e}")))?;
        let mut out = Vec::new();
        for entry in entries.flatten() {
            if let Some(tid) = entry.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) {
                out.push(ThreadId(tid));
            }
        }
        out.sort();
        Ok(out)
    }

    fn set_sw_breakpoint(&mut self, addr: usize) -> Result<BpId> {
        let ret = self.rpc("addBp", Some(json!([format!("{addr:#x}")])))?;
        let listener_id = ret
            .as_ref()
            .and_then(Value::as_u64)
            .ok_or_else(|| backend(format!("addBp({addr:#x}): agent returned no listener id")))?;

        let bp_id = self.next_bp;
        self.next_bp += 1;
        self.bps.insert(bp_id, addr);
        self.listeners.insert(bp_id, listener_id);
        self.by_listener.insert(listener_id, bp_id);
        Ok(BpId(bp_id))
    }

    fn set_hw_breakpoint(
        &mut self,
        _addr: usize,
        _size: WatchSize,
        _kind: WatchKind,
    ) -> Result<BpId> {
        Err(SdkError::Unsupported(
            "frida backend: hardware breakpoints/watchpoints are not supported",
        ))
    }

    fn clear_breakpoint(&mut self, id: BpId) -> Result<()> {
        let listener_id = self
            .listeners
            .remove(&id.0)
            .ok_or_else(|| SdkError::Debug(format!("unknown breakpoint id {}", id.0)))?;
        self.bps.remove(&id.0);
        self.by_listener.remove(&listener_id);
        self.rpc("delBp", Some(json!([listener_id])))?;
        Ok(())
    }

    fn cont(&mut self) -> Result<()> {
        // No-op: under Frida the target is never suspended by us — it runs freely
        // and hooks fire on its own threads. There is nothing to resume.
        Ok(())
    }

    fn step(&mut self, _tid: ThreadId) -> Result<()> {
        Err(SdkError::Unsupported(
            "frida backend: single-step is not supported",
        ))
    }

    fn wait(&mut self, timeout: Option<Duration>) -> Result<DebugEvent> {
        let deadline = timeout.map(|d| Instant::now() + d);
        loop {
            if let Some(ev) = self.poll_hit()? {
                return Ok(ev);
            }
            if let Some(deadline) = deadline {
                if Instant::now() >= deadline {
                    return Err(SdkError::Debug("timeout".into()));
                }
            }
            // Hooks buffer hits concurrently on the target's own threads; sleep
            // briefly between polls to avoid a busy-spin.
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn registers(&self, tid: ThreadId) -> Result<Registers> {
        self.last_regs
            .get(&tid.0)
            .copied()
            .ok_or(SdkError::Unsupported(
                "frida backend: registers are only available at a breakpoint hit",
            ))
    }

    fn set_registers(&mut self, _tid: ThreadId, _regs: &Registers) -> Result<()> {
        Err(SdkError::Unsupported(
            "frida backend: register writes are not supported",
        ))
    }

    fn read_mem(&self, addr: usize, buf: &mut [u8]) -> Result<usize> {
        let ret = self.rpc("readMem", Some(json!([format!("{addr:#x}"), buf.len()])))?;
        let arr = ret
            .as_ref()
            .and_then(Value::as_array)
            .ok_or(SdkError::Access { address: addr })?;
        let n = arr.len().min(buf.len());
        for (dst, v) in buf.iter_mut().zip(arr.iter()).take(n) {
            *dst = v.as_u64().unwrap_or(0) as u8;
        }
        Ok(n)
    }

    fn write_mem(&mut self, addr: usize, buf: &[u8]) -> Result<usize> {
        let bytes: Vec<Value> = buf.iter().map(|b| json!(*b as u64)).collect();
        let ret = self.rpc(
            "writeMem",
            Some(json!([format!("{addr:#x}"), Value::Array(bytes)])),
        )?;
        let written = ret
            .as_ref()
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(buf.len());
        Ok(written.min(buf.len()))
    }

    fn detach(mut self: Box<Self>) -> Result<()> {
        let (reply, resp) = channel();
        // If the owner thread is already gone, treat detach as a no-op success.
        if self.tx.send(Cmd::Detach { reply }).is_ok() {
            let res = resp
                .recv()
                .unwrap_or_else(|_| Err(backend("owner thread dropped detach reply".into())));
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            return res;
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        Ok(())
    }
}

impl Drop for FridaDebugger {
    fn drop(&mut self) {
        // Ensure the owner thread tears down even if `detach()` was never called.
        let (reply, resp) = channel();
        if self.tx.send(Cmd::Detach { reply }).is_ok() {
            let _ = resp.recv();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Builds a `frida` [`SdkError::Backend`] from a reason string.
fn backend(reason: String) -> SdkError {
    SdkError::Backend {
        name: "frida",
        reason,
    }
}

/// Parses a `0x`-prefixed or decimal unsigned integer string (as emitted by the
/// agent's `NativePointer.toString()` / decimal marshalling).
fn parse_uint(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u64>().ok()
    }
}

/// Maps a JS `CpuContext` (decimal/hex string fields) onto [`Registers`].
/// Missing fields default to `0`.
fn regs_from_ctx(ctx: &Value) -> Registers {
    let get = |name: &str| -> u64 {
        ctx.get(name)
            .and_then(Value::as_str)
            .and_then(parse_uint)
            .or_else(|| ctx.get(name).and_then(Value::as_u64))
            .unwrap_or(0)
    };
    Registers {
        r15: get("r15"),
        r14: get("r14"),
        r13: get("r13"),
        r12: get("r12"),
        rbp: get("rbp"),
        rbx: get("rbx"),
        r11: get("r11"),
        r10: get("r10"),
        r9: get("r9"),
        r8: get("r8"),
        rax: get("rax"),
        rcx: get("rcx"),
        rdx: get("rdx"),
        rsi: get("rsi"),
        rdi: get("rdi"),
        orig_rax: 0,
        rip: get("rip"),
        cs: 0,
        eflags: 0,
        rsp: get("rsp"),
        ss: 0,
        fs_base: 0,
        gs_base: 0,
        ds: 0,
        es: 0,
        fs: 0,
        gs: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_and_decimal_uints() {
        assert_eq!(parse_uint("0x10"), Some(16));
        assert_eq!(parse_uint("42"), Some(42));
        assert_eq!(parse_uint("  0xff "), Some(255));
        assert_eq!(parse_uint("nope"), None);
    }

    #[test]
    fn maps_cpu_context_to_registers() {
        let ctx = json!({
            "rip": "0x1000",
            "rsp": "0x7fffffffe000",
            "rax": "1234",
        });
        let regs = regs_from_ctx(&ctx);
        assert_eq!(regs.ip(), 0x1000);
        assert_eq!(regs.sp(), 0x7fffffffe000);
        assert_eq!(regs.rax, 1234);
        assert_eq!(regs.rbx, 0);
    }

    #[test]
    fn type_is_present_and_nonzero_sized() {
        // References the concrete backend type so it must compile under `frida`.
        assert!(std::mem::size_of::<FridaDebugger>() > 0);
        fn assert_unsupported(e: SdkError) {
            assert!(matches!(e, SdkError::Unsupported(_)));
        }
        assert_unsupported(SdkError::Unsupported(
            "frida backend: single-step is not supported",
        ));
    }

    /// Compile-time proof that the backend satisfies the trait's `Send` bound
    /// (which is what the `Box<dyn Debugger>` coercion in `attach` requires).
    #[allow(dead_code)]
    fn assert_send<T: Send>() {}
    #[allow(dead_code)]
    fn frida_debugger_is_send() {
        assert_send::<FridaDebugger>();
    }
}
