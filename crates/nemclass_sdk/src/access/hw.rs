//! Hardware-watchpoint "find what accesses" tracer (feature `debug-ptrace`).
//!
//! Owns a [`PtraceDebugger`](crate::debug::ptrace::PtraceDebugger), programs a
//! DR0–DR3 watchpoint on the target address, and on each trap records the
//! accessing instruction (RIP) + registers, aggregating by instruction so `hits`
//! accumulates across [`AccessTracer::poll`] calls.

use crate::access::{AccessRecord, AccessTracer};
use crate::debug::ptrace::PtraceDebugger;
use crate::debug::{BpId, Debugger, Registers, StopReason, WatchKind, WatchSize};
use crate::error::{Result, SdkError};
use std::collections::HashMap;
use std::time::Duration;

/// Hardware-debug-register access tracer.
pub struct HwAccessTracer {
    /// `Option` so `stop()` can take ownership and honor `detach(self: Box<Self>)`.
    dbg: Option<PtraceDebugger>,
    /// insn_addr (RIP) -> (latest registers, hit count).
    records: HashMap<usize, (Registers, u64)>,
    bp: Option<BpId>,
}

impl HwAccessTracer {
    /// Attaches to `pid`, opening its own ptrace attach.
    pub fn attach(pid: u32) -> Result<Self> {
        Ok(Self {
            dbg: Some(PtraceDebugger::attach(pid)?),
            records: HashMap::new(),
            bp: None,
        })
    }
}

impl AccessTracer for HwAccessTracer {
    fn start(&mut self, addr: usize, size: WatchSize, kind: WatchKind) -> Result<()> {
        let d = self
            .dbg
            .as_mut()
            .ok_or_else(|| SdkError::Debug("tracer stopped".into()))?;
        self.bp = Some(d.set_hw_breakpoint(addr, size, kind)?);
        d.cont()
    }

    fn poll(&mut self) -> Result<Vec<AccessRecord>> {
        let d = self
            .dbg
            .as_mut()
            .ok_or_else(|| SdkError::Debug("tracer stopped".into()))?;
        loop {
            match d.wait(Some(Duration::from_millis(50))) {
                Ok(ev) => match ev.reason {
                    StopReason::Watchpoint { .. } => {
                        if let Ok(regs) = d.registers(ev.tid) {
                            let e = self.records.entry(regs.rip as usize).or_insert((regs, 0));
                            e.0 = regs;
                            e.1 += 1;
                        }
                        // Step off the watched instruction, consume the step trap,
                        // then resume.
                        let _ = d.step(ev.tid);
                        let _ = d.wait(Some(Duration::from_millis(50)));
                        let _ = d.cont();
                    }
                    StopReason::Exited(_) => break,
                    _ => {
                        let _ = d.cont();
                    }
                },
                // Timeout: return what we've gathered this round.
                Err(SdkError::Debug(m)) if m == "timeout" => break,
                Err(_) => break,
            }
        }

        Ok(self
            .records
            .iter()
            .map(|(&insn_addr, &(regs, hits))| AccessRecord { insn_addr, regs, hits })
            .collect())
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(mut d) = self.dbg.take() {
            if let Some(b) = self.bp.take() {
                let _ = d.clear_breakpoint(b);
            }
            let _ = Box::new(d).detach();
        }
        Ok(())
    }
}
