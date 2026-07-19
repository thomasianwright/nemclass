//! x86-64 disassembly and lightweight code/memory analysis (feature `disasm`).
//!
//! Built on [`iced-x86`](https://docs.rs/iced-x86). Provides:
//! - [`disassemble`] — decode instructions from a [`Target`] at an address,
//!   with flow classification and resolved near-branch/call targets.
//! - [`memory_map`] — the target's full `/proc/<pid>/maps`, including
//!   **anonymous** mappings, with stack/heap/vdso/module classification.
//! - [`find_strings`] — printable-ASCII string extraction over a range.
//! - [`call_targets`] / [`find_functions`] — best-effort referenced-call and
//!   function discovery (call targets + `push rbp; mov rbp, rsp` prologues); this
//!   is a heuristic, not a full analysis engine.

use crate::error::{Result, SdkError};
use crate::target::Target;
use iced_x86::{Decoder, DecoderOptions, FlowControl, Formatter, Instruction, NasmFormatter};

/// The control-flow role of an instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowKind {
    /// Falls through to the next instruction.
    Seq,
    /// A call (near call resolves [`Insn::target`]).
    Call,
    /// An unconditional/indirect jump.
    Jump,
    /// A conditional jump.
    CondJump,
    /// A return.
    Ret,
    /// A software interrupt / breakpoint.
    Int,
    /// An invalid / undecodable byte.
    Bad,
}

/// One decoded instruction.
#[derive(Debug, Clone)]
pub struct Insn {
    /// Instruction address.
    pub addr: usize,
    /// Encoded length in bytes.
    pub len: usize,
    /// Raw encoded bytes.
    pub bytes: Vec<u8>,
    /// Formatted (NASM) text.
    pub text: String,
    /// Control-flow role.
    pub kind: FlowKind,
    /// Resolved absolute target for near branches/calls, if any.
    pub target: Option<usize>,
}

/// Decodes up to `count` instructions starting at `start` in `target`.
pub fn disassemble(target: &Target, start: usize, count: usize) -> Vec<Insn> {
    let cap = count.saturating_mul(16).max(16);
    let bytes = target.read_bytes(start, cap).unwrap_or_default();
    disassemble_bytes(&bytes, start, count)
}

/// Decodes every instruction in a `byte_len`-byte range starting at `start`
/// (capped at [`MAX_SCAN`]). Use this to analyze a whole region; use
/// [`disassemble`] to fetch a fixed number of instructions for a view.
pub fn disassemble_range(target: &Target, start: usize, byte_len: usize) -> Vec<Insn> {
    let bytes = target.read_bytes(start, byte_len.min(MAX_SCAN)).unwrap_or_default();
    disassemble_bytes(&bytes, start, usize::MAX)
}

/// Decodes up to `count` instructions from `bytes` (whose first byte is at `start`).
pub fn disassemble_bytes(bytes: &[u8], start: usize, count: usize) -> Vec<Insn> {
    let mut out = Vec::new();
    if bytes.is_empty() {
        return out;
    }
    let mut decoder = Decoder::with_ip(64, bytes, start as u64, DecoderOptions::NONE);
    let mut formatter = NasmFormatter::new();
    let mut text = String::new();
    while decoder.can_decode() && out.len() < count {
        let instr = decoder.decode();
        let addr = instr.ip() as usize;
        let len = instr.len().max(1);
        let off = addr.wrapping_sub(start);
        let ibytes = bytes.get(off..off + len).map(<[u8]>::to_vec).unwrap_or_default();

        if instr.is_invalid() {
            out.push(Insn {
                addr,
                len,
                bytes: ibytes,
                text: "(bad)".to_owned(),
                kind: FlowKind::Bad,
                target: None,
            });
            continue;
        }

        text.clear();
        formatter.format(&instr, &mut text);
        let (kind, target) = classify(&instr);
        out.push(Insn {
            addr,
            len,
            bytes: ibytes,
            text: text.clone(),
            kind,
            target,
        });
    }
    out
}

fn classify(instr: &Instruction) -> (FlowKind, Option<usize>) {
    match instr.flow_control() {
        FlowControl::Call | FlowControl::IndirectCall => (FlowKind::Call, near_target(instr)),
        FlowControl::UnconditionalBranch | FlowControl::IndirectBranch => {
            (FlowKind::Jump, near_target(instr))
        }
        FlowControl::ConditionalBranch => (FlowKind::CondJump, near_target(instr)),
        FlowControl::Return => (FlowKind::Ret, None),
        FlowControl::Interrupt => (FlowKind::Int, None),
        _ => (FlowKind::Seq, None),
    }
}

