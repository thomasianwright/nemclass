//! Debugger control layer — the pluggable backend boundary.
//!
//! [`Target`](crate::target::Target) is the *memory* layer (read/write/scan via
//! `process_vm_readv`, no special privilege). This module is the *control* layer:
//! breakpoints, registers, single-step and hardware watchpoints, which on Linux
//! require `ptrace`. The two coexist on the same PID — a [`Debugger`] does not
//! replace a `Target`, it augments it.
//!
//! The [`Debugger`] trait is the swap point: new engines (native ptrace, Frida,
//! …) implement it and are selected at runtime through [`BackendKind`] /
//! [`attach`]. Everything above this trait (the value scanner, cheat tables, the
//! "find what accesses" tracers) is backend-agnostic.

use crate::error::{Result, SdkError};
use std::time::Duration;

#[cfg(all(target_os = "linux", feature = "debug-ptrace"))]
pub mod ptrace;

#[cfg(feature = "frida")]
pub mod frida;

/// A thread id (Linux LWP / TID).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThreadId(pub u32);

impl ThreadId {
    /// The raw thread id.
    pub fn raw(self) -> u32 {
        self.0
    }
}

/// An opaque, backend-assigned breakpoint/watchpoint handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BpId(pub u64);

/// What kind of access a hardware breakpoint/watchpoint should trap on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchKind {
    /// Trap on instruction execution at the address.
    Execute,
    /// Trap on writes to the watched range.
    Write,
    /// Trap on reads or writes to the watched range (x86 has no read-only trap).
    ReadWrite,
}

/// The length of a hardware watchpoint range, in bytes (x86 DR7 `LEN` encoding).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchSize {
    /// 1 byte.
    B1,
    /// 2 bytes.
    B2,
    /// 4 bytes.
    B4,
    /// 8 bytes.
    B8,
}

impl WatchSize {
    /// The size in bytes.
    pub fn bytes(self) -> usize {
        match self {
            WatchSize::B1 => 1,
            WatchSize::B2 => 2,
            WatchSize::B4 => 4,
            WatchSize::B8 => 8,
        }
    }

    /// The smallest `WatchSize` covering `bytes` (rounded up, capped at 8).
    pub fn for_len(bytes: usize) -> WatchSize {
        match bytes {
            0 | 1 => WatchSize::B1,
            2 => WatchSize::B2,
            3..=4 => WatchSize::B4,
            _ => WatchSize::B8,
        }
    }
}

/// x86-64 general-purpose register file, field-compatible with
/// `libc::user_regs_struct` for cheap conversion in the ptrace backend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct Registers {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rax: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub orig_rax: u64,
    pub rip: u64,
    pub cs: u64,
    pub eflags: u64,
    pub rsp: u64,
    pub ss: u64,
    pub fs_base: u64,
    pub gs_base: u64,
    pub ds: u64,
    pub es: u64,
    pub fs: u64,
    pub gs: u64,
}

impl Registers {
    /// The instruction pointer (`rip`).
    pub fn ip(&self) -> usize {
        self.rip as usize
    }

    /// The stack pointer (`rsp`).
    pub fn sp(&self) -> usize {
        self.rsp as usize
    }
}

/// Why the target stopped, as reported by [`Debugger::wait`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// A software breakpoint was hit at `addr`.
    Breakpoint {
        /// The breakpoint handle.
        id: BpId,
        /// The address that trapped.
        addr: usize,
    },
    /// A hardware watchpoint fired for `addr`.
    Watchpoint {
        /// The watchpoint handle.
        id: BpId,
        /// The watched address that trapped.
        addr: usize,
    },
    /// A single-step completed.
    SingleStep,
    /// The target stopped on a signal we don't attribute to our breakpoints.
    Signal(i32),
    /// The target exited with the given status code.
    Exited(i32),
    /// A new thread was created (LWP).
    ThreadCreated(ThreadId),
    /// The stop could not be attributed to a specific cause.
    Unknown,
}

