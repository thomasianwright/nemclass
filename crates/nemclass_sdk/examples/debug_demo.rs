//! Attach the native ptrace debugger, set a breakpoint, and print registers.
//!
//! Usage: `cargo run -p nemclass_sdk --example debug_demo -- <pid> <hex-addr>`
//!
//! Requires the `debug-ptrace` feature (on by default) and permission to ptrace
//! the target (same uid, or `CAP_SYS_PTRACE`, or `ptrace_scope=0`).

use nemclass_sdk::debug::{attach, BackendKind};
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let pid: u32 = args.next().and_then(|a| a.parse().ok()).expect("usage: debug_demo <pid> <hex-addr>");
    let addr = args
        .next()
        .and_then(|a| usize::from_str_radix(a.trim_start_matches("0x"), 16).ok())
        .expect("usage: debug_demo <pid> <hex-addr>");

    let mut dbg = attach(pid, BackendKind::Ptrace).expect("attach debugger");
    let bp = dbg.set_sw_breakpoint(addr).expect("set breakpoint");
    println!("breakpoint {bp:?} set at {addr:#x}; continuing...");

    dbg.cont().expect("continue");
    let event = dbg.wait(Some(Duration::from_secs(10))).expect("wait");
    println!("stopped: {:?}", event);

    let regs = dbg.registers(event.tid).expect("registers");
    println!("rip={:#x} rsp={:#x} rax={:#x}", regs.rip, regs.rsp, regs.rax);

    dbg.clear_breakpoint(bp).ok();
    dbg.detach().expect("detach");
}