/// Resolved target of a near branch/call (0 for indirect operands → `None`).
fn near_target(instr: &Instruction) -> Option<usize> {
    let t = instr.near_branch_target();
    (t != 0).then_some(t as usize)
}

// --- memory map -----------------------------------------------------------

/// The role of a memory region in the target's address space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    /// A file-backed mapping (executable, library, etc.).
    Module,
    /// The main heap (`[heap]`).
    Heap,
    /// A thread stack (`[stack]`).
    Stack,
    /// Kernel-provided mapping (`[vdso]`/`[vvar]`/`[vsyscall]`).
    Vdso,
    /// An anonymous mapping (no backing file).
    Anon,
    /// Anything else (e.g. named anon `[anon:...]`).
    Other,
}

/// A parsed `/proc/<pid>/maps` region.
#[derive(Debug, Clone)]
pub struct MapRegion {
    /// Start address (inclusive).
    pub from: usize,
    /// End address (exclusive).
    pub to: usize,
    /// Readable.
    pub read: bool,
    /// Writable.
    pub write: bool,
    /// Executable.
    pub exec: bool,
    /// Path / pseudo-name (empty for plain anonymous mappings).
    pub name: String,
    /// Classified role.
    pub kind: RegionKind,
}

impl MapRegion {
    /// Size in bytes.
    pub fn size(&self) -> usize {
        self.to.saturating_sub(self.from)
    }

    /// A short display label (name, or a synthesized one for anon regions).
    pub fn label(&self) -> String {
        if self.name.is_empty() {
            format!("anon {:#x}", self.from)
        } else {
            self.name.clone()
        }
    }
}

/// Reads and parses the target's `/proc/<pid>/maps`, including anonymous regions.
pub fn memory_map(pid: u32) -> Result<Vec<MapRegion>> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/maps"))
        .map_err(|e| SdkError::Io(e.to_string()))?;
    Ok(text.lines().filter_map(parse_maps_line).collect())
}

/// Parses one `/proc/<pid>/maps` line, e.g.
/// `55d4…-55d4… r-xp 00001000 08:02 1234  /usr/bin/foo`.
fn parse_maps_line(line: &str) -> Option<MapRegion> {
    let mut it = line.split_whitespace();
    let range = it.next()?;
    let perms = it.next()?;
    let _offset = it.next();
    let _dev = it.next();
    let _inode = it.next();
    let name = it.collect::<Vec<_>>().join(" ");

    let (f, t) = range.split_once('-')?;
    let from = usize::from_str_radix(f, 16).ok()?;
    let to = usize::from_str_radix(t, 16).ok()?;
    let p = perms.as_bytes();
    let read = p.first() == Some(&b'r');
    let write = p.get(1) == Some(&b'w');
    let exec = p.get(2) == Some(&b'x');
    let kind = classify_region(&name);

    Some(MapRegion { from, to, read, write, exec, name, kind })
}

fn classify_region(name: &str) -> RegionKind {
    if name.is_empty() {
        RegionKind::Anon
    } else if name == "[heap]" {
        RegionKind::Heap
    } else if name.starts_with("[stack") {
        RegionKind::Stack
    } else if matches!(name, "[vdso]" | "[vvar]" | "[vsyscall]") {
        RegionKind::Vdso
    } else if name.starts_with('/') {
        RegionKind::Module
    } else {
        RegionKind::Other
    }
}

// --- strings & functions --------------------------------------------------

/// A printable-ASCII string found in memory.
#[derive(Debug, Clone)]
pub struct StringHit {
    /// Address of the first byte.
    pub addr: usize,
    /// The decoded text.
    pub text: String,
}

/// The largest span [`find_strings`]/[`find_functions`] will read in one call.
const MAX_SCAN: usize = 8 << 20; // 8 MiB