/// A stop event: which thread stopped and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebugEvent {
    /// The thread that stopped.
    pub tid: ThreadId,
    /// Why it stopped.
    pub reason: StopReason,
}

/// The pluggable debugger control interface.
///
/// Implementors provide breakpoint, register, stepping and memory control for a
/// single attached process. This is the boundary new backends implement; the
/// scanner, cheat tables and access tracers are written against it, not against
/// any concrete backend.
pub trait Debugger: Send {
    /// The attached process id.
    fn pid(&self) -> u32;

    /// The threads (LWPs) currently known for the target.
    fn threads(&self) -> Result<Vec<ThreadId>>;

    /// Sets a software (int3) breakpoint at `addr`.
    fn set_sw_breakpoint(&mut self, addr: usize) -> Result<BpId>;

    /// Sets a hardware breakpoint/watchpoint on `[addr, addr+size)` for `kind`.
    fn set_hw_breakpoint(&mut self, addr: usize, size: WatchSize, kind: WatchKind) -> Result<BpId>;

    /// Removes a previously-set breakpoint/watchpoint.
    fn clear_breakpoint(&mut self, id: BpId) -> Result<()>;

    /// Resumes all stopped threads.
    fn cont(&mut self) -> Result<()>;

    /// Single-steps one thread.
    fn step(&mut self, tid: ThreadId) -> Result<()>;

    /// Waits for the next stop event, optionally bounded by `timeout`.
    ///
    /// A `None` timeout blocks until an event arrives. A `Some(_)` that elapses
    /// with no event is reported as [`SdkError::Debug`] with a `"timeout"`
    /// message so callers can distinguish it.
    fn wait(&mut self, timeout: Option<Duration>) -> Result<DebugEvent>;

    /// Reads a thread's register file.
    fn registers(&self, tid: ThreadId) -> Result<Registers>;

    /// Writes a thread's register file.
    fn set_registers(&mut self, tid: ThreadId, regs: &Registers) -> Result<()>;

    /// Reads memory into `buf`; returns the number of bytes read.
    fn read_mem(&self, addr: usize, buf: &mut [u8]) -> Result<usize>;

    /// Writes `buf` to memory; returns the number of bytes written.
    fn write_mem(&mut self, addr: usize, buf: &[u8]) -> Result<usize>;

    /// Detaches from the target, restoring any patched bytes.
    fn detach(self: Box<Self>) -> Result<()>;
}

/// Selects which [`Debugger`] backend [`attach`] should build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// Native Linux `ptrace` debugger (feature `debug-ptrace`).
    Ptrace,
    /// Frida-backed debugger/instrumentation (feature `frida`).
    Frida,
}

/// Attaches a [`Debugger`] of the requested backend to `pid`.
///
/// Returns [`SdkError::Unsupported`] when the chosen backend is not compiled in.
/// This factory + `Box<dyn Debugger>` is what makes new backends pluggable: a
/// caller picks a [`BackendKind`] at runtime and never names a concrete type.
pub fn attach(pid: u32, kind: BackendKind) -> Result<Box<dyn Debugger>> {
    match kind {
        BackendKind::Ptrace => {
            #[cfg(all(target_os = "linux", feature = "debug-ptrace"))]
            {
                Ok(Box::new(ptrace::PtraceDebugger::attach(pid)?))
            }
            #[cfg(not(all(target_os = "linux", feature = "debug-ptrace")))]
            {
                let _ = pid;
                Err(SdkError::Unsupported(
                    "ptrace backend not compiled in (enable feature `debug-ptrace` on Linux)",
                ))
            }
        }
        BackendKind::Frida => {
            #[cfg(feature = "frida")]
            {
                Ok(Box::new(frida::FridaDebugger::attach(pid)?))
            }
            #[cfg(not(feature = "frida"))]
            {
                let _ = pid;
                Err(SdkError::Unsupported(
                    "frida backend not compiled in (enable feature `frida`)",
                ))
            }
        }
    }
}
