//! Debugger tool window.
//!
//! Ptrace/Frida-backed features that share a single attach (only one tracer per
//! process, so starting one stops the other):
//! - **Find what accesses** an address (backend-selectable: hardware watchpoint,
//!   LibIHT, or Intel PT) via [`find_what_accesses`].
//! - **Breakpoints**: attach the debugger (ptrace or Frida), and reconcile the
//!   shared desired-breakpoint set ([`GlobalState::breakpoints`]) against the
//!   live session; register state is shown at each stop and the current RIP is
//!   published to [`GlobalState::debug_rip`] for the disassembly window.
//!
//! Polling + reconciliation run every frame (even while closed) so an active
//! session keeps moving. Memory reads/writes elsewhere use `process_vm_readv`
//! and are unaffected by the ptrace attach.

use crate::{gui::floating_window, state::StateRef};
use eframe::egui::{Button, ComboBox, Context, ScrollArea, TextEdit, Ui};
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
        /// Breakpoints actually applied to the live session (id + address).
        applied: Vec<(BpId, usize)>,
        stopped: Option<Stop>,
    },
}

const ACCESS_BACKENDS: &[AccessBackend] = &[
    AccessBackend::Hardware,
    AccessBackend::LibIht,
    AccessBackend::IntelPt,
];
const DEBUG_BACKENDS: &[BackendKind] = &[BackendKind::Ptrace, BackendKind::Frida];

fn access_label(b: AccessBackend) -> &'static str {
    match b {
        AccessBackend::Hardware => "Hardware watchpoint",
        AccessBackend::LibIht => "LibIHT (LBR)",
        AccessBackend::IntelPt => "Intel PT",
    }
}

fn debug_label(b: BackendKind) -> &'static str {
    match b {
        BackendKind::Ptrace => "ptrace (native)",
        BackendKind::Frida => "Frida",
    }
}

pub struct DebuggerWindow {
    state: StateRef,
    shown: bool,
    session: Session,
    watch_input: String,
    bp_input: String,
    access_backend: AccessBackend,
    debug_backend: BackendKind,
}

impl DebuggerWindow {
    pub fn new(state: StateRef) -> Self {
        Self {
            state,
            shown: false,
            session: Session::None,
            watch_input: String::new(),
            bp_input: String::new(),
            access_backend: AccessBackend::Hardware,
            debug_backend: BackendKind::Ptrace,
        }
    }

    pub fn toggle(&mut self) {
        self.shown = !self.shown;
    }

    pub fn show(&mut self, ctx: &Context) {
        if let Some(addr) = self.state.borrow_mut().debug_request.take() {
            self.start_access(addr, WatchKind::Write);
        }
        self.poll_session();
        self.reconcile_breakpoints();

        let shown = self.shown;
        let response = floating_window(
            ctx,
            shown,
            "nem_debugger_viewport",
            "Debugger",
            [580.0, 520.0],
            |ui| self.ui(ui),
        );
        if let Some((true, ())) = response {
            self.shown = false;
        }
    }

    fn pid(&self) -> Option<u32> {
        self.state.borrow().process.read().as_ref().map(|p| p.id())
    }

    fn set_rip(&self, rip: Option<usize>) {
        self.state.borrow_mut().debug_rip = rip;
    }

