//! LibIHT (Last Branch Record) "find what accesses" tracer (feature `libiht`).
//!
//! Raw `libc` FFI to the LibIHT Linux kernel module
//! (`github.com/libiht/libiht`, author Thomason Zhao). The module exposes a
//! procfs device `/proc/libiht-info` (mode 0666); all commands go through a
//! single `ioctl` number and are dispatched by the `cmd` field of the request
//! struct the kernel copies from userspace.
//!
//! ## ABI (from the module's C headers — do not invent)
//! - Device: `/proc/libiht-info`, opened `O_RDWR`. Absent ⇒ module not loaded ⇒
//!   we return [`SdkError::Unsupported`] (the clean runtime gate).
//! - ioctl number: `LIBIHT_LKM_IOCTL_BASE = _IO('l', 0) = 0x6C00` (classic
//!   `_IO(type,nr) = (type<<8)|nr`, no size/dir bits). The kernel ignores the
//!   number and switches on `xioctl_request.cmd`.
//! - `enum IOCTL`: `ENABLE_LBR=1, DISABLE_LBR=2, DUMP_LBR=3, CONFIG_LBR=4`.
//! - Structs mirrored below `#[repr(C)]`. `DUMP_LBR` has userspace pre-allocate
//!   `MAX_LBR_LIST_LEN = 32` `{from,to}` entries the kernel fills.
//!
//! ## Attribution
//! LBR records branch `{from,to}` pairs. LBR captures control flow, not the
//! access type, so `kind` is advisory; we attribute a branch whose `from`/`to`
//! touches the watched range to its source instruction (`insn_addr = from`).
//!
//! ## Runtime prerequisites
//! Intel CPU with LBR and the libiht LKM loaded (`insmod`, creating
//! `/proc/libiht-info`). Without it, the tracer returns `Unsupported`.

use crate::access::{AccessRecord, AccessTracer};
use crate::debug::{Registers, WatchKind, WatchSize};
use crate::error::{Result, SdkError};
use std::collections::HashMap;
use std::os::unix::io::RawFd;

const LIBIHT_DEVICE: &str = "/proc/libiht-info";

/// `_IO(type, nr)` — no size/direction bits, matching the module's macro.
const fn io(ty: u32, nr: u32) -> libc::c_ulong {
    ((ty << 8) | nr) as libc::c_ulong
}

/// `LIBIHT_LKM_IOCTL_BASE = _IO('l', 0)` = 0x6C00.
const LIBIHT_IOCTL_BASE: libc::c_ulong = io(b'l' as u32, 0);

// `enum IOCTL` ordinals (LBR arm).
const CMD_ENABLE_LBR: libc::c_int = 1;
const CMD_DISABLE_LBR: libc::c_int = 2;
const CMD_DUMP_LBR: libc::c_int = 3;

