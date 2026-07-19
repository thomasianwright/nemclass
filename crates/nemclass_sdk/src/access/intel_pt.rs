//! Intel Processor Trace "find what accesses" tracer (feature `intel-pt`).
//!
//! STUB — to be implemented as raw `libc` FFI: `perf_event_open` on the
//! `intel_pt` PMU for the target, mmap the AUX ring, and decode PT packets to
//! reconstruct the control flow reaching the access. Runtime-gated: returns
//! [`SdkError::Unsupported`] without PT-capable hardware / perf permission.

use crate::access::{AccessRecord, AccessTracer};
use crate::debug::{WatchKind, WatchSize};
use crate::error::{Result, SdkError};

const TODO: SdkError = SdkError::Unsupported("intel-pt access tracer not yet implemented");

/// Intel PT-based access tracer.
pub struct IntelPtTracer {
    #[allow(dead_code)]
    pid: u32,
}

impl IntelPtTracer {
    /// Attaches to `pid` via `perf_event_open` on the Intel PT PMU.
    pub fn attach(pid: u32) -> Result<Self> {
        let _ = pid;
        Err(TODO)
    }
}

impl AccessTracer for IntelPtTracer {
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
