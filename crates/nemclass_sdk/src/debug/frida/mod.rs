//! Frida-backed debugger/instrumentation backend (feature `frida`).
//!
//! STUB — to be implemented on top of the `frida` crate (frida-gum/core):
//! `Interceptor` for function hooks, `Stalker` / `MemoryAccessMonitor` for access
//! tracing, and memory read/write through Frida. Enabling this feature pulls the
//! `frida` crate, whose build downloads the frida-gum devkit (needs network).

use crate::debug::{BpId, DebugEvent, Debugger, Registers, ThreadId, WatchKind, WatchSize};
use crate::error::{Result, SdkError};
use std::time::Duration;

const TODO: SdkError = SdkError::Unsupported("frida backend not yet implemented");

/// Frida-backed [`Debugger`].
pub struct FridaDebugger {
    #[allow(dead_code)]
    pid: u32,
}

impl FridaDebugger {
    /// Attaches Frida to `pid`.
    pub fn attach(pid: u32) -> Result<Self> {
        let _ = pid;
        Err(TODO)
    }
}

impl Debugger for FridaDebugger {
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
