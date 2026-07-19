//! Native Linux `ptrace` debugger backend (feature `debug-ptrace`).
//!
//! STUB — to be implemented: `PTRACE_SEIZE` attach across all threads, software
//! (int3) breakpoints, hardware breakpoints/watchpoints via DR0–DR7, register
//! get/set, single-step, continue, and `waitpid`-based event decoding. Memory
//! reads/writes use `process_vm_readv/writev` (via `nix::sys::uio`) for bulk and
//! ptrace poke for code patching.

use crate::debug::{BpId, DebugEvent, Debugger, Registers, ThreadId, WatchKind, WatchSize};
use crate::error::{Result, SdkError};
use std::time::Duration;

pub mod regs;

const TODO: SdkError = SdkError::Unsupported("ptrace backend not yet implemented");

/// Native ptrace-backed [`Debugger`].
pub struct PtraceDebugger {
    #[allow(dead_code)]
    pid: u32,
}

impl PtraceDebugger {
    /// Attaches to `pid` via `ptrace`.
    pub fn attach(pid: u32) -> Result<Self> {
        let _ = pid;
        Err(TODO)
    }
}

impl Debugger for PtraceDebugger {
    fn pid(&self) -> u32 {
        self.pid
    }
    fn threads(&self) -> Result<Vec<ThreadId>> {
        Err(TODO)
    }
    fn set_sw_breakpoint(&mut self, _addr: usize) -> Result<BpId> {
        Err(TODO)
    }
    fn set_hw_breakpoint(&mut self, _addr: usize, _size: WatchSize, _kind: WatchKind) -> Result<BpId> {
        Err(TODO)
    }
    fn clear_breakpoint(&mut self, _id: BpId) -> Result<()> {
        Err(TODO)
    }
    fn cont(&mut self) -> Result<()> {
        Err(TODO)
    }
    fn step(&mut self, _tid: ThreadId) -> Result<()> {
        Err(TODO)
    }
    fn wait(&mut self, _timeout: Option<Duration>) -> Result<DebugEvent> {
        Err(TODO)
    }
    fn registers(&self, _tid: ThreadId) -> Result<Registers> {
        Err(TODO)
    }
    fn set_registers(&mut self, _tid: ThreadId, _regs: &Registers) -> Result<()> {
        Err(TODO)
    }
    fn read_mem(&self, _addr: usize, _buf: &mut [u8]) -> Result<usize> {
        Err(TODO)
    }
    fn write_mem(&mut self, _addr: usize, _buf: &[u8]) -> Result<usize> {
        Err(TODO)
    }
    fn detach(self: Box<Self>) -> Result<()> {
        Err(TODO)
    }
}
