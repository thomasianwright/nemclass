//! CheatEngine-style "find out what writes to this address".
//!
//! Usage: `cargo run -p nemclass_sdk --example find_what_writes -- <pid> <hex-addr>`
//!
//! Watches the address with a hardware watchpoint and prints the instructions
//! that write to it. Requires the `debug-ptrace` feature (default) and ptrace
//! permission. Swap `AccessBackend::Hardware` for `LibIht` / `IntelPt` to use the
//! feature-gated hardware-trace backends.

use nemclass_sdk::access::{find_what_accesses, AccessBackend};
use nemclass_sdk::debug::{WatchKind, WatchSize};
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let pid: u32 = args.next().and_then(|a| a.parse().ok()).expect("usage: find_what_writes <pid> <hex-addr>");
    let addr = args
        .next()
        .and_then(|a| usize::from_str_radix(a.trim_start_matches("0x"), 16).ok())
        .expect("usage: find_what_writes <pid> <hex-addr>");

    let mut tracer = find_what_accesses(pid, AccessBackend::Hardware).expect("tracer");
    tracer.start(addr, WatchSize::B4, WatchKind::Write).expect("start");
    println!("watching {addr:#x} for writes for 10s...");

    for _ in 0..10 {
        std::thread::sleep(Duration::from_secs(1));
        for rec in tracer.poll().expect("poll") {
            println!("  {:#x} wrote (x{})  rax={:#x}", rec.insn_addr, rec.hits, rec.regs.rax);
        }
    }
    tracer.stop().ok();
}