    fn poll_session(&mut self) {
        enum Act {
            None,
            Ended(String),
            Stopped(usize),
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
                    Act::None
                } else {
                    match dbg.wait(Some(Duration::from_millis(5))) {
                        Ok(ev) => {
                            if let StopReason::Exited(code) = ev.reason {
                                Act::Ended(format!("target exited ({code})"))
                            } else {
                                let regs = dbg.registers(ev.tid).ok();
                                let rip = regs.as_ref().map(|r| r.rip as usize).unwrap_or(0);
                                *stopped = Some(Stop {
                                    reason: format!("{:?}", ev.reason),
                                    regs,
                                    tid: ev.tid,
                                });
                                Act::Stopped(rip)
                            }
                        }
                        Err(_) => Act::None,
                    }
                }
            }
            Session::None => Act::None,
        };
        match act {
            Act::Ended(msg) => {
                self.session = Session::None;
                self.set_rip(None);
                self.warn(msg);
            }
            Act::Stopped(rip) => self.set_rip(Some(rip)),
            Act::None => {}
        }
    }

    /// Applies additions/removals from the shared desired-breakpoint set to the
    /// live debug session.
    fn reconcile_breakpoints(&mut self) {
        let desired: Vec<usize> = self.state.borrow().breakpoints.clone();
        let mut err = None;
        if let Session::Debug { dbg, applied, .. } = &mut self.session {
            for &addr in &desired {
                if !applied.iter().any(|(_, a)| *a == addr) {
                    match dbg.set_sw_breakpoint(addr) {
                        Ok(id) => applied.push((id, addr)),
                        Err(e) => err = Some(e.to_string()),
                    }
                }
            }
            let mut i = 0;
            while i < applied.len() {
                let (id, addr) = applied[i];
                if !desired.contains(&addr) {
                    let _ = dbg.clear_breakpoint(id);
                    applied.remove(i);
                } else {
                    i += 1;
                }
            }
        }
        if let Some(e) = err {
            self.warn(e);
        }
    }

    fn start_access(&mut self, addr: usize, kind: WatchKind) {
        self.stop_session();
        let backend = self.access_backend;
        let Some(pid) = self.pid() else {
            self.warn("Not attached to a process");
            return;
        };
        match find_what_accesses(pid, backend) {
            Ok(mut tracer) => match tracer.start(addr, WatchSize::B4, kind) {
                Ok(()) => {
                    self.session = Session::Access { tracer, addr, kind, results: Vec::new() };
                    self.shown = true;
                }
                Err(e) => self.warn(format!("watch failed: {e}")),
            },
            Err(e) => self.warn(format!("{} unavailable: {e}", access_label(backend))),
        }
    }

    fn start_debug(&mut self) {
        self.stop_session();
        let backend = self.debug_backend;
        let Some(pid) = self.pid() else {
            self.warn("Not attached to a process");
            return;
        };
        match debug::attach(pid, backend) {
            Ok(mut dbg) => {
                let _ = dbg.cont();
                self.session = Session::Debug {
                    dbg,
                    applied: Vec::new(),
                    stopped: None,
                };
                self.shown = true;
            }
            Err(e) => self.warn(format!("{} attach failed: {e}", debug_label(backend))),
        }
    }

    fn do_continue(&mut self) {
        if let Session::Debug { dbg, stopped, .. } = &mut self.session {
            *stopped = None;
            let _ = dbg.cont();
        }
        self.set_rip(None);
    }

    fn stop_session(&mut self) {
        match std::mem::replace(&mut self.session, Session::None) {
            Session::Access { mut tracer, .. } => {
                let _ = tracer.stop();
            }
            Session::Debug { mut dbg, applied, .. } => {
                for (id, _) in applied {
                    let _ = dbg.clear_breakpoint(id);
                }
                let _ = dbg.detach();
            }
            Session::None => {}
        }
        self.set_rip(None);
    }

    fn ui(&mut self, ui: &mut Ui) {
        let bps = self.state.borrow().breakpoints.clone();

        let mut do_find_write = false;
        let mut do_find_rw = false;
        let mut do_stop_access = false;
        let mut do_attach = false;
        let mut do_detach = false;
        let mut do_continue = false;
        let mut set_bp = false;
        let mut clear_bp = None;

        ui.group(|ui| {
            ui.label("Find what accesses an address");
            ui.horizontal(|ui| {
                ComboBox::new("_acc_backend", "backend")
                    .selected_text(access_label(self.access_backend))
                    .show_ui(ui, |ui| {
                        for b in ACCESS_BACKENDS {
                            if ui
                                .selectable_label(self.access_backend == *b, access_label(*b))
                                .clicked()
                            {
                                self.access_backend = *b;
                            }
                        }
                    });
            });
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
                    .max_height(140.0)
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
            ui.horizontal(|ui| {
                ui.add(
                    TextEdit::singleline(&mut self.bp_input)
                        .hint_text("0x… address")
                        .desired_width(160.0),
                );
                set_bp = ui.button("Set breakpoint").clicked();
                match &self.session {
                    Session::Debug { stopped, .. } => {
                        do_continue = ui
                            .add_enabled(stopped.is_some(), Button::new("Continue"))
                            .clicked();
                        do_detach = ui.button("Detach").clicked();
                    }
                    _ => {
                        ComboBox::new("_dbg_backend", "")
                            .selected_text(debug_label(self.debug_backend))
                            .show_ui(ui, |ui| {
                                for b in DEBUG_BACKENDS {
                                    if ui
                                        .selectable_label(
                                            self.debug_backend == *b,
                                            debug_label(*b),
                                        )
                                        .clicked()
                                    {
                                        self.debug_backend = *b;
                                    }
                                }
                            });
                        do_attach = ui.button("Attach debugger").clicked();
                    }
                }
            });

            for &addr in &bps {
                ui.horizontal(|ui| {
                    let live = matches!(&self.session,
                        Session::Debug { applied, .. } if applied.iter().any(|(_, a)| *a == addr));
                    ui.monospace(format!("{} {addr:#x}", if live { "●" } else { "○" }));
                    if ui.small_button("clear").clicked() {
                        clear_bp = Some(addr);
                    }
                });
            }

            if let Session::Debug { stopped: Some(stop), .. } = &self.session {
                ui.separator();
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
            } else if matches!(self.session, Session::Debug { .. }) {
                ui.label("running…");
            }
        });

        // Apply collected intents.
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
                Some(a) => {
                    let mut st = self.state.borrow_mut();
                    if !st.breakpoints.contains(&a) {
                        st.breakpoints.push(a);
                    }
                }
                None => self.warn("Enter a hex address"),
            }
        }
        if let Some(addr) = clear_bp {
            self.state.borrow_mut().breakpoints.retain(|&a| a != addr);
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