/// `MAX_LBR_LIST_LEN` — entries the kernel fills on a dump.
const MAX_LBR_ENTRIES: usize = 0x20;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct LbrStackEntry {
    from: u64,
    to: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LbrConfig {
    pid: u32,
    // 4 bytes implicit padding before the u64 (C layout).
    lbr_select: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LbrData {
    lbr_tos: u64,
    entries: *mut LbrStackEntry,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LbrIoctlRequest {
    lbr_config: LbrConfig,
    buffer: *mut LbrData,
}

// BTS arm — modelled only so the union (and thus `XioctlRequest`) matches the
// kernel struct size; we never issue BTS commands here.
#[repr(C)]
#[derive(Clone, Copy)]
struct BtsConfig {
    pid: u32,
    bts_config: u64,
    bts_buffer_size: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct BtsData {
    base: *mut u8,
    index: *mut u8,
    threshold: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct BtsIoctlRequest {
    bts_config: BtsConfig,
    buffer: *mut BtsData,
}

#[repr(C)]
union XioctlBody {
    lbr: LbrIoctlRequest,
    #[allow(dead_code)]
    bts: BtsIoctlRequest,
}

#[repr(C)]
struct XioctlRequest {
    cmd: libc::c_int,
    body: XioctlBody,
}

fn last_errno() -> i32 {
    unsafe { *libc::__errno_location() }
}

/// LibIHT LBR-based access tracer.
pub struct LibIhtTracer {
    fd: RawFd,
    pid: u32,
    watch: Option<(usize, usize)>,
    /// Stable dump buffer handed to the kernel (kept alive for the tracer's life).
    entries: Vec<LbrStackEntry>,
    hits: HashMap<usize, u64>,
    enabled: bool,
}

impl LibIhtTracer {
    /// Opens the LibIHT device for `pid`.
    pub fn attach(pid: u32) -> Result<Self> {
        let path = std::ffi::CString::new(LIBIHT_DEVICE).unwrap();
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDWR) };
        if fd < 0 {
            // Module absent / inaccessible is the normal case on generic hosts.
            return Err(SdkError::Unsupported("libiht kernel module not available"));
        }
        Ok(Self {
            fd,
            pid,
            watch: None,
            entries: vec![LbrStackEntry::default(); MAX_LBR_ENTRIES],
            hits: HashMap::new(),
            enabled: false,
        })
    }

    /// Issues one LBR command with a fresh request pointing at our dump buffer.
    fn lbr_ioctl(&mut self, cmd: libc::c_int) -> std::result::Result<(), i32> {
        let mut data = LbrData {
            lbr_tos: 0,
            entries: self.entries.as_mut_ptr(),
        };
        let mut req = XioctlRequest {
            cmd,
            body: XioctlBody {
                lbr: LbrIoctlRequest {
                    lbr_config: LbrConfig { pid: self.pid, lbr_select: 0 },
                    buffer: &mut data,
                },
            },
        };
        // SAFETY: `req` and the buffers it points at (stack `data`, heap
        // `self.entries`) outlive this synchronous ioctl.
        let r = unsafe { libc::ioctl(self.fd, LIBIHT_IOCTL_BASE, &mut req as *mut XioctlRequest) };
        if r != 0 {
            return Err(last_errno());
        }
        Ok(())
    }
}

impl AccessTracer for LibIhtTracer {
    fn start(&mut self, addr: usize, size: WatchSize, kind: WatchKind) -> Result<()> {
        let _ = kind; // LBR captures branches regardless of access type.
        self.watch = Some((addr, size.bytes()));
        self.lbr_ioctl(CMD_ENABLE_LBR).map_err(|e| SdkError::Backend {
            name: "libiht",
            reason: format!("ENABLE_LBR ioctl failed: errno {e}"),
        })?;
        self.enabled = true;
        Ok(())
    }

    fn poll(&mut self) -> Result<Vec<AccessRecord>> {
        let Some((addr, size)) = self.watch else {
            return Ok(vec![]);
        };
        if !self.enabled {
            return Ok(vec![]);
        }
        for e in &mut self.entries {
            *e = LbrStackEntry::default();
        }
        self.lbr_ioctl(CMD_DUMP_LBR).map_err(|e| SdkError::Backend {
            name: "libiht",
            reason: format!("DUMP_LBR ioctl failed: errno {e}"),
        })?;

        let in_range = |x: u64| (x as usize) >= addr && (x as usize) < addr + size;
        for e in &self.entries {
            if e.from == 0 && e.to == 0 {
                continue; // unfilled slot
            }
            if in_range(e.to) || in_range(e.from) {
                *self.hits.entry(e.from as usize).or_insert(0) += 1;
            }
        }

        Ok(self
            .hits
            .iter()
            .map(|(&insn_addr, &hits)| AccessRecord {
                insn_addr,
                regs: Registers::default(),
                hits,
            })
            .collect())
    }

    fn stop(&mut self) -> Result<()> {
        if self.enabled {
            let _ = self.lbr_ioctl(CMD_DISABLE_LBR);
            self.enabled = false;
        }
        if self.fd >= 0 {
            unsafe {
                libc::close(self.fd);
            }
            self.fd = -1;
        }
        Ok(())
    }
}

impl Drop for LibIhtTracer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_gates_cleanly_without_module() {
        // On a host without the libiht module this MUST be the Unsupported gate;
        // if the module is loaded, Ok is equally acceptable.
        match LibIhtTracer::attach(std::process::id()) {
            Ok(_) => {}
            Err(SdkError::Unsupported(_)) => {}
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    #[test]
    fn struct_layout_matches_abi() {
        assert_eq!(std::mem::size_of::<LbrStackEntry>(), 16);
        assert_eq!(std::mem::size_of::<LbrConfig>(), 16);
        // cmd(4) + pad(4) + body(union sized to the 32-byte BTS arm) = 40.
        assert_eq!(std::mem::size_of::<XioctlRequest>(), 40);
        assert_eq!(LIBIHT_IOCTL_BASE, 0x6C00);
    }
}
