//! Debugger tool window.
//!
//! Two ptrace-backed features that share a single attach (only one ptrace tracer
//! per process is allowed, so starting one stops the other):
//! - **Find what accesses** an address via a hardware watchpoint
//!   ([`find_what_accesses`]) — lists the writing/accessing instructions.
//! - **Breakpoints**: attach the native debugger, set software breakpoints, and
//!   inspect registers at each stop.
//!
//! Polling happens every frame (even while the window is closed) so an active
//! trace keeps the target moving; memory reads/writes elsewhere in the GUI use
//! `process_vm_readv/writev` and are unaffected by the ptrace attach.

use crate::{gui::floating_window, state::StateRef};
use eframe::egui::{Button, Context, ScrollArea, TextEdit, Ui};
use nemclass_sdk::{
    debug::{self, BackendKind},
    find_what_accesses, AccessBackend, AccessRecord, AccessTracer, BpId, Debugger, Registers,
    StopReason, ThreadId, WatchKind, WatchSize,
};
use std::time::Duration;

struct Stop {
    reason: String,
    regs: Option<Registers>,
    #[allow(dead_code)]
    tid: ThreadId,
}

enum Session {
    None,
    Access {
        tracer: Box<dyn AccessTracer>,
        addr: usize,
        kind: WatchKind,
        results: Vec<AccessRecord>,
    },
    Debug {
        dbg: Box<dyn Debugger>,
        breakpoints: Vec<(BpId, usize)>,
        stopped: Option<Stop>,
    },
}

pub struct DebuggerWindow {
    state: StateRef,
    shown: bool,
    session: Session,
    watch_input: String,
    bp_input: String,
}

