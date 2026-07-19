//! Intel Processor Trace "find what accesses" tracer (feature `intel-pt`).
//!
//! Raw `libc` FFI: `perf_event_open` on the `intel_pt` PMU for the target, an
//! mmap'd data ring + AUX buffer, and a minimal PT packet decoder that recovers
//! flow-changing instruction pointers (TIP / FUP / TIP.PGE / TIP.PGD).
//!
//! ## "What accesses" — a control-flow approximation
//! PT emits **control-flow IPs**, not data addresses. Correlating an IP to the
//! data it touches needs side-band memory/instruction-image analysis this tracer
//! does not perform. So we surface the distinct control-flow instruction pointers
//! observed (with hit counts) — a best-effort, control-flow-level approximation
//! of "what runs around / reaches the watched address", not a precise data-access
//! attribution. For exact data-access attribution use the hardware-watchpoint
//! backend ([`super::hw`]).
//!
//! ## Runtime prerequisites
//! An Intel PT-capable CPU (the `intel_pt` PMU node present) and sufficient
//! `perf_event_paranoid` / `CAP_PERFMON`. Otherwise the tracer returns
//! [`SdkError::Unsupported`].

use crate::access::{AccessRecord, AccessTracer};
use crate::debug::{Registers, WatchKind, WatchSize};
use crate::error::{Result, SdkError};
use std::collections::HashMap;
use std::os::unix::io::RawFd;
use std::sync::atomic::{fence, Ordering};

// `_IO('$', n)` — no argument. x86 perf ioctls.
const PERF_EVENT_IOC_ENABLE: libc::c_ulong = 0x2400;
const PERF_EVENT_IOC_DISABLE: libc::c_ulong = 0x2401;

// perf_event_attr flag-word bits we set (bit0 disabled, bit5 exclude_kernel).
const ATTR_DISABLED: u64 = 1 << 0;
const ATTR_EXCLUDE_KERNEL: u64 = 1 << 5;

/// `#[repr(C)]` mirror of `perf_event_attr` (named explicitly to avoid libc
/// field-name drift). Only the fields we set are named meaningfully; the bitfield
/// word is modelled as a plain `u64` we OR flags into.
#[repr(C)]
#[derive(Clone, Copy)]
struct PtPerfAttr {
    type_: u32,
    size: u32,
    config: u64,
    sample_period_or_freq: u64,
    sample_type: u64,
    read_format: u64,
    flags: u64,
    wakeup: u32,
    bp_type: u32,
    config1: u64,
    config2: u64,
    branch_sample_type: u64,
    sample_regs_user: u64,
    sample_stack_user: u32,
    clockid: i32,
    sample_regs_intr: u64,
    aux_watermark: u32,
    sample_max_stack: u16,
    __reserved_2: u16,
    aux_sample_size: u32,
    __reserved_3: u32,
}

/// `#[repr(C)]` mirror of `perf_event_mmap_page`. The control fields the AUX ring
/// protocol needs (`aux_head`/`aux_tail`/`aux_offset`/`aux_size`) live at a fixed
/// 1 KiB offset, so we reserve the leading kilobyte opaquely.
#[repr(C)]
struct PtPerfMmapPage {
    _head: [u8; 1024],
    data_head: u64,
    data_tail: u64,
    data_offset: u64,
    data_size: u64,
    aux_head: u64,
    aux_tail: u64,
    aux_offset: u64,
    aux_size: u64,
}

fn last_errno() -> i32 {
    unsafe { *libc::__errno_location() }
}

/// Intel PT-based access tracer.
pub struct IntelPtTracer {
    pid: u32,
    fd: RawFd,
    base: *mut libc::c_void,
    base_len: usize,
    aux: *mut libc::c_void,
    aux_len: usize,
    meta: *mut PtPerfMmapPage,
    watched: Option<usize>,
    hits: HashMap<usize, u64>,
    enabled: bool,
}

// The mmap pointers are owned by this tracer and only used from the owning
// thread; the trait requires `Send`.
unsafe impl Send for IntelPtTracer {}

impl IntelPtTracer {
    /// The traced process id.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Attaches a PT trace to `pid`.
    pub fn attach(pid: u32) -> Result<Self> {
        let type_str = std::fs::read_to_string("/sys/bus/event_source/devices/intel_pt/type")
            .map_err(|_| SdkError::Unsupported("intel_pt PMU not available"))?;
        let pt_type: u32 = type_str
            .trim()
            .parse()
            .map_err(|_| SdkError::Unsupported("intel_pt PMU type unreadable"))?;

        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };

        let mut attr: PtPerfAttr = unsafe { std::mem::zeroed() };
        attr.type_ = pt_type;
        attr.size = std::mem::size_of::<PtPerfAttr>() as u32;
        attr.flags = ATTR_DISABLED | ATTR_EXCLUDE_KERNEL;

