//! Native Linux `ptrace` debugger backend (feature `debug-ptrace`).
//!
//! `PTRACE_SEIZE`-attaches to every thread of a process and provides software
//! (int3) breakpoints, hardware breakpoints/watchpoints (DR0–DR3 + DR7), register
//! access, single-step, continue, and `waitpid`-based event decoding. Bulk memory
//! goes through `process_vm_readv/writev`; code patching falls back to
//! `PTRACE_POKEDATA` for read-only pages. x86-64 Linux only.

use crate::debug::{
    BpId, DebugEvent, Debugger, Registers, StopReason, ThreadId, WatchKind, WatchSize,
};
use crate::error::{Result, SdkError};

use std::collections::{HashMap, HashSet};
use std::io::{IoSlice, IoSliceMut};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::ptrace;
use nix::sys::signal::Signal;
use nix::sys::uio::{process_vm_readv, process_vm_writev, RemoteIoVec};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;

pub mod regs;
use regs::{dr7_clear, dr7_set};

fn perr(op: &'static str, e: Errno) -> SdkError {
    SdkError::Ptrace { op, errno: e as i32 }
}

/// Byte offset of debug register `idx` within `struct user`.
fn dr_offset(idx: usize) -> usize {
    core::mem::offset_of!(libc::user, u_debugreg) + idx * 8
}

/// Raw `PTRACE_POKEUSER` write of a debug register.
unsafe fn pokeuser(tid: i32, off: usize, val: u64) -> Result<()> {
    *libc::__errno_location() = 0;
    libc::ptrace(
        libc::PTRACE_POKEUSER,
        tid,
        off as *mut libc::c_void,
        val as usize as *mut libc::c_void,
    );
    let errno = *libc::__errno_location();
    if errno != 0 {
        return Err(SdkError::Ptrace { op: "POKEUSER", errno });
    }
    Ok(())
}

/// Raw `PTRACE_PEEKUSER` read of a debug register.
unsafe fn peekuser(tid: i32, off: usize) -> Result<i64> {
    *libc::__errno_location() = 0;
    let r = libc::ptrace(
        libc::PTRACE_PEEKUSER,
        tid,
        off as *mut libc::c_void,
        std::ptr::null_mut::<libc::c_void>(),
    );
    let errno = *libc::__errno_location();
    if errno != 0 {
        return Err(SdkError::Ptrace { op: "PEEKUSER", errno });
    }
    Ok(r as i64)
}

/// Reads the thread ids of `pid` from `/proc/<pid>/task`.
fn read_tids(pid: u32) -> Result<Vec<i32>> {
    let dir = format!("/proc/{pid}/task");
    let rd = std::fs::read_dir(&dir).map_err(|e| SdkError::Debug(format!("read {dir}: {e}")))?;
    let mut tids = Vec::new();
    for e in rd.flatten() {
        if let Some(t) = e.file_name().to_str().and_then(|n| n.parse::<i32>().ok()) {
            tids.push(t);
        }
    }
    Ok(tids)
}

#[derive(Clone, Copy)]
struct HwBp {
    id: u64,
    addr: usize,
    kind: WatchKind,
    size: WatchSize,
}

/// Native ptrace-backed [`Debugger`].
pub struct PtraceDebugger {
    pid: u32,
    tids: Vec<i32>,
    sw_bps: HashMap<u64, (usize, u8)>,
    hw_bps: [Option<HwBp>; 4],
    next_id: u64,
    /// A non-breakpoint signal to redeliver to a thread on the next `cont`.
    pending: Option<(i32, i32)>,
    /// Threads currently stopped under our control.
    stopped: HashSet<i32>,
}

impl PtraceDebugger {
    /// Attaches to `pid`, seizing every thread.
    pub fn attach(pid: u32) -> Result<Self> {
        let tids = read_tids(pid)?;
        if tids.is_empty() {
            return Err(SdkError::Debug(format!("no threads for pid {pid}")));
        }
        for &tid in &tids {
            ptrace::seize(Pid::from_raw(tid), ptrace::Options::PTRACE_O_TRACECLONE)
                .map_err(|e| perr("SEIZE", e))?;
        }
        let mut dbg = Self {
            pid,
            tids,
            sw_bps: HashMap::new(),
            hw_bps: [None; 4],
            next_id: 1,
            pending: None,
            stopped: HashSet::new(),
        };
        // Interrupt the main thread and drain its group-stop so we start stopped.
        let main = Pid::from_raw(pid as i32);
        ptrace::interrupt(main).map_err(|e| perr("INTERRUPT", e))?;
        let _ = waitpid(main, Some(WaitPidFlag::__WALL));
        dbg.stopped.insert(pid as i32);
        Ok(dbg)
    }