/// Extracts printable-ASCII runs of at least `min_len` bytes from `[start, start+len)`.
pub fn find_strings(target: &Target, start: usize, len: usize, min_len: usize) -> Vec<StringHit> {
    let len = len.min(MAX_SCAN);
    let bytes = target.read_bytes(start, len).unwrap_or_default();
    let mut out = Vec::new();
    let mut run_start = 0;
    let mut run: Vec<u8> = Vec::new();
    let flush = |run: &mut Vec<u8>, run_start: usize, out: &mut Vec<StringHit>| {
        if run.len() >= min_len {
            out.push(StringHit {
                addr: start + run_start,
                text: String::from_utf8_lossy(run).into_owned(),
            });
        }
        run.clear();
    };
    for (i, &b) in bytes.iter().enumerate() {
        if (0x20..=0x7e).contains(&b) {
            if run.is_empty() {
                run_start = i;
            }
            run.push(b);
        } else {
            flush(&mut run, run_start, &mut out);
        }
    }
    flush(&mut run, run_start, &mut out);
    out
}

/// Direct near-call targets referenced by `insns`, sorted and deduplicated.
pub fn call_targets(insns: &[Insn]) -> Vec<usize> {
    let mut v: Vec<usize> = insns
        .iter()
        .filter(|i| i.kind == FlowKind::Call)
        .filter_map(|i| i.target)
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Best-effort function-entry discovery over an executable range: call targets
/// that land inside the range plus `push rbp; mov rbp, rsp` prologues.
pub fn find_functions(target: &Target, start: usize, len: usize) -> Vec<usize> {
    let len = len.min(MAX_SCAN);
    let bytes = target.read_bytes(start, len).unwrap_or_default();
    let end = start + bytes.len();

    let insns = disassemble_bytes(&bytes, start, usize::MAX);
    let mut funcs: Vec<usize> = call_targets(&insns)
        .into_iter()
        .filter(|&t| t >= start && t < end)
        .collect();

    // Prologue: 55 48 89 E5  (push rbp; mov rbp, rsp).
    for (i, w) in bytes.windows(4).enumerate() {
        if w == [0x55, 0x48, 0x89, 0xE5] {
            funcs.push(start + i);
        }
    }

    funcs.sort_unstable();
    funcs.dedup();
    funcs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_mov_call_ret() {
        // mov rbp, rsp ; call +0 ; ret
        let start = 0x1000;
        let bytes = [0x48, 0x89, 0xE5, 0xE8, 0x00, 0x00, 0x00, 0x00, 0xC3];
        let insns = disassemble_bytes(&bytes, start, 8);
        assert_eq!(insns.len(), 3);
        assert!(insns[0].text.contains("rbp"), "{}", insns[0].text);
        assert_eq!(insns[0].kind, FlowKind::Seq);
        assert_eq!(insns[1].kind, FlowKind::Call);
        // call rel32=0 at ip 0x1003, next ip 0x1008 -> target 0x1008.
        assert_eq!(insns[1].target, Some(0x1008));
        assert_eq!(insns[2].kind, FlowKind::Ret);
        assert_eq!(call_targets(&insns), vec![0x1008]);
    }

    #[test]
    fn strings_extraction() {
        let target_bytes = b"ab\x00hello world\x01xy";
        // Emulate via disassemble_bytes' sibling: reuse the run logic directly.
        let mut out = Vec::new();
        let mut run_start = 0;
        let mut run: Vec<u8> = Vec::new();
        for (i, &b) in target_bytes.iter().enumerate() {
            if (0x20..=0x7e).contains(&b) {
                if run.is_empty() {
                    run_start = i;
                }
                run.push(b);
            } else {
                if run.len() >= 4 {
                    out.push((run_start, String::from_utf8_lossy(&run).into_owned()));
                }
                run.clear();
            }
        }
        if run.len() >= 4 {
            out.push((run_start, String::from_utf8_lossy(&run).into_owned()));
        }
        assert_eq!(out, vec![(3, "hello world".to_owned())]);
    }

    #[test]
    fn maps_line_parsing() {
        let line = "55d40a1b2000-55d40a1b3000 r-xp 00001000 08:02 393221  /usr/bin/cat";
        let r = parse_maps_line(line).unwrap();
        assert_eq!(r.from, 0x55d40a1b2000);
        assert_eq!(r.to, 0x55d40a1b3000);
        assert!(r.read && r.exec && !r.write);
        assert_eq!(r.kind, RegionKind::Module);
        assert_eq!(r.name, "/usr/bin/cat");

        let anon = parse_maps_line("7f00-7f10 rw-p 0 00:00 0 ").unwrap();
        assert_eq!(anon.kind, RegionKind::Anon);
        assert!(anon.name.is_empty());

        let stack = parse_maps_line("7ffd-7ffe rw-p 0 00:00 0 [stack]").unwrap();
        assert_eq!(stack.kind, RegionKind::Stack);
    }
}
