import { ArrowRightToLine, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { api, hexAddr, parseAddr, type Insn, type MapRegion } from "../lib/api";
import { useStore } from "../store";

const CHUNK = 200;
const MAX_ANALYSIS = 1 << 20; // 1 MiB

const FLOW_COLOR: Record<string, string> = {
  call: "text-accent",
  jump: "text-warn",
  condJump: "text-warn",
  ret: "text-danger",
  int: "text-[#c586ff]",
  bad: "text-faint",
  seq: "text-text",
};

type Tab = "strings" | "functions" | "calls";

export function DisasmPanel() {
  const attached = useStore((s) => s.attached);
  const guard = useStore((s) => s.guard);
  const [regions, setRegions] = useState<MapRegion[]>([]);
  const [insns, setInsns] = useState<Insn[]>([]);
  const [start, setStart] = useState<number | null>(null);
  const [gotoText, setGotoText] = useState("");
  const [analysisRegion, setAnalysisRegion] = useState<MapRegion | null>(null);
  const [tab, setTab] = useState<Tab>("strings");
  const [strings, setStrings] = useState<{ addr: number; text: string }[]>([]);
  const [funcs, setFuncs] = useState<number[]>([]);
  const [calls, setCalls] = useState<number[]>([]);
  const [busy, setBusy] = useState(false);

  const loadMap = async () => {
    const m = await api.memoryMap().catch(() => []);
    setRegions(m);
  };

  useEffect(() => {
    if (attached) loadMap();
    else {
      setRegions([]);
      setInsns([]);
    }
  }, [attached?.pid]);

  const disasmFrom = async (addr: number) => {
    setStart(addr);
    const r = await guard(() => api.disassemble(addr, CHUNK));
    if (r !== undefined) setInsns(r);
  };

  const loadMore = async () => {
    if (!insns.length) return;
    const last = insns[insns.length - 1];
    const next = last.addr + last.len;
    const r = await api.disassemble(next, CHUNK).catch(() => []);
    setInsns((prev) => [...prev, ...r]);
  };

  const selectRegion = (r: MapRegion) => {
    setAnalysisRegion(r);
    disasmFrom(r.from);
  };

  const doGoto = () => {
    const a = parseAddr(gotoText);
    if (a !== null) disasmFrom(a);
  };

  const runAnalysis = async (t: Tab) => {
    setTab(t);
    if (!analysisRegion) return;
    setBusy(true);
    const { from } = analysisRegion;
    const len = Math.min(analysisRegion.size, MAX_ANALYSIS);
    try {
      if (t === "strings") setStrings(await api.regionStrings(from, len, 4));
      else if (t === "functions") setFuncs(await api.regionFunctions(from, len));
      else setCalls(await api.regionCalls(from, len));
    } catch {
      /* surfaced elsewhere */
    }
    setBusy(false);
  };

  if (!attached) {
    return (
      <div className="panel-body flex items-center justify-center text-faint">
        Attach to a process to disassemble.
      </div>
    );
  }

  return (
    <div className="panel-body flex flex-col">
      {/* toolbar */}
      <div className="flex items-center gap-2 border-b border-border bg-panel-2 px-2 py-1.5">
        <input
          className="input mono w-48"
          placeholder="goto address 0x…"
          value={gotoText}
          onChange={(e) => setGotoText(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && doGoto()}
        />
        <button className="btn" onClick={doGoto}>
          <ArrowRightToLine size={13} /> Go
        </button>
        <div className="flex-1" />
        <button className="btn btn-ghost btn-icon" title="Refresh map" onClick={loadMap}>
          <RefreshCw size={13} />
        </button>
      </div>

      <div className="flex min-h-0 flex-1">
        {/* memory map */}
        <div className="mono w-56 shrink-0 overflow-auto border-r border-border text-[11px]">
          {regions.map((r) => (
            <button
              key={r.from}
              className={`row-hover flex w-full items-center gap-1 px-2 py-0.5 text-left ${
                analysisRegion?.from === r.from ? "bg-accent-soft" : ""
              }`}
              onClick={() => selectRegion(r)}
              title={`${hexAddr(r.from)}–${hexAddr(r.to)} ${r.name}`}
            >
              <span className={`w-8 ${r.exec ? "text-danger" : "text-faint"}`}>
                {r.read ? "r" : "-"}
                {r.write ? "w" : "-"}
                {r.exec ? "x" : "-"}
              </span>
              <span className="flex-1 truncate text-text">{r.label}</span>
            </button>
          ))}
          {!regions.length && (
            <div className="px-2 py-3 text-faint">No regions.</div>
          )}
        </div>

        {/* disassembly listing */}
        <div className="mono flex-1 overflow-auto text-[11px]">
          {insns.map((i) => (
            <div
              key={i.addr}
              className="row-hover grid grid-cols-[8rem_11rem_1fr] items-center gap-2 px-2 py-[1px]"
            >
              <span className="text-faint">{hexAddr(i.addr)}</span>
              <span className="truncate text-faint/70">{i.bytes}</span>
              <span className={FLOW_COLOR[i.kind] ?? "text-text"}>
                {i.text}
                {i.target != null && (
                  <button
                    className="ml-2 text-accent hover:underline"
                    onClick={() => disasmFrom(i.target!)}
                    title="Follow"
                  >
                    →{hexAddr(i.target)}
                  </button>
                )}
              </span>
            </div>
          ))}
          {insns.length > 0 && (
            <button className="btn btn-ghost m-2" onClick={loadMore}>
              Load more ↓
            </button>
          )}
          {!insns.length && (
            <div className="px-3 py-4 text-faint">
              Pick a region or enter an address.
            </div>
          )}
        </div>
      </div>

      {/* analysis tabs */}
      <div className="flex h-40 flex-col border-t border-border">
        <div className="flex items-center gap-1 bg-panel-2 px-2 py-1">
          {(["strings", "functions", "calls"] as Tab[]).map((t) => (
            <button
              key={t}
              className={`btn btn-ghost py-0.5 ${tab === t ? "text-accent" : "text-muted"}`}
              onClick={() => runAnalysis(t)}
            >
              {t}
            </button>
          ))}
          <span className="mono ml-2 text-[10px] text-faint">
            {analysisRegion ? analysisRegion.label : "select a region"}
            {busy && " · scanning…"}
          </span>
        </div>
        <div className="mono flex-1 overflow-auto px-2 py-1 text-[11px]">
          {tab === "strings" &&
            strings.map((s, i) => (
              <div
                key={i}
                className="row-hover flex cursor-pointer gap-2"
                onClick={() => disasmFrom(s.addr)}
              >
                <span className="w-32 shrink-0 text-faint">{hexAddr(s.addr)}</span>
                <span className="truncate text-success">{s.text}</span>
              </div>
            ))}
          {tab === "functions" &&
            funcs.map((a) => (
              <button
                key={a}
                className="row-hover mr-2 inline-block text-accent"
                onClick={() => disasmFrom(a)}
              >
                {hexAddr(a)}
              </button>
            ))}
          {tab === "calls" &&
            calls.map((a) => (
              <button
                key={a}
                className="row-hover mr-2 inline-block text-accent"
                onClick={() => disasmFrom(a)}
              >
                {hexAddr(a)}
              </button>
            ))}
        </div>
      </div>
    </div>
  );
}
