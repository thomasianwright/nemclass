import { Plus, RotateCcw, Search } from "lucide-react";
import { useEffect, useState } from "react";
import { api, hexAddr, type ScanRow } from "../lib/api";
import { useStore } from "../store";

const SCAN_TYPES = ["I8", "U8", "I16", "U16", "I32", "U32", "I64", "U64", "F32", "F64", "Bytes"];

const FIRST_OPS = [
  ["exact", "equal to"],
  ["unknown", "unknown initial"],
  ["between", "between"],
  ["greater", "greater than"],
  ["less", "less than"],
];
const NEXT_OPS = [
  ["exact", "equal to"],
  ["changed", "changed"],
  ["unchanged", "unchanged"],
  ["increased", "increased"],
  ["decreased", "decreased"],
  ["greater", "greater than"],
  ["less", "less than"],
  ["between", "between"],
  ["increasedBy", "increased by"],
  ["decreasedBy", "decreased by"],
];

const NEEDS_VALUE = (op: string) =>
  !["unknown", "changed", "unchanged", "increased", "decreased"].includes(op);

export function ScannerPanel() {
  const attached = useStore((s) => s.attached);
  const guard = useStore((s) => s.guard);
  const toast = useStore((s) => s.toast);

  const [valueType, setValueType] = useState("I32");
  const [op, setOp] = useState("exact");
  const [value, setValue] = useState("");
  const [value2, setValue2] = useState("");
  const [writableOnly, setWritableOnly] = useState(true);
  const [firstDone, setFirstDone] = useState(false);
  const [count, setCount] = useState(0);
  const [rows, setRows] = useState<ScanRow[]>([]);
  const [busy, setBusy] = useState(false);

  const ops = firstDone ? NEXT_OPS : FIRST_OPS;

  useEffect(() => {
    if (!ops.some(([o]) => o === op)) setOp(ops[0][0]);
  }, [firstDone]);

  const loadPage = async () => setRows(await api.scanPage(0, 300).catch(() => []));

  // Live-refresh visible values.
  useEffect(() => {
    if (!firstDone || !count) return;
    const id = setInterval(loadPage, 500);
    return () => clearInterval(id);
  }, [firstDone, count]);

  const compare = () => ({ op, value: value || null, value2: value2 || null });

  const first = async () => {
    setBusy(true);
    const s = await guard(() => api.scanFirst(valueType, compare(), writableOnly));
    setBusy(false);
    if (s !== undefined) {
      setCount(s.count);
      setFirstDone(true);
      await loadPage();
    }
  };

  const next = async () => {
    setBusy(true);
    const s = await guard(() => api.scanNext(compare()));
    setBusy(false);
    if (s !== undefined) {
      setCount(s.count);
      await loadPage();
    }
  };

  const reset = async () => {
    await api.scanReset().catch(() => {});
    setFirstDone(false);
    setRows([]);
    setCount(0);
  };

  const addRow = async (index: number) => {
    const ok = await guard(() => api.scanAddToTable(index, ""));
    if (ok !== undefined) toast("Added to cheat table", "success");
  };

  if (!attached) {
    return (
      <div className="panel-body flex items-center justify-center text-faint">
        Attach to a process to scan.
      </div>
    );
  }

  return (
    <div className="panel-body flex flex-col">
      <div className="flex flex-col gap-2 border-b border-border bg-panel-2 p-2">
        <div className="flex items-center gap-1.5">
          <select
            className="input mono w-20"
            value={valueType}
            disabled={firstDone}
            onChange={(e) => setValueType(e.target.value)}
          >
            {SCAN_TYPES.map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
          <select className="input flex-1" value={op} onChange={(e) => setOp(e.target.value)}>
            {ops.map(([o, label]) => (
              <option key={o} value={o}>
                {label}
              </option>
            ))}
          </select>
        </div>
        {NEEDS_VALUE(op) && (
          <div className="flex items-center gap-1.5">
            <input
              className="input mono flex-1"
              placeholder={valueType === "Bytes" ? "AA BB CC" : "value"}
              value={value}
              onChange={(e) => setValue(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && (firstDone ? next() : first())}
            />
            {op === "between" && (
              <input
                className="input mono flex-1"
                placeholder="max"
                value={value2}
                onChange={(e) => setValue2(e.target.value)}
              />
            )}
          </div>
        )}
        <div className="flex items-center gap-2">
          {!firstDone && (
            <label className="flex items-center gap-1.5 text-xs text-muted">
              <input
                type="checkbox"
                checked={writableOnly}
                onChange={(e) => setWritableOnly(e.target.checked)}
                className="accent-accent"
              />
              writable only
            </label>
          )}
          <div className="flex-1" />
          {firstDone && (
            <button className="btn" onClick={reset} title="New scan">
              <RotateCcw size={13} /> New
            </button>
          )}
          <button
            className="btn btn-primary"
            disabled={busy}
            onClick={firstDone ? next : first}
          >
            <Search size={13} /> {firstDone ? "Next Scan" : "First Scan"}
          </button>
        </div>
      </div>

      <div className="mono flex items-center justify-between border-b border-border-soft bg-panel px-2 py-1 text-[10px] uppercase tracking-wide text-faint">
        <span>{count.toLocaleString()} results{busy ? " · scanning…" : ""}</span>
        <span>showing {rows.length}</span>
      </div>

      <div className="flex-1 overflow-auto">
        {rows.map((r, i) => (
          <div
            key={r.address}
            className="mono group row-hover grid grid-cols-[8rem_1fr_1fr_1.5rem] items-center gap-2 border-b border-border-soft px-2 py-0.5 text-xs"
            onDoubleClick={() => addRow(i)}
          >
            <span className="text-accent">{hexAddr(r.address)}</span>
            <span className="truncate text-success">{r.value ?? "—"}</span>
            <span className="truncate text-faint">{r.previous}</span>
            <button
              className="btn-ghost btn-icon hidden text-faint hover:text-accent group-hover:block"
              title="Add to cheat table"
              onClick={() => addRow(i)}
            >
              <Plus size={12} />
            </button>
          </div>
        ))}
        {firstDone && !rows.length && (
          <div className="px-3 py-4 text-center text-xs text-faint">No results.</div>
        )}
        {!firstDone && (
          <div className="px-3 py-4 text-center text-xs text-faint">
            Configure a scan and press First Scan.
          </div>
        )}
      </div>
    </div>
  );
}
