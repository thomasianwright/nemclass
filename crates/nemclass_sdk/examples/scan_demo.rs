//! Progressive value scan against a running process.
//!
//! Usage: `cargo run -p nemclass_sdk --example scan_demo -- <pid> <i32-value>`
//!
//! Prints how many addresses currently hold the value, then (as a demo of
//! refinement) how many are unchanged a moment later.

use nemclass_sdk::scan::{ScanCompare, ScanConfig, ScanType, ScanValue, Scanner};
use nemclass_sdk::Target;
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let pid: u32 = args.next().and_then(|a| a.parse().ok()).expect("usage: scan_demo <pid> <i32>");
    let value: i128 = args.next().and_then(|a| a.parse().ok()).expect("usage: scan_demo <pid> <i32>");

    let target = Target::attach_pid(pid).expect("attach");
    let scanner = Scanner::new(&target, ScanConfig::new(ScanType::I32));

    let first = scanner
        .first_scan(ScanCompare::Exact(ScanValue::Int(value)))
        .expect("first scan");
    println!("first scan: {} addresses hold {value}", first.len());

    std::thread::sleep(Duration::from_secs(1));

    let unchanged = scanner.next_scan(&first, ScanCompare::Unchanged).expect("next scan");
    println!("still {} unchanged after 1s", unchanged.len());
    for &addr in unchanged.addresses().iter().take(10) {
        println!("  {addr:#x}");
    }
}
