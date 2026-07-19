import { Plus } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, hexAddr, parseAddr, type InspectResult } from "../lib/api";
import { useStore } from "../store";
import { InspectorRow } from "./inspector/InspectorRow";

const POLL_MS = 150;

export function InspectorPanel() {
  const selected = useStore((s) => s.selectedClass);
  const classRev = useStore((s) => s.classRev);
  const kinds = useStore((s) => s.kinds);
  const guard = useStore((s) => s.guard);
  const mutated = useStore((s) => s.mutated);

  const [result, setResult] = useState<InspectResult | null>(null);
  const [baseText, setBaseText] = useState("0x0");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [newField, setNewField] = useState("");
  const [newKind, setNewKind] = useState("I32");
  // True while the user edits the base field, so live syncs don't clobber typing.
  const editingBase = useRef(false);

  const base = result?.baseAddr ?? 0;
  const expandedArr = useMemo(
    () => [...expanded].map((k) => k.split(".").filter(Boolean).map(Number)),
    [expanded],
  );

  // Live polling of structure + values. The base comes from the backend
  // (class_addresses), so a script's nem.set_class_address shows up here too.
  useEffect(() => {
    if (!selected) {
      setResult(null);
      return;
    }
    let alive = true;
    const tick = async () => {
      const r = await api.inspectClass(selected, expandedArr).catch(() => null);
      if (alive && r) {
        setResult(r);
        if (!editingBase.current) setBaseText(hexAddr(r.baseAddr));
      }
    };
    tick();
    const id = setInterval(tick, POLL_MS);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [selected, expandedArr, classRev]);

  const commitBase = async () => {
    editingBase.current = false;
    const v = parseAddr(baseText);
    if (v !== null && selected) await api.setClassAddress(selected, v);
  };

  const toggle = (key: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      next.has(key) ? next.delete(key) : next.add(key);
      return next;
    });

  const addField = async () => {
    if (!selected) return;
    const name = newField.trim() || `field_${result?.rows.length ?? 0}`;
    const ok = await guard(() => api.addField(selected, name, newKind));
    if (ok !== undefined) {
      setNewField("");
      await mutated();
    }
  };

  if (!selected) {
    return (
      <div className="panel-body flex items-center justify-center text-faint">
        Select or create a class to inspect.
      </div>
    );
  }

  const rows = result?.rows ?? [];

  return (
    <div className="panel-body flex flex-col">
      {/* base address bar */}
      <div className="flex items-center gap-2 border-b border-border bg-panel-2 px-2 py-1.5">
        <span className="mono text-sm font-semibold text-text">{selected}</span>
        <span
          className={`chip ${result?.attached ? "bg-success/15 text-success" : "bg-elevated text-faint"}`}
        >
          {result?.attached ? "live" : "detached"}
        </span>
        <div className="flex-1" />
        <label className="flex items-center gap-1.5 text-xs text-muted">
          base
          <input
            className="input mono w-44"
            value={baseText}
            spellCheck={false}
            onFocus={() => (editingBase.current = true)}
            onChange={(e) => setBaseText(e.target.value)}
            onBlur={commitBase}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          />
        </label>
      </div>

      {/* grid header */}
      <div className="mono grid grid-cols-[1.2rem_4.5rem_7.5rem_6.5rem_1fr_10rem_1.5rem] items-center gap-2 border-b border-border-soft bg-panel px-2 py-1 text-[10px] uppercase tracking-wide text-faint">
        <span />
        <span>offset</span>
        <span>address</span>
        <span>type</span>
        <span>name</span>
        <span>value</span>
        <span />
      </div>

      <div className="flex-1 overflow-auto">
        {rows.map((row) => (
          <InspectorRow
            key={row.fieldIndex}
            row={row}
            ownerClass={selected}
            path={[row.fieldIndex]}
            depth={0}
            base={base}
            kinds={kinds}
            expanded={expanded}
            onToggle={toggle}
          />
        ))}
        {!rows.length && (
          <div className="px-3 py-4 text-center text-xs text-faint">
            No fields. Add one below.
          </div>
        )}
      </div>

      {/* add-field bar */}
      <div className="flex items-center gap-1.5 border-t border-border bg-panel-2 px-2 py-1.5">
        <input
          className="input mono flex-1"
          placeholder="new field name…"
          value={newField}
          onChange={(e) => setNewField(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && addField()}
        />
        <select
          className="input mono w-28"
          value={newKind}
          onChange={(e) => setNewKind(e.target.value)}
        >
          {kinds.map((k) => (
            <option key={k.kind} value={k.kind}>
              {k.kind}
            </option>
          ))}
        </select>
        <button className="btn btn-primary btn-icon" title="Add field" onClick={addField}>
          <Plus size={14} />
        </button>
      </div>

      <div className="mono border-t border-border-soft bg-panel px-2 py-0.5 text-[10px] text-faint">
        base {hexAddr(base)} · {rows.length} fields
        {result && ` · ${result.ptrSize * 8}-bit`}
      </div>
    </div>
  );
}