        // Per-thread trace on any CPU.
        let fd = unsafe {
            libc::syscall(
                libc::SYS_perf_event_open,
                &attr as *const PtPerfAttr,
                pid as libc::pid_t,
                -1i32,
                -1i32,
                0u64,
            )
        };
        if fd < 0 {
            let e = last_errno();
            if e == libc::EACCES || e == libc::EPERM {
                return Err(SdkError::Unsupported(
                    "intel_pt requires perf permissions (perf_event_paranoid)",
                ));
            }
            return Err(SdkError::Backend {
                name: "intel-pt",
                reason: format!("perf_event_open failed: errno {e}"),
            });
        }
        let fd = fd as RawFd;

        // Data ring: 1 header page + 2^n data pages.
        let data_pages = 8usize;
        let base_len = (1 + data_pages) * page_size;
        let base = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                base_len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if base == libc::MAP_FAILED {
            let e = last_errno();
            unsafe {
                libc::close(fd);
            }
            return Err(SdkError::Backend {
                name: "intel-pt",
                reason: format!("mmap data ring failed: errno {e}"),
            });
        }
        let meta = base as *mut PtPerfMmapPage;

        // AUX buffer placed right after the data ring.
        let aux_pages = 128usize;
        let aux_len = aux_pages * page_size;
        unsafe {
            std::ptr::write_volatile(std::ptr::addr_of_mut!((*meta).aux_offset), base_len as u64);
            std::ptr::write_volatile(std::ptr::addr_of_mut!((*meta).aux_size), aux_len as u64);
        }
        let aux = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                aux_len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                base_len as libc::off_t,
            )
        };
        if aux == libc::MAP_FAILED {
            let e = last_errno();
            unsafe {
                libc::munmap(base, base_len);
                libc::close(fd);
            }
            return Err(SdkError::Backend {
                name: "intel-pt",
                reason: format!("mmap aux failed: errno {e}"),
            });
        }

        Ok(Self {
            pid,
            fd,
            base,
            base_len,
            aux,
            aux_len,
            meta,
            watched: None,
            hits: HashMap::new(),
            enabled: false,
        })
    }

    /// Copies the unread `[aux_tail, aux_head)` slice of the AUX ring, handling
    /// wrap, and advances `aux_tail`.
    fn drain_aux(&mut self) -> Vec<u8> {
        let head = unsafe { std::ptr::read_volatile(std::ptr::addr_of!((*self.meta).aux_head)) };
        fence(Ordering::Acquire);
        let mut tail = unsafe { std::ptr::read_volatile(std::ptr::addr_of!((*self.meta).aux_tail)) };
        let size = self.aux_len as u64;
        let mut avail = head.wrapping_sub(tail);
        if avail == 0 {
            return Vec::new();
        }
        if avail > size {
            // Overflowed: keep only the most recent `size` bytes.
            tail = head - size;
            avail = size;
        }
        let mut data = vec![0u8; avail as usize];
        let start = (tail % size) as usize;
        let first = (self.aux_len - start).min(avail as usize);
        unsafe {
            std::ptr::copy_nonoverlapping((self.aux as *const u8).add(start), data.as_mut_ptr(), first);
            if avail as usize > first {
                std::ptr::copy_nonoverlapping(
                    self.aux as *const u8,
                    data.as_mut_ptr().add(first),
                    avail as usize - first,
                );
            }
        }
        fence(Ordering::Release);
        unsafe {
            std::ptr::write_volatile(std::ptr::addr_of_mut!((*self.meta).aux_tail), head);
        }
        data
    }
}

impl AccessTracer for IntelPtTracer {
    fn start(&mut self, addr: usize, _size: WatchSize, _kind: WatchKind) -> Result<()> {
        // PT records control flow globally; the watched address is advisory here.
        self.watched = Some(addr);
        let r = unsafe { libc::ioctl(self.fd, PERF_EVENT_IOC_ENABLE, 0) };
        if r < 0 {
            return Err(SdkError::Backend {
                name: "intel-pt",
                reason: format!("PERF_EVENT_IOC_ENABLE failed: errno {}", last_errno()),
            });
        }
        self.enabled = true;
        Ok(())
    }

    fn poll(&mut self) -> Result<Vec<AccessRecord>> {
        if !self.enabled {
            return Ok(vec![]);
        }
        let data = self.drain_aux();
        for ip in decode_ips(&data) {
            *self.hits.entry(ip as usize).or_insert(0) += 1;
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
            unsafe {
                libc::ioctl(self.fd, PERF_EVENT_IOC_DISABLE, 0);
            }
            self.enabled = false;
        }
        unsafe {
            if !self.aux.is_null() {
                libc::munmap(self.aux, self.aux_len);
                self.aux = std::ptr::null_mut();
            }
            if !self.base.is_null() {
                libc::munmap(self.base, self.base_len);
                self.base = std::ptr::null_mut();
            }
            if self.fd >= 0 {
                libc::close(self.fd);
                self.fd = -1;
            }
        }
        Ok(())
    }
}

impl Drop for IntelPtTracer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

// --- Minimal PT packet decoder (pure, unit-tested) ------------------------