    fn compute_dr7(&self) -> u64 {
        let mut dr7 = 0u64;
        for (i, b) in self.hw_bps.iter().enumerate() {
            if let Some(b) = b {
                dr7 = dr7_set(dr7, i, b.kind, b.size);
            } else {
                dr7 = dr7_clear(dr7, i);
            }
        }
        dr7
    }

    /// (Re)programs DR0–DR3 addresses and DR7 on every attached thread.
    /// Best-effort per thread — a thread that is momentarily unstoppable is
    /// skipped rather than failing the whole call.
    fn program_hw(&self) {
        let dr7 = self.compute_dr7();
        for &tid in &self.tids {
            for (i, b) in self.hw_bps.iter().enumerate() {
                let addr = b.map(|b| b.addr as u64).unwrap_or(0);
                unsafe {
                    let _ = pokeuser(tid, dr_offset(i), addr);
                }
            }
            unsafe {
                let _ = pokeuser(tid, dr_offset(7), dr7);
            }
        }
    }

    fn read_at(&self, addr: usize, buf: &mut [u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let len = buf.len();
        let pid = Pid::from_raw(self.pid as i32);
        {
            let remote = [RemoteIoVec { base: addr, len }];
            let mut local = [IoSliceMut::new(&mut *buf)];
            if let Ok(n) = process_vm_readv(pid, &mut local, &remote) {
                return Ok(n);
            }
        }
        self.read_peek(addr, buf)
    }

    fn read_peek(&self, addr: usize, buf: &mut [u8]) -> Result<usize> {
        let pid = Pid::from_raw(self.pid as i32);
        let word = std::mem::size_of::<libc::c_long>();
        let mut done = 0;
        while done < buf.len() {
            let cur = addr + done;
            let aligned = cur & !(word - 1);
            let val = ptrace::read(pid, aligned as ptrace::AddressType)
                .map_err(|_| SdkError::Access { address: cur })?;
            let bytes = (val as i64).to_ne_bytes();
            let off = cur - aligned;
            let n = (word - off).min(buf.len() - done);
            buf[done..done + n].copy_from_slice(&bytes[off..off + n]);
            done += n;
        }
        Ok(done)
    }

    fn write_at(&self, addr: usize, buf: &[u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let len = buf.len();
        let pid = Pid::from_raw(self.pid as i32);
        {
            let remote = [RemoteIoVec { base: addr, len }];
            let local = [IoSlice::new(buf)];
            if let Ok(n) = process_vm_writev(pid, &local, &remote) {
                return Ok(n);
            }
        }
        self.write_poke(addr, buf)
    }

    fn write_poke(&self, addr: usize, buf: &[u8]) -> Result<usize> {
        let pid = Pid::from_raw(self.pid as i32);
        let word = std::mem::size_of::<libc::c_long>();
        let mut done = 0;
        while done < buf.len() {
            let cur = addr + done;
            let aligned = cur & !(word - 1);
            let off = cur - aligned;
            let n = (word - off).min(buf.len() - done);
            let old = ptrace::read(pid, aligned as ptrace::AddressType)
                .map_err(|_| SdkError::Access { address: cur })?;
            let mut bytes = (old as i64).to_ne_bytes();
            bytes[off..off + n].copy_from_slice(&buf[done..done + n]);
            let newval = i64::from_ne_bytes(bytes);
            ptrace::write(pid, aligned as ptrace::AddressType, newval as _)
                .map_err(|_| SdkError::Access { address: cur })?;
            done += n;
        }
        Ok(done)
    }
}

impl Debugger for PtraceDebugger {
    fn pid(&self) -> u32 {
        self.pid
    }

    fn threads(&self) -> Result<Vec<ThreadId>> {
        Ok(read_tids(self.pid)?
            .into_iter()
            .map(|t| ThreadId(t as u32))
            .collect())
    }

    fn set_sw_breakpoint(&mut self, addr: usize) -> Result<BpId> {
        let mut orig = [0u8; 1];
        self.read_at(addr, &mut orig)?;
        self.write_at(addr, &[0xCC])?;
        let id = self.next_id;
        self.next_id += 1;
        self.sw_bps.insert(id, (addr, orig[0]));
        Ok(BpId(id))
    }

    fn set_hw_breakpoint(&mut self, addr: usize, size: WatchSize, kind: WatchKind) -> Result<BpId> {
        let slot = (0..4)
            .find(|&i| self.hw_bps[i].is_none())
            .ok_or_else(|| SdkError::Debug("no free debug register".into()))?;
        let id = self.next_id;
        self.next_id += 1;
        self.hw_bps[slot] = Some(HwBp { id, addr, kind, size });
        self.program_hw();
        Ok(BpId(id))
    }

    fn clear_breakpoint(&mut self, id: BpId) -> Result<()> {
        if let Some((addr, orig)) = self.sw_bps.remove(&id.0) {
            let _ = self.write_at(addr, &[orig]);
            return Ok(());
        }
        if let Some(slot) = (0..4).find(|&i| self.hw_bps[i].map(|b| b.id) == Some(id.0)) {
            self.hw_bps[slot] = None;
            self.program_hw();
            return Ok(());
        }
        Err(SdkError::Debug("unknown breakpoint".into()))
    }

    fn cont(&mut self) -> Result<()> {
        let tids: Vec<i32> = self.stopped.drain().collect();
        for tid in tids {
            let sig = match self.pending {
                Some((t, s)) if t == tid => {
                    self.pending = None;
                    Signal::try_from(s).ok()
                }
                _ => None,
            };
            match ptrace::cont(Pid::from_raw(tid), sig) {
                Ok(_) | Err(Errno::ESRCH) => {}
                Err(e) => return Err(perr("CONT", e)),
            }
        }
        Ok(())
    }

    fn step(&mut self, tid: ThreadId) -> Result<()> {
        let t = tid.raw() as i32;
        ptrace::step(Pid::from_raw(t), None).map_err(|e| perr("STEP", e))?;
        self.stopped.remove(&t);
        Ok(())
    }

    fn wait(&mut self, timeout: Option<Duration>) -> Result<DebugEvent> {
        let deadline = timeout.map(|d| Instant::now() + d);
        loop {
            let flags = if timeout.is_some() {
                WaitPidFlag::WNOHANG | WaitPidFlag::__WALL
            } else {
                WaitPidFlag::__WALL
            };
            let status = waitpid(Pid::from_raw(-1), Some(flags)).map_err(|e| perr("WAITPID", e))?;
            match status {
                WaitStatus::StillAlive => {
                    if deadline.is_some_and(|dl| Instant::now() >= dl) {
                        return Err(SdkError::Debug("timeout".into()));
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                WaitStatus::Exited(pid, code) => {
                    let t = pid.as_raw();
                    self.tids.retain(|&x| x != t);
                    self.stopped.remove(&t);
                    return Ok(DebugEvent {
                        tid: ThreadId(t as u32),
                        reason: StopReason::Exited(code),
                    });
                }
                WaitStatus::Signaled(pid, sig, _) => {
                    let t = pid.as_raw();
                    self.tids.retain(|&x| x != t);
                    self.stopped.remove(&t);
                    return Ok(DebugEvent {
                        tid: ThreadId(t as u32),
                        reason: StopReason::Signal(sig as i32),
                    });
                }
                WaitStatus::PtraceEvent(pid, _sig, ev) => {
                    if ev == libc::PTRACE_EVENT_CLONE {
                        let new = ptrace::getevent(pid).unwrap_or(0) as i32;
                        if new != 0 && !self.tids.contains(&new) {
                            self.tids.push(new);
                        }
                        let _ = ptrace::cont(pid, None);
                        return Ok(DebugEvent {
                            tid: ThreadId(new as u32),
                            reason: StopReason::ThreadCreated(ThreadId(new as u32)),
                        });
                    }
                    let _ = ptrace::cont(pid, None);
                }
                WaitStatus::Stopped(pid, sig) => {
                    let t = pid.as_raw();
                    self.stopped.insert(t);
                    if sig == Signal::SIGTRAP {
                        // Hardware watchpoint? DR6 bits 0..3 flag which slot fired.
                        if let Ok(dr6) = unsafe { peekuser(t, dr_offset(6)) } {
                            for slot in 0..4 {
                                if (dr6 & (1 << slot)) != 0 {
                                    if let Some(b) = self.hw_bps[slot] {
                                        unsafe {
                                            let _ = pokeuser(t, dr_offset(6), 0);
                                        }
                                        return Ok(DebugEvent {
                                            tid: ThreadId(t as u32),
                                            reason: StopReason::Watchpoint {
                                                id: BpId(b.id),
                                                addr: b.addr,
                                            },
                                        });
                                    }
                                }
                            }
                        }
                        // Software breakpoint? rip is one past the int3.
                        if let Ok(regs) = ptrace::getregs(Pid::from_raw(t)) {
                            let bp_addr = (regs.rip as usize).wrapping_sub(1);
                            let hit = self
                                .sw_bps
                                .iter()
                                .find(|(_, (a, _))| *a == bp_addr)
                                .map(|(&id, &(addr, orig))| (id, addr, orig));
                            if let Some((id, addr, orig)) = hit {
                                // Restore, rewind, step over, re-arm.
                                let _ = self.write_at(addr, &[orig]);
                                let mut r2 = regs;
                                r2.rip = addr as u64;
                                let _ = ptrace::setregs(Pid::from_raw(t), r2);
                                let _ = ptrace::step(Pid::from_raw(t), None);
                                let _ = waitpid(Pid::from_raw(t), Some(WaitPidFlag::__WALL));
                                let _ = self.write_at(addr, &[0xCC]);
                                return Ok(DebugEvent {
                                    tid: ThreadId(t as u32),
                                    reason: StopReason::Breakpoint { id: BpId(id), addr },
                                });
                            }
                        }
                        return Ok(DebugEvent {
                            tid: ThreadId(t as u32),
                            reason: StopReason::SingleStep,
                        });
                    } else {
                        // Redeliver real signals on the next cont, but swallow the
                        // initial SIGSTOP a freshly-cloned thread receives.
                        if sig != Signal::SIGSTOP {
                            self.pending = Some((t, sig as i32));
                        }
                        return Ok(DebugEvent {
                            tid: ThreadId(t as u32),
                            reason: StopReason::Signal(sig as i32),
                        });
                    }
                }
                WaitStatus::Continued(_) => {}
                #[allow(unreachable_patterns)]
                _ => {}
            }
        }
    }

    fn registers(&self, tid: ThreadId) -> Result<Registers> {
        let r = ptrace::getregs(Pid::from_raw(tid.raw() as i32)).map_err(|e| perr("GETREGS", e))?;
        Ok(r.into())
    }

    fn set_registers(&mut self, tid: ThreadId, regs: &Registers) -> Result<()> {
        ptrace::setregs(Pid::from_raw(tid.raw() as i32), regs.into()).map_err(|e| perr("SETREGS", e))
    }

    fn read_mem(&self, addr: usize, buf: &mut [u8]) -> Result<usize> {
        self.read_at(addr, buf)
    }

    fn write_mem(&mut self, addr: usize, buf: &[u8]) -> Result<usize> {
        self.write_at(addr, buf)
    }

    fn detach(self: Box<Self>) -> Result<()> {
        for &(addr, orig) in self.sw_bps.values() {
            let _ = self.write_at(addr, &[orig]);
        }
        for &tid in &self.tids {
            unsafe {
                let _ = pokeuser(tid, dr_offset(7), 0);
                for i in 0..4 {
                    let _ = pokeuser(tid, dr_offset(i), 0);
                }
            }
            match ptrace::detach(Pid::from_raw(tid), None) {
                Ok(_) | Err(Errno::ESRCH) => {}
                Err(e) => return Err(perr("DETACH", e)),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod live {
    use super::*;

    // A parent may always ptrace its own child (yama allows descendants), so this
    // exercises SEIZE + interrupt + process_vm_readv end-to-end. Skips cleanly
    // when `sleep` is missing or ptrace is otherwise denied.
    #[test]
    fn attach_child_reads_memory_and_detaches() {
        let mut child = match std::process::Command::new("sleep").arg("5").spawn() {
            Ok(c) => c,
            Err(_) => return,
        };
        let pid = child.id();
        let dbg = match PtraceDebugger::attach(pid) {
            Ok(d) => d,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
        };

        assert!(!dbg.threads().unwrap().is_empty());

        // Read 4 bytes from the first readable region in the child's map.
        let maps = std::fs::read_to_string(format!("/proc/{pid}/maps")).unwrap();
        let start_line = maps
            .lines()
            .find(|l| l.contains("r-x") || l.contains("r--"))
            .expect("a readable region");
        let start = usize::from_str_radix(start_line.split('-').next().unwrap(), 16).unwrap();
        let mut buf = [0u8; 4];
        assert_eq!(dbg.read_mem(start, &mut buf).unwrap(), 4);

        Box::new(dbg).detach().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
}
