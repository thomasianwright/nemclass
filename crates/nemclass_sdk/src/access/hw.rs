//! Hardware-watchpoint "find what accesses" tracer (feature `debug-ptrace`).
//!
//! STUB — to be implemented on top of `super::super::debug::ptrace`: program a
//! DR0–DR3 watchpoint via ptrace, and on each trap capture RIP + registers,
//! aggregate by instruction address, single-step off the watch, and continue.

use crate::access::{AccessRecord, AccessTracer};
use crate::debug::{WatchKind, WatchSize};
use crate::error::{Result, SdkError};

const TODO: SdkError = SdkError::Unsupported("hardware access tracer not yet implemented");

/// Hardware-debug-register access tracer.
pub struct HwAccessTracer {
    #[allow(dead_code)]
    pid: u32,
}

impl HwAccessTracer {
    /// Attaches to `pid` (opens its own ptrace attach).
    pub fn attach(pid: u32) -> Result<Self> {
        let _ = pid;
        Err(TODO)
    }
}

impl AccessTracer for HwAccessTracer {
    fn start(&mut self, _addr: usize, _size: WatchSize, _kind: WatchKind) -> Result<()> {
        Err(TODO)
    }
    fn poll(&mut self) -> Result<Vec<AccessRecord>> {
        Err(TODO)
    }
    fn stop(&mut self) -> Result<()> {
        Err(TODO)
    }
}
