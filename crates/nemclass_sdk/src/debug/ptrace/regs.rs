//! Conversion between [`Registers`](crate::debug::Registers) and
//! `libc::user_regs_struct`, plus x86 debug-register (DR7) encoding.

use crate::debug::{Registers, WatchKind, WatchSize};

impl From<libc::user_regs_struct> for Registers {
    fn from(u: libc::user_regs_struct) -> Self {
        // Field order is identical to `Registers`; copy straight across.
        Registers {
            r15: u.r15,
            r14: u.r14,
            r13: u.r13,
            r12: u.r12,
            rbp: u.rbp,
            rbx: u.rbx,
            r11: u.r11,
            r10: u.r10,
            r9: u.r9,
            r8: u.r8,
            rax: u.rax,
            rcx: u.rcx,
            rdx: u.rdx,
            rsi: u.rsi,
            rdi: u.rdi,
            orig_rax: u.orig_rax,
            rip: u.rip,
            cs: u.cs,
            eflags: u.eflags,
            rsp: u.rsp,
            ss: u.ss,
            fs_base: u.fs_base,
            gs_base: u.gs_base,
            ds: u.ds,
            es: u.es,
            fs: u.fs,
            gs: u.gs,
        }
    }
}

impl From<&Registers> for libc::user_regs_struct {
    fn from(r: &Registers) -> Self {
        // Start from a zeroed struct (fields are otherwise identical) so we don't
        // depend on the exact field set of the libc definition.
        let mut u: libc::user_regs_struct = unsafe { std::mem::zeroed() };
        u.r15 = r.r15;
        u.r14 = r.r14;
        u.r13 = r.r13;
        u.r12 = r.r12;
        u.rbp = r.rbp;
        u.rbx = r.rbx;
        u.r11 = r.r11;
        u.r10 = r.r10;
        u.r9 = r.r9;
        u.r8 = r.r8;
        u.rax = r.rax;
        u.rcx = r.rcx;
        u.rdx = r.rdx;
        u.rsi = r.rsi;
        u.rdi = r.rdi;
        u.orig_rax = r.orig_rax;
        u.rip = r.rip;
        u.cs = r.cs;
        u.eflags = r.eflags;
        u.rsp = r.rsp;
        u.ss = r.ss;
        u.fs_base = r.fs_base;
        u.gs_base = r.gs_base;
        u.ds = r.ds;
        u.es = r.es;
        u.fs = r.fs;
        u.gs = r.gs;
        u
    }
}

// --- DR7 debug-register control encoding ----------------------------------
//
// DR7 layout (x86-64):
//  - bits 0..7: local/global enable per slot (Ln = bit 2*n, Gn = bit 2*n+1).
//    We use the *local* enable, which the CPU auto-clears on task switch.
//  - bits 16..31: a 4-bit condition field per slot at bit `16 + slot*4`,
//    low 2 bits = R/W, high 2 bits = LEN.

/// R/W condition bits for a watch kind (x86 has no read-only trap; reads use RW).
pub(super) fn rw_bits(kind: WatchKind) -> u64 {
    match kind {
        WatchKind::Execute => 0b00,
        WatchKind::Write => 0b01,
        WatchKind::ReadWrite => 0b11,
    }
}

/// LEN bits for a watch size. Execution breakpoints must encode LEN=00.
pub(super) fn len_bits(size: WatchSize, kind: WatchKind) -> u64 {
    if matches!(kind, WatchKind::Execute) {
        return 0b00;
    }
    match size {
        WatchSize::B1 => 0b00,
        WatchSize::B2 => 0b01,
        WatchSize::B8 => 0b10,
        WatchSize::B4 => 0b11,
    }
}

/// Returns `dr7` with `slot` enabled and its condition field set for `kind`/`size`.
pub(super) fn dr7_set(dr7: u64, slot: usize, kind: WatchKind, size: WatchSize) -> u64 {
    let mut v = dr7;
    v |= 1 << (slot * 2); // local enable
    let shift = 16 + slot * 4;
    v &= !(0xF << shift); // clear old field
    let field = rw_bits(kind) | (len_bits(size, kind) << 2);
    v |= field << shift;
    v
}

/// Returns `dr7` with `slot` disabled and its condition field cleared.
pub(super) fn dr7_clear(dr7: u64, slot: usize) -> u64 {
    let mut v = dr7;
    v &= !(1 << (slot * 2));
    v &= !(0xF << (16 + slot * 4));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dr7_encoding_write_b4_slot0() {
        // Write watch, 4 bytes, slot 0: enable bit0; field nibble = rw(01)|len(11)<<2 = 0b1101.
        let dr7 = dr7_set(0, 0, WatchKind::Write, WatchSize::B4);
        assert_eq!(dr7 & 0b1, 1, "local enable bit set");
        assert_eq!((dr7 >> 16) & 0xF, 0b1101, "rw=01 len=11");
        assert_eq!(dr7, 0x000D_0001);
        assert_eq!(dr7_clear(dr7, 0), 0, "clear restores zero");
    }

    #[test]
    fn dr7_execute_slot2_len_zero() {
        // Execute breakpoint in slot 2 forces LEN=00, rw=00.
        let dr7 = dr7_set(0, 2, WatchKind::Execute, WatchSize::B8);
        assert_eq!((dr7 >> (2 * 2)) & 0b1, 1, "slot2 local enable at bit4");
        assert_eq!((dr7 >> (16 + 2 * 4)) & 0xF, 0b0000);
    }

    #[test]
    fn registers_roundtrip() {
        let mut r = Registers::default();
        r.rip = 0x4011_22;
        r.rsp = 0x7fff_dead_beef;
        r.rax = 0x1234;
        r.r15 = 0xabcd;
        r.gs = 0x33;
        let u: libc::user_regs_struct = (&r).into();
        let back: Registers = u.into();
        assert_eq!(back, r);
        assert_eq!(back.ip(), 0x4011_22);
        assert_eq!(back.sp(), 0x7fff_dead_beef);
    }
}
