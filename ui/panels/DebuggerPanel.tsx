import { Bug, Play, StepForward, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  api,
  hexAddr,
  parseAddr,
  type AccessRecord,
  type DebugEvent,
  type Registers,
} from "../lib/api";
import { on } from "../lib/events";
import { useStore } from "../store";

const REG_ORDER: (keyof Registers)[] = [
  "rip", "rsp", "rbp", "rax", "rbx", "rcx", "rdx",
  "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15", "eflags",
];

interface Bp {
  id: number;
  addr: number;
  hw: boolean;
  kind: string;
}

export function DebuggerPanel() {
  const attached = useStore((s) => s.attached);
  const guard = useStore((s) => s.guard);
  const toast = useStore((s) => s.toast);

  const [backend, setBackend] = useState("ptrace");
  const [dbgOn, setDbgOn] = useState(false);
  const [threads, setThreads] = useState<number[]>([]);
  const [tid, setTid] = useState<number | null>(null);
  const [regs, setRegs] = useState<Registers | null>(null);
  const [events, setEvents] = useState<DebugEvent[]>([]);
  const [bps, setBps] = useState<Bp[]>([]);

  const [bpAddr, setBpAddr] = useState("");
  const [bpHw, setBpHw] = useState(false);
  const [bpKind, setBpKind] = useState("execute");
  const [bpSize, setBpSize] = useState("4");

  // access tracer
  const [accAddr, setAccAddr] = useState("");
  const [accSize, setAccSize] = useState("4");
  const [accKind, setAccKind] = useState("write");
  const [accBackend, setAccBackend] = useState("hardware");
  const [accOn, setAccOn] = useState(false);
  const [records, setRecords] = useState<AccessRecord[]>([]);

  const refreshRegs = async (t: number) => {
    const r = await api.dbgRegisters(t).catch(() => null);
    if (r) setRegs(r);
  };

  // Event subscriptions.
  useEffect(() => {
    const unsubs: Promise<() => void>[] = [
      on<DebugEvent>("debugger:stopped", (ev) => {
        setEvents((prev) => [ev, ...prev].slice(0, 100));
        setTid(ev.tid);
        refreshRegs(ev.tid);
      }),
      on<AccessRecord[]>("access:hits", (recs) => setRecords(recs)),
    ];
    return () => {
      unsubs.forEach((p) => p.then((u) => u()));
    };
  }, []);

  const attach = async () => {
    const ok = await guard(() => api.debuggerAttach(backend), `Debugger attached (${backend})`);
    if (ok !== undefined) {
      setDbgOn(true);
      const t = await api.debuggerThreads().catch(() => []);
      setThreads(t);
      if (t.length) {
        setTid(t[0]);
        refreshRegs(t[0]);
      }
    }
  };
  const detach = async () => {
    await api.debuggerDetach().catch(() => {});
    setDbgOn(false);
    setThreads([]);
    setRegs(null);
    setBps([]);
  };

  const addBp = async () => {
    const a = parseAddr(bpAddr);
    if (a === null) return;
    const id = await guard(() =>
      bpHw ? api.bpSetHw(a, parseInt(bpSize) || 4, bpKind) : api.bpSetSw(a),
    );
    if (typeof id === "number") {
      setBps((prev) => [...prev, { id, addr: a, hw: bpHw, kind: bpHw ? bpKind : "sw" }]);
      setBpAddr("");
    }
  };
  const clearBp = async (id: number) => {
    await api.bpClear(id).catch(() => {});
    setBps((prev) => prev.filter((b) => b.id !== id));
  };

  const startAccess = async () => {
    const a = parseAddr(accAddr);
    if (a === null) return;
    const ok = await guard(() =>
      api.accessStart(a, parseInt(accSize) || 4, accKind, accBackend),
    );
    if (ok !== undefined) {
      setAccOn(true);
      setRecords([]);
    }
  };
  const stopAccess = async () => {
    await api.accessStop().catch(() => {});
    setAccOn(false);
  };

  const editReg = async (key: keyof Registers, text: string) => {
    if (!regs || tid == null) return;
    const v = parseAddr(text);
    if (v === null || v === regs[key]) return;
    const next = { ...regs, [key]: v };
    const ok = await guard(() => api.dbgSetRegisters(tid, next));
    if (ok !== undefined) setRegs(next);
  };

  if (!attached) {
    return (
      <div className="panel-body flex items-center justify-center text-faint">
        Attach to a process first.
      </div>
    );
  }

  return (
    <div className="panel-body flex flex-col overflow-auto">
      {/* attach bar */}
      <div className="flex items-center gap-2 border-b border-border bg-panel-2 px-2 py-1.5">
        <select
          className="input mono w-24"
          value={backend}
          disabled={dbgOn}
          onChange={(e) => setBackend(e.target.value)}
        >
          <option value="ptrace">ptrace</option>
          <option value="frida">frida</option>
        </select>
        {dbgOn ? (
          <>
            <button className="btn" onClick={() => guard(() => api.dbgContinue())}>
              <Play size={13} /> Continue
            </button>
            <button
              className="btn"
              onClick={() => tid != null && guard(() => api.dbgStep(tid))}
            >
              <StepForward size={13} /> Step
            </button>
            <div className="flex-1" />
            <button className="btn" onClick={detach}>
              <X size={13} /> Detach
            </button>
          </>
        ) : (
          <>
            <button className="btn btn-primary" onClick={attach}>
              <Bug size={13} /> Attach debugger
            </button>
            <div className="flex-1" />
          </>
        )}
      </div>

      {dbgOn && (
        <>
          {/* breakpoints */}
          <Section title="Breakpoints">
            <div className="flex items-center gap-1.5 pb-1.5">
              <input
                className="input mono flex-1"
                placeholder="address 0x…"
                value={bpAddr}
                onChange={(e) => setBpAddr(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && addBp()}
              />
              <label className="flex items-center gap-1 text-[10px] text-muted">
                <input type="checkbox" checked={bpHw} onChange={(e) => setBpHw(e.target.checked)} />
                hw
              </label>
              {bpHw && (
                <>
                  <select className="input w-24" value={bpKind} onChange={(e) => setBpKind(e.target.value)}>
                    <option value="execute">execute</option>
                    <option value="write">write</option>
                    <option value="readwrite">read/write</option>
                  </select>
                  <select className="input w-14" value={bpSize} onChange={(e) => setBpSize(e.target.value)}>
                    {["1", "2", "4", "8"].map((s) => (
                      <option key={s} value={s}>{s}</option>
                    ))}
                  </select>
                </>
              )}
              <button className="btn btn-primary" onClick={addBp}>Add</button>
            </div>
            {bps.map((b) => (
              <div key={b.id} className="mono group flex items-center gap-2 py-0.5 text-xs">
                <span className="text-accent">{hexAddr(b.addr)}</span>
                <span className="chip bg-elevated text-muted">{b.hw ? `hw:${b.kind}` : "sw"}</span>
                <div className="flex-1" />
                <button
                  className="btn-ghost btn-icon text-faint hover:text-danger"
                  onClick={() => clearBp(b.id)}
                >
                  <Trash2 size={12} />
                </button>
              </div>
            ))}
          </Section>

          {/* threads + registers */}
          <Section title="Registers">
            <div className="flex items-center gap-2 pb-1.5">
              <span className="text-[10px] text-muted">thread</span>
              <select
                className="input mono w-28"
                value={tid ?? ""}
                onChange={(e) => {
                  const t = Number(e.target.value);
                  setTid(t);
                  refreshRegs(t);
                }}
              >
                {threads.map((t) => (
                  <option key={t} value={t}>{t}</option>
                ))}
              </select>
              <button className="btn btn-ghost" onClick={() => tid != null && refreshRegs(tid)}>
                refresh
              </button>
            </div>
            {regs && (
              <div className="mono grid grid-cols-2 gap-x-3 gap-y-0.5 text-[11px]">
                {REG_ORDER.map((k) => (
                  <div key={k} className="flex items-center gap-1">
                    <span className="w-12 text-faint">{k}</span>
                    <input
                      className="flex-1 bg-transparent text-success outline-none focus:text-accent"
                      defaultValue={hexAddr(regs[k])}
                      key={`${k}-${regs[k]}`}
                      spellCheck={false}
                      onBlur={(e) => editReg(k, e.target.value)}
                      onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                    />
                  </div>
                ))}
              </div>
            )}
          </Section>

          {/* stop events */}
          <Section title="Events">
            <div className="mono max-h-24 overflow-auto text-[11px]">
              {events.map((e, i) => (
                <div key={i} className="flex gap-2 py-px">
                  <span className="text-warn">{e.reason}</span>
                  <span className="text-faint">tid {e.tid}</span>
                  {e.addr != null && <span className="text-accent">{hexAddr(e.addr)}</span>}
                  {e.exitCode != null && <span className="text-danger">code {e.exitCode}</span>}
                </div>
              ))}
              {!events.length && <span className="text-faint">no stops yet</span>}
            </div>
          </Section>
        </>
      )}

      {/* find what accesses */}
      <Section title="Find what accesses">
        <div className="flex flex-wrap items-center gap-1.5 pb-1.5">
          <input
            className="input mono w-40"
            placeholder="address 0x…"
            value={accAddr}
            onChange={(e) => setAccAddr(e.target.value)}
          />
          <select className="input w-14" value={accSize} onChange={(e) => setAccSize(e.target.value)}>
            {["1", "2", "4", "8"].map((s) => <option key={s} value={s}>{s}</option>)}
          </select>
          <select className="input w-24" value={accKind} onChange={(e) => setAccKind(e.target.value)}>
            <option value="write">write</option>
            <option value="readwrite">read/write</option>
            <option value="execute">execute</option>
          </select>
          <select className="input w-24" value={accBackend} onChange={(e) => setAccBackend(e.target.value)}>
            <option value="hardware">hardware</option>
            <option value="libiht">libiht</option>
            <option value="intelpt">intel-pt</option>
          </select>
          {accOn ? (
            <button className="btn" onClick={stopAccess}><X size={12} /> Stop</button>
          ) : (
            <button className="btn btn-primary" onClick={startAccess}>Start</button>
          )}
        </div>
        <div className="mono max-h-40 overflow-auto text-[11px]">
          {records.map((r) => (
            <div key={r.insnAddr} className="flex gap-2 py-px">
              <span className="text-accent">{hexAddr(r.insnAddr)}</span>
              <span className="text-faint">×{r.hits}</span>
              <span className="text-muted">rip {hexAddr(r.regs.rip)}</span>
            </div>
          ))}
          {accOn && !records.length && <span className="text-faint">watching… trigger the access</span>}
        </div>
      </Section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="border-b border-border-soft px-2 py-1.5">
      <div className="mb-1 text-[10px] font-semibold uppercase tracking-wide text-faint">
        {title}
      </div>
      {children}
    </div>
  );
}
