import { Play, Plus, Radar, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, hexAddr, parseAddr, type SpiderResult } from "../lib/api";
import { useStore } from "../store";

const SCALAR_TYPES = ["I8", "U8", "I16", "U16", "I32", "U32", "I64", "U64", "F32", "F64"];
const FILTERS = [
  ["greater", "greater"],
  ["greaterEq", "greater/eq"],
  ["less", "less"],
  ["lessEq", "less/eq"],
  ["equal", "equal"],
  ["notEqual", "not equal"],
  ["changed", "changed"],
  ["unchanged", "unchanged"],
];

export function SpiderPanel() {
  const attached = useStore((s) => s.attached);
  const guard = useStore((s) => s.guard);
  const toast = useStore((s) => s.toast);

  const [addr, setAddr] = useState("");
  const [structSize, setStructSize] = useState("0x1000");
  const [alignment, setAlignment] = useState("4");
  const [depth, setDepth] = useState("3");
  const [kind, setKind] = useState("I32");
  const [value, setValue] = useState("");

  const [started, setStarted] = useState(false);
  const [running, setRunning] = useState(false);
  const [count, setCount] = useState(0);
  const [rows, setRows] = useState<SpiderResult[]>([]);
  const [filterOp, setFilterOp] = useState("greater");
  const [filterVal, setFilterVal] = useState("");
  const poll = useRef<number | null>(null);

  // Poll status + first page while a search exists.
  useEffect(() => {
    if (!started) return;
    const tick = async () => {
      const s = await api.spiderStatus().catch(() => null);
      if (s) {
        setRunning(s.running);
        setCount(s.count);
      }
      setRows(await api.spiderPage(0, 300).catch(() => []));
    };
    tick();
    poll.current = window.setInterval(tick, 400);
    return () => {
      if (poll.current) clearInterval(poll.current);
    };
  }, [started]);

  const search = async () => {
    const a = parseAddr(addr);
    if (a === null) {
      toast("Enter a valid base address", "error");
      return;
    }
    const ok = await guard(() =>
      api.spiderSearch(
        a,
        parseAddr(structSize) ?? 0x1000,
        parseInt(alignment) || 4,
        parseInt(depth) || 3,
        kind,
        value,
      ),
    );
    if (ok !== undefined) setStarted(true);
  };

  const cancel = async () => {
    await api.spiderCancel().catch(() => {});
  };

  const filter = async () => {
    const n = await guard(() => api.spiderFilter(filterOp, filterVal));
    if (n !== undefined) setRows(await api.spiderPage(0, 300).catch(() => []));
  };

  const add = async (index: number) => {
    const ok = await guard(() => api.spiderAddToTable(index, ""));
    if (ok !== undefined) toast("Added to cheat table", "success");
  };

  if (!attached) {
    return (
      <div className="panel-body flex items-center justify-center text-faint">
        Attach to a process to run the spider.
      </div>
    );
  }

  return (
    <div className="panel-body flex flex-col">
      <div className="flex flex-col gap-2 border-b border-border bg-panel-2 p-2">
        <div className="flex items-center gap-1.5">
          <input
            className="input mono flex-1"
            placeholder="base address 0x…"
            value={addr}
            onChange={(e) => setAddr(e.target.value)}
          />
          <input
            className="input mono w-24"
            title="value to find"
            placeholder="value"
            value={value}
            onChange={(e) => setValue(e.target.value)}
          />
        </div>
        <div className="mono grid grid-cols-4 gap-1.5 text-[10px] text-faint">
          <label className="flex flex-col gap-0.5">
            size
            <input className="input" value={structSize} onChange={(e) => setStructSize(e.target.value)} />
          </label>
          <label className="flex flex-col gap-0.5">
            align
            <input className="input" value={alignment} onChange={(e) => setAlignment(e.target.value)} />
          </label>
          <label className="flex flex-col gap-0.5">
            depth
            <input className="input" value={depth} onChange={(e) => setDepth(e.target.value)} />
          </label>
          <label className="flex flex-col gap-0.5">
            type
            <select className="input" value={kind} onChange={(e) => setKind(e.target.value)}>
              {SCALAR_TYPES.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="flex items-center gap-2">
          <span className="mono text-[10px] text-faint">
            {count.toLocaleString()} paths{running ? " · searching…" : ""}
          </span>
          <div className="flex-1" />
          {running ? (
            <button className="btn" onClick={cancel}>
              <X size={13} /> Stop
            </button>
          ) : (
            <button className="btn btn-primary" onClick={search}>
              <Radar size={13} /> Search
            </button>
          )}
        </div>
      </div>

      {started && !running && count > 0 && (
        <div className="flex items-center gap-1.5 border-b border-border-soft bg-panel px-2 py-1.5">
          <select className="input flex-1" value={filterOp} onChange={(e) => setFilterOp(e.target.value)}>
            {FILTERS.map(([o, l]) => (
              <option key={o} value={o}>
                {l}
              </option>
            ))}
          </select>
          <input
            className="input mono w-24"
            placeholder="value"
            value={filterVal}
            onChange={(e) => setFilterVal(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && filter()}
          />
          <button className="btn" onClick={filter}>
            <Play size={12} /> Filter
          </button>
        </div>
      )}

      <div className="flex-1 overflow-auto">
        {rows.map((r, i) => (
          <div
            key={r.expr}
            className="mono group row-hover grid grid-cols-[1fr_7rem_6rem_1.5rem] items-center gap-2 border-b border-border-soft px-2 py-0.5 text-xs"
            onDoubleClick={() => add(i)}
          >
            <span className="truncate text-accent" title={r.expr}>
              {r.expr}
            </span>
            <span className="text-faint">{r.address != null ? hexAddr(r.address) : "—"}</span>
            <span className="truncate text-success">{r.value ?? "—"}</span>
            <button
              className="btn-ghost btn-icon hidden text-faint hover:text-accent group-hover:block"
              title="Add to cheat table"
              onClick={() => add(i)}
            >
              <Plus size={12} />
            </button>
          </div>
        ))}
        {started && !rows.length && (
          <div className="px-3 py-4 text-center text-xs text-faint">
            {running ? "Searching…" : "No pointer paths found."}
          </div>
        )}
        {!started && (
          <div className="px-3 py-4 text-center text-xs text-faint">
            Enter a base address + value, then Search.
          </div>
        )}
      </div>
    </div>
  );
}
