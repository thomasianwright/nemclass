//! "Find out what accesses this address" — the CheatEngine-style feature.
//!
//! A caller picks a watched address and an [`AccessBackend`]; the returned
//! [`AccessTracer`] records the distinct instructions that read/write/execute it,
//! each with a register snapshot. Backends differ only in *how* they observe the
//! access:
//!
//! - [`hw`] — x86 debug registers (DR0–DR7) driven over `ptrace`. Exact, always
//!   available with the default features, but intrusive (one HW slot per watch,
//!   single ptrace tracer per process).
//! - [`libiht`] — Last Branch Records via the LibIHT kernel module. Lightweight,
//!   attributes the accessing branch source; needs the module loaded.
//! - [`intel_pt`] — full control-flow reconstruction via `perf_event_open` with
//!   the Intel PT PMU; needs PT-capable hardware and perf permission.

use crate::debug::{Registers, WatchKind, WatchSize};
use crate::error::Result;
// `SdkError` is only referenced by the `#[cfg(not(...))]` "not compiled in" arms
// of `find_what_accesses`, so it is unused when every backend feature is enabled.
#[allow(unused_imports)]
use crate::error::SdkError;

#[cfg(all(target_os = "linux", feature = "debug-ptrace"))]
pub mod hw;

#[cfg(all(target_os = "linux", feature = "libiht"))]
pub mod libiht;

#[cfg(all(target_os = "linux", feature = "intel-pt"))]
pub mod intel_pt;

/// One instruction observed accessing the watched address.
#[derive(Debug, Clone)]
pub struct AccessRecord {
    /// Address of the instruction that performed the access.
    pub insn_addr: usize,
    /// Register snapshot at the moment of the access (best-effort; a zeroed
    /// [`Registers`] when the backend cannot recover a full context).
    pub regs: Registers,
    /// How many times this instruction has hit the watch so far.
    pub hits: u64,
}

/// A running "what accesses this address" observation.
///
/// Backends implement this; [`find_what_accesses`] hands back a boxed one so
/// callers stay backend-agnostic.
pub trait AccessTracer: Send {
    /// Begins watching `[addr, addr+size)` for accesses of `kind`.
    fn start(&mut self, addr: usize, size: WatchSize, kind: WatchKind) -> Result<()>;

    /// Returns the accessing instructions discovered so far, aggregated by
    /// instruction address (so `hits` accumulates across polls).
    fn poll(&mut self) -> Result<Vec<AccessRecord>>;

    /// Stops the trace and releases backend resources.
    fn stop(&mut self) -> Result<()>;
}

/// Which observation mechanism [`find_what_accesses`] should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessBackend {
    /// x86 hardware debug registers via `ptrace`.
    Hardware,
    /// LibIHT Last Branch Records.
    LibIht,
    /// Intel Processor Trace.
    IntelPt,
}

/// Builds an [`AccessTracer`] for `pid` using `backend`.
///
/// Returns [`SdkError::Unsupported`] when the backend is not compiled in or is
/// unavailable on this host. Call [`AccessTracer::start`] on the result to begin.
///
/// Note: the [`AccessBackend::Hardware`] tracer opens its own `ptrace` attach, so
/// it cannot be combined with a separate [`Debugger`](crate::debug::Debugger)
/// attached to the same process — use that debugger's hardware-breakpoint API
/// directly in that case.
pub fn find_what_accesses(pid: u32, backend: AccessBackend) -> Result<Box<dyn AccessTracer>> {
    match backend {
        AccessBackend::Hardware => {
            #[cfg(all(target_os = "linux", feature = "debug-ptrace"))]
            {
                Ok(Box::new(hw::HwAccessTracer::attach(pid)?))
            }
            #[cfg(not(all(target_os = "linux", feature = "debug-ptrace")))]
            {
                let _ = pid;
                Err(SdkError::Unsupported(
                    "hardware access tracer not compiled in (enable `debug-ptrace` on Linux)",
                ))
            }
        }
        AccessBackend::LibIht => {
            #[cfg(all(target_os = "linux", feature = "libiht"))]
            {
                Ok(Box::new(libiht::LibIhtTracer::attach(pid)?))
            }
            #[cfg(not(all(target_os = "linux", feature = "libiht")))]
            {
                let _ = pid;
                Err(SdkError::Unsupported(
                    "libiht access tracer not compiled in (enable feature `libiht` on Linux)",
                ))
            }
        }
        AccessBackend::IntelPt => {
            #[cfg(all(target_os = "linux", feature = "intel-pt"))]
            {
                Ok(Box::new(intel_pt::IntelPtTracer::attach(pid)?))
            }
            #[cfg(not(all(target_os = "linux", feature = "intel-pt")))]
            {
                let _ = pid;
                Err(SdkError::Unsupported(
                    "intel-pt access tracer not compiled in (enable feature `intel-pt` on Linux)",
                ))
            }
        }
    }
}