impl DebuggerWindow {
    pub fn new(state: StateRef) -> Self {
        Self {
            state,
            shown: false,
            session: Session::None,
            watch_input: String::new(),
            bp_input: String::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.shown = !self.shown;
    }

    pub fn show(&mut self, ctx: &Context) {
        // A cheat-table row can ask us to trace an address.
        if let Some(addr) = self.state.borrow_mut().debug_request.take() {
            self.start_access(addr, WatchKind::Write);
        }
        // Advance any active session every frame, even while hidden.
        self.poll_session();

        let shown = self.shown;
        let response = floating_window(
            ctx,
            shown,
            "nem_debugger_viewport",
            "Debugger",
            [560.0, 480.0],
            |ui| self.ui(ui),
        );
        if let Some((true, ())) = response {
            self.shown = false;
        }
    }

    fn pid(&self) -> Option<u32> {
        self.state.borrow().process.read().as_ref().map(|p| p.id())
    }

    fn poll_session(&mut self) {
        enum Act {
            None,
            Ended(String),
        }
        let act = match &mut self.session {
            Session::Access { tracer, results, .. } => {
                if let Ok(recs) = tracer.poll() {
                    *results = recs;
                }
                Act::None
            }
            Session::Debug { dbg, stopped, .. } => {
                if stopped.is_some() {
                    Act::None // wait for the user to Continue
                } else {
                    match dbg.wait(Some(Duration::from_millis(5))) {
                        Ok(ev) => {
                            if let StopReason::Exited(code) = ev.reason {
                                Act::Ended(format!("target exited ({code})"))
                            } else {
                                let regs = dbg.registers(ev.tid).ok();
                                *stopped = Some(Stop {
                                    reason: format!("{:?}", ev.reason),
                                    regs,
                                    tid: ev.tid,
                                });
                                Act::None
                            }
                        }
                        Err(_) => Act::None, // timeout / transient: keep running
                    }
                }
            }
            Session::None => Act::None,
        };
        if let Act::Ended(msg) = act {
            self.session = Session::None;
            self.warn(msg);
        }
    }

    fn start_access(&mut self, addr: usize, kind: WatchKind) {
        self.stop_session();
        let Some(pid) = self.pid() else {
            self.warn("Not attached to a process");
            return;
        };
        match find_what_accesses(pid, AccessBackend::Hardware) {
            Ok(mut tracer) => match tracer.start(addr, WatchSize::B4, kind) {
                Ok(()) => {
                    self.session = Session::Access { tracer, addr, kind, results: Vec::new() };
                    self.shown = true;
                }
                Err(e) => self.warn(format!("watch failed: {e}")),
            },
            Err(e) => self.warn(format!("tracer unavailable: {e}")),
        }
    }

    fn start_debug(&mut self) {
        self.stop_session();
        let Some(pid) = self.pid() else {
            self.warn("Not attached to a process");
            return;
        };
        match debug::attach(pid, BackendKind::Ptrace) {
            Ok(mut dbg) => {
                let _ = dbg.cont(); // let the target run until a breakpoint
                self.session = Session::Debug {
                    dbg,
                    breakpoints: Vec::new(),
                    stopped: None,
                };
                self.shown = true;
            }
            Err(e) => self.warn(format!("attach failed: {e}")),
        }
    }

    fn set_breakpoint(&mut self, addr: usize) {
        let err = if let Session::Debug { dbg, breakpoints, .. } = &mut self.session {
            match dbg.set_sw_breakpoint(addr) {
                Ok(id) => {
                    breakpoints.push((id, addr));
                    None
                }
                Err(e) => Some(e.to_string()),
            }
        } else {
            Some("attach the debugger first".to_owned())
        };
        if let Some(e) = err {
            self.warn(e);
        }
    }

    fn clear_breakpoint(&mut self, index: usize) {
        if let Session::Debug { dbg, breakpoints, .. } = &mut self.session {
            if index < breakpoints.len() {
                let (id, _) = breakpoints.remove(index);
                let _ = dbg.clear_breakpoint(id);
            }
        }
    }

    fn do_continue(&mut self) {
        if let Session::Debug { dbg, stopped, .. } = &mut self.session {
            *stopped = None;
            let _ = dbg.cont();
        }
    }

    fn stop_session(&mut self) {
        match std::mem::replace(&mut self.session, Session::None) {
            Session::Access { mut tracer, .. } => {
                let _ = tracer.stop();
            }
            Session::Debug {
                mut dbg,
                breakpoints,
                ..
            } => {
                for (id, _) in breakpoints {
                    let _ = dbg.clear_breakpoint(id);
                }
                let _ = dbg.detach();
            }
            Session::None => {}
        }
    }

    fn ui(&mut self, ui: &mut Ui) {
        let mut do_find_write = false;
        let mut do_find_rw = false;
        let mut do_stop_access = false;
        let mut do_attach = false;
        let mut do_detach = false;
        let mut do_continue = false;
        let mut set_bp = false;
        let mut clear_bp = None;

        ui.group(|ui| {
            ui.label("Find what accesses an address (hardware watchpoint)");
            ui.horizontal(|ui| {
                ui.add(
                    TextEdit::singleline(&mut self.watch_input)
                        .hint_text("0x… address")
                        .desired_width(160.0),
                );
                do_find_write = ui.button("Find writes").clicked();
                do_find_rw = ui.button("Find reads+writes").clicked();
            });
            if let Session::Access { addr, kind, results, .. } = &self.session {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "watching {addr:#x} ({kind:?}) — {} site(s)",
                        results.len()
                    ));
                    do_stop_access = ui.button("Stop").clicked();
                });
                ScrollArea::vertical()
                    .max_height(150.0)
                    .id_salt("_acc_results")
                    .show(ui, |ui| {
                        for r in results {
                            ui.monospace(format!(
                                "{:#016x}   ×{}   rax={:#x}",
                                r.insn_addr, r.hits, r.regs.rax
                            ));
                        }
                    });
            }
        });

        ui.add_space(4.0);

        ui.group(|ui| {
            ui.label("Breakpoints");
            match &self.session {
                Session::Debug { breakpoints, stopped, .. } => {
                    ui.horizontal(|ui| {
                        ui.add(
                            TextEdit::singleline(&mut self.bp_input)
                                .hint_text("0x… address")
                                .desired_width(160.0),
                        );
                        set_bp = ui.button("Set breakpoint").clicked();
                        do_continue = ui
                            .add_enabled(stopped.is_some(), Button::new("Continue"))
                            .clicked();
                        do_detach = ui.button("Detach").clicked();
                    });
                    for (i, (_, addr)) in breakpoints.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.monospace(format!("{addr:#x}"));
                            if ui.small_button("clear").clicked() {
                                clear_bp = Some(i);
                            }
                        });
                    }
                    ui.separator();
                    if let Some(stop) = stopped {
                        ui.label(format!("Stopped: {}", stop.reason));
                        if let Some(r) = &stop.regs {
                            ui.monospace(format!(
                                "rip={:#018x}  rsp={:#018x}  rbp={:#018x}",
                                r.rip, r.rsp, r.rbp
                            ));
                            ui.monospace(format!(
                                "rax={:#018x}  rbx={:#018x}  rcx={:#018x}",
                                r.rax, r.rbx, r.rcx
                            ));
                            ui.monospace(format!(
                                "rdx={:#018x}  rsi={:#018x}  rdi={:#018x}",
                                r.rdx, r.rsi, r.rdi
                            ));
                        }
                    } else {
                        ui.label("running…");
                    }
                }
                _ => {
                    do_attach = ui.button("Attach debugger (ptrace)").clicked();
                }
            }
        });

        if do_find_write {
            match parse_addr(&self.watch_input) {
                Some(a) => self.start_access(a, WatchKind::Write),
                None => self.warn("Enter a hex address"),
            }
        }
        if do_find_rw {
            match parse_addr(&self.watch_input) {
                Some(a) => self.start_access(a, WatchKind::ReadWrite),
                None => self.warn("Enter a hex address"),
            }
        }
        if do_stop_access {
            self.stop_session();
        }
        if do_attach {
            self.start_debug();
        }
        if do_detach {
            self.stop_session();
        }
        if set_bp {
            match parse_addr(&self.bp_input) {
                Some(a) => self.set_breakpoint(a),
                None => self.warn("Enter a hex address"),
            }
        }
        if let Some(i) = clear_bp {
            self.clear_breakpoint(i);
        }
        if do_continue {
            self.do_continue();
        }
    }

    fn warn(&self, msg: impl Into<String>) {
        self.state.borrow_mut().toasts.warning(msg.into());
    }
}

impl Drop for DebuggerWindow {
    fn drop(&mut self) {
        self.stop_session();
    }
}

fn parse_addr(s: &str) -> Option<usize> {
    let t = s.trim();
    usize::from_str_radix(t.strip_prefix("0x").unwrap_or(t), 16).ok()
}
