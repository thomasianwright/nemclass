//! Dedicated-thread debugger + access-tracer harness.
//!
//! ptrace requires every call for a tracee to come from the *same* OS thread
//! that attached, and the Frida backend runs a GLib main loop with thread
//! affinity. So a [`DebuggerHandle`] owns one thread that holds the
//! `Box<dyn Debugger>`, interleaving command handling with `wait()` polling and
//! emitting `debugger:stopped` events. The access tracer likewise runs on its
//! own thread, polling and emitting `access:hits`.

use crate::dto::{AccessRecordDto, DebugEventDto, RegistersDto};
use nemclass_sdk::access::{self, AccessBackend};
use nemclass_sdk::debug::{
    self, BackendKind, BpId, Debugger, Registers, ThreadId, WatchKind, WatchSize,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Successful reply payloads from the debugger thread.
pub enum DbgReply {
    Ok,
    BpId(u64),
    Threads(Vec<u32>),
    Regs(Box<Registers>),
}

type Reply = Sender<Result<DbgReply, String>>;

enum DbgCmd {
    SetSwBp(usize, Reply),
    SetHwBp(usize, WatchSize, WatchKind, Reply),
    ClearBp(u64, Reply),
    Cont(Reply),
    Step(u32, Reply),
    Threads(Reply),
    Registers(u32, Reply),
    SetRegisters(u32, Box<Registers>, Reply),
    Detach,
}

/// Handle to a running debugger thread.
pub struct DebuggerHandle {
    tx: Sender<DbgCmd>,
    join: Option<JoinHandle<()>>,
    pub backend: BackendKind,
}

impl DebuggerHandle {
    fn call(&self, make: impl FnOnce(Reply) -> DbgCmd) -> Result<DbgReply, String> {
        let (rtx, rrx) = channel();
        self.tx
            .send(make(rtx))
            .map_err(|_| "debugger thread is gone".to_string())?;
        rrx.recv().map_err(|_| "debugger thread is gone".to_string())?
    }

    pub fn threads(&self) -> Result<Vec<u32>, String> {
        match self.call(DbgCmd::Threads)? {
            DbgReply::Threads(t) => Ok(t),
            _ => Err("unexpected reply".into()),
        }
    }
    pub fn set_sw_bp(&self, addr: usize) -> Result<u64, String> {
        match self.call(|r| DbgCmd::SetSwBp(addr, r))? {
            DbgReply::BpId(id) => Ok(id),
            _ => Err("unexpected reply".into()),
        }
    }
    pub fn set_hw_bp(&self, addr: usize, size: WatchSize, kind: WatchKind) -> Result<u64, String> {
        match self.call(|r| DbgCmd::SetHwBp(addr, size, kind, r))? {
            DbgReply::BpId(id) => Ok(id),
            _ => Err("unexpected reply".into()),
        }
    }
    pub fn clear_bp(&self, id: u64) -> Result<(), String> {
        self.call(|r| DbgCmd::ClearBp(id, r)).map(|_| ())
    }
    pub fn cont(&self) -> Result<(), String> {
        self.call(DbgCmd::Cont).map(|_| ())
    }
    pub fn step(&self, tid: u32) -> Result<(), String> {
        self.call(|r| DbgCmd::Step(tid, r)).map(|_| ())
    }
    pub fn registers(&self, tid: u32) -> Result<RegistersDto, String> {
        match self.call(|r| DbgCmd::Registers(tid, r))? {
            DbgReply::Regs(r) => Ok(RegistersDto::of(&r)),
            _ => Err("unexpected reply".into()),
        }
    }
    pub fn set_registers(&self, tid: u32, dto: RegistersDto) -> Result<(), String> {
        // Read-modify-write so segment registers stay intact.
        let mut regs = match self.call(|r| DbgCmd::Registers(tid, r))? {
            DbgReply::Regs(r) => *r,
            _ => return Err("unexpected reply".into()),
        };
        dto.apply(&mut regs);
        self.call(|r| DbgCmd::SetRegisters(tid, Box::new(regs), r))
            .map(|_| ())
    }
}

impl Drop for DebuggerHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(DbgCmd::Detach);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// Attaches a debugger on a dedicated thread; blocks until attach succeeds/fails.
pub fn start_debugger(
    app: AppHandle,
    pid: u32,
    backend: BackendKind,
) -> Result<DebuggerHandle, String> {
    let (tx, rx) = channel::<DbgCmd>();
    let (ready_tx, ready_rx) = channel::<Result<(), String>>();

    let join = std::thread::spawn(move || {
        let mut dbg: Box<dyn Debugger> = match debug::attach(pid, backend) {
            Ok(d) => {
                let _ = ready_tx.send(Ok(()));
                d
            }
            Err(e) => {
                let _ = ready_tx.send(Err(e.to_string()));
                return;
            }
        };
        debugger_loop(&app, &mut dbg, rx);
        let _ = dbg.detach();
    });

    ready_rx
        .recv()
        .map_err(|_| "debugger thread died".to_string())??;
    Ok(DebuggerHandle {
        tx,
        join: Some(join),
        backend,
    })
}

fn debugger_loop(app: &AppHandle, dbg: &mut Box<dyn Debugger>, rx: Receiver<DbgCmd>) {
    use std::sync::mpsc::TryRecvError;
    loop {
        loop {
            match rx.try_recv() {
                Ok(DbgCmd::Detach) => return,
                Ok(cmd) => handle_cmd(dbg, cmd),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        // Poll for a stop; a timeout is the common (idle) case.
        if let Ok(ev) = dbg.wait(Some(Duration::from_millis(50))) {
            let _ = app.emit("debugger:stopped", DebugEventDto::of(ev));
        }
    }
}

fn handle_cmd(dbg: &mut Box<dyn Debugger>, cmd: DbgCmd) {
    let s = |e: nemclass_sdk::SdkError| e.to_string();
    match cmd {
        DbgCmd::SetSwBp(addr, r) => {
            let _ = r.send(dbg.set_sw_breakpoint(addr).map(|i| DbgReply::BpId(i.0)).map_err(s));
        }
        DbgCmd::SetHwBp(addr, size, kind, r) => {
            let _ = r.send(
                dbg.set_hw_breakpoint(addr, size, kind)
                    .map(|i| DbgReply::BpId(i.0))
                    .map_err(s),
            );
        }
        DbgCmd::ClearBp(id, r) => {
            let _ = r.send(dbg.clear_breakpoint(BpId(id)).map(|_| DbgReply::Ok).map_err(s));
        }
        DbgCmd::Cont(r) => {
            let _ = r.send(dbg.cont().map(|_| DbgReply::Ok).map_err(s));
        }
        DbgCmd::Step(tid, r) => {
            let _ = r.send(dbg.step(ThreadId(tid)).map(|_| DbgReply::Ok).map_err(s));
        }
        DbgCmd::Threads(r) => {
            let _ = r.send(
                dbg.threads()
                    .map(|t| DbgReply::Threads(t.into_iter().map(|x| x.0).collect()))
                    .map_err(s),
            );
        }
        DbgCmd::Registers(tid, r) => {
            let _ = r.send(
                dbg.registers(ThreadId(tid))
                    .map(|regs| DbgReply::Regs(Box::new(regs)))
                    .map_err(s),
            );
        }
        DbgCmd::SetRegisters(tid, regs, r) => {
            let _ = r.send(
                dbg.set_registers(ThreadId(tid), &regs)
                    .map(|_| DbgReply::Ok)
                    .map_err(s),
            );
        }
        DbgCmd::Detach => {}
    }
}

// --- access tracer --------------------------------------------------------

/// Handle to a running access tracer thread.
pub struct AccessHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Drop for AccessHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// Starts a "find what accesses" tracer on its own thread; blocks until the
/// tracer has attached and started (so failures surface synchronously).
pub fn start_access(
    app: AppHandle,
    pid: u32,
    addr: usize,
    size: WatchSize,
    kind: WatchKind,
    backend: AccessBackend,
) -> Result<AccessHandle, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = channel::<Result<(), String>>();
    let stop_thread = stop.clone();

    let join = std::thread::spawn(move || {
        let mut tracer = match access::find_what_accesses(pid, backend) {
            Ok(t) => t,
            Err(e) => {
                let _ = ready_tx.send(Err(e.to_string()));
                return;
            }
        };
        if let Err(e) = tracer.start(addr, size, kind) {
            let _ = ready_tx.send(Err(e.to_string()));
            return;
        }
        let _ = ready_tx.send(Ok(()));

        while !stop_thread.load(Ordering::SeqCst) {
            if let Ok(records) = tracer.poll() {
                if !records.is_empty() {
                    let dto: Vec<AccessRecordDto> =
                        records.iter().map(AccessRecordDto::of).collect();
                    let _ = app.emit("access:hits", dto);
                }
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        let _ = tracer.stop();
    });

    ready_rx
        .recv()
        .map_err(|_| "access thread died".to_string())??;
    Ok(AccessHandle {
        stop,
        join: Some(join),
    })
}
