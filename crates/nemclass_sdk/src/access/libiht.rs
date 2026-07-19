//! LibIHT (Last Branch Record) "find what accesses" tracer (feature `libiht`).
//!
//! STUB — to be implemented as raw `libc` FFI to the LibIHT kernel module: open
//! its char device, `ioctl` to enable LBR capture around the target, and decode
//! the branch records to attribute the accessing branch source. Runtime-gated:
//! returns [`SdkError::Unsupported`] when the module is not loaded.

use crate::access::{AccessRecord, AccessTracer};
use crate::debug::{WatchKind, WatchSize};
use crate::error::{Result, SdkError};

const TODO: SdkError = SdkError::Unsupported("libiht access tracer not yet implemented");

/// LibIHT LBR-based access tracer.
pub struct LibIhtTracer {
    #[allow(dead_code)]
    pid: u32,
}

impl LibIhtTracer {
    /// Attaches to `pid` via the LibIHT device.
    pub fn attach(pid: u32) -> Result<Self> {
        let _ = pid;
        Err(TODO)
    }
}

impl AccessTracer for LibIhtTracer {
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