/// Bytes of IP payload following an IP packet opcode, by the IPBytes field
/// (`opcode >> 5`). `None` marks a reserved encoding.
fn ip_payload_len(ipbytes: u8) -> Option<usize> {
    match ipbytes {
        0 => Some(0),
        1 => Some(2),
        2 => Some(4),
        3 => Some(6),
        4 => Some(6),
        6 => Some(8),
        _ => None,
    }
}

fn read_le(buf: &[u8], n: usize) -> u64 {
    let mut v = 0u64;
    for (i, &b) in buf.iter().take(n).enumerate() {
        v |= (b as u64) << (8 * i);
    }
    v
}

fn sext(v: u64, bits: u32) -> u64 {
    let shift = 64 - bits;
    (((v << shift) as i64) >> shift) as u64
}

/// Rebuilds a full IP from the running `last` IP and a compressed payload.
fn reconstruct_ip(last: u64, ipbytes: u8, payload: &[u8]) -> u64 {
    match ipbytes {
        0 => last,
        1 => (last & !0xFFFF) | read_le(payload, 2),
        2 => (last & !0xFFFF_FFFF) | read_le(payload, 4),
        3 => sext(read_le(payload, 6), 48),
        4 => (last & !0xFFFF_FFFF_FFFF) | read_le(payload, 6),
        6 => read_le(payload, 8),
        _ => last,
    }
}

/// Walks a PT byte stream and returns the flow-changing IPs (from TIP / FUP /
/// TIP.PGE / TIP.PGD). Best-effort: unrecognised bytes advance by one and the
/// stream resynchronises at the next PSB.
fn decode_ips(buf: &[u8]) -> Vec<u64> {
    let mut out = Vec::new();
    let mut last: u64 = 0;
    let mut i = 0usize;
    while i < buf.len() {
        let b = buf[i];

        // Extended (0x02-prefixed) packets — skip by known length.
        if b == 0x02 {
            if i + 1 >= buf.len() {
                break;
            }
            let len = match buf[i + 1] {
                0x82 => 16, // PSB (02 82 x8)
                0x23 => 2,  // PSBEND
                0x03 => 4,  // CBR
                0xF3 => 2,  // OVF
                0xA3 => 8,  // long TNT
                0x43 => 8,  // PIP
                0x83 => 2,  // TraceStop
                _ => 2,     // unknown ext: resync-friendly
            };
            i += len;
            continue;
        }

        // Single-byte non-IP opcodes.
        match b {
            0x00 => {
                i += 1;
                continue;
            } // PAD
            0x19 => {
                i += 8;
                continue;
            } // TSC
            0x59 => {
                i += 2;
                continue;
            } // MTC
            0x99 => {
                i += 2;
                continue;
            } // MODE
            _ => {}
        }

        // IP packets carry the opcode in the low 5 bits, IPBytes in the high 3.
        let opc = b & 0x1F;
        if opc == 0x0D || opc == 0x11 || opc == 0x01 || opc == 0x1D {
            let ipbytes = b >> 5;
            if let Some(plen) = ip_payload_len(ipbytes) {
                if i + 1 + plen > buf.len() {
                    break;
                }
                if ipbytes != 0 {
                    last = reconstruct_ip(last, ipbytes, &buf[i + 1..i + 1 + plen]);
                    out.push(last);
                }
                i += 1 + plen;
                continue;
            }
            i += 1;
            continue;
        }

        // Short TNT / CYC / unknown: consume one byte and keep scanning.
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ioctl_numbers() {
        assert_eq!(PERF_EVENT_IOC_ENABLE, 0x2400);
        assert_eq!(PERF_EVENT_IOC_DISABLE, 0x2401);
    }

    #[test]
    fn ipbytes_len_mapping() {
        assert_eq!(ip_payload_len(0), Some(0));
        assert_eq!(ip_payload_len(1), Some(2));
        assert_eq!(ip_payload_len(2), Some(4));
        assert_eq!(ip_payload_len(3), Some(6));
        assert_eq!(ip_payload_len(4), Some(6));
        assert_eq!(ip_payload_len(6), Some(8));
        assert_eq!(ip_payload_len(5), None);
    }

    #[test]
    fn decode_recovers_full_tip() {
        let ip: u64 = 0x0000_7f12_3456_789a;
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0x02, 0x82].repeat(8)); // PSB (16 bytes)
        buf.push(0x0D | (6 << 5)); // TIP, IPBytes=6 (full 8-byte IP)
        buf.extend_from_slice(&ip.to_le_bytes());
        buf.push(0x00); // PAD
        let ips = decode_ips(&buf);
        assert!(ips.contains(&ip), "recovered {ips:x?}");
    }

    #[test]
    fn attach_gates_without_pt() {
        // No panic; without PT / perms this is Unsupported (or Backend). On a PT
        // host with permission it may succeed.
        match IntelPtTracer::attach(std::process::id()) {
            Ok(mut t) => {
                let _ = t.stop();
            }
            Err(SdkError::Unsupported(_)) | Err(SdkError::Backend { .. }) => {}
            Err(e) => panic!("unexpected error: {e}"),
        }
    }
}
