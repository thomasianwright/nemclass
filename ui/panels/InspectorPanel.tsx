import {
  type MouseEvent as ReactMouseEvent,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { api, hexAddr, parseAddr, type InspectResult } from "../lib/api";
import {
  clipRead,
  clipWrite,
  kindColor,
  parsePayload,
  type FieldSel,
} from "../lib/inspector";
import { useStore } from "../store";
import { InspectorRow } from "./inspector/InspectorRow";

const POLL_MS = 150;
const GRID = "grid grid-cols-[1.1rem_3.5rem_6.5rem_5rem_9rem_1fr] items-center gap-2";

// ReClass byte/field granularities and the scalar type palette (egui toolbar).
const ADD_STEPS = [8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096];
const INS_STEPS = [1, 2, 4, 8, 16, 64, 256, 1024];
const REM_STEPS = [1, 2, 4, 16, 64, 256, 1024];
const TYPE_GROUPS: string[][] = [
  ["Bool"],
  ["U8", "U16", "U32", "U64"],
  ["I8", "I16", "I32", "I64"],
  ["F32", "F64"],
  ["Hex8", "Hex16", "Hex32", "Hex64"],
  ["Ptr", "StrPtr"],
];

interface Menu {
  x: number;
  y: number;
  sel: FieldSel;
}

export function InspectorPanel() {
  const selectedClass = useStore((s) => s.selectedClass);
  const classRev = useStore((s) => s.classRev);
  const kinds = useStore((s) => s.kinds);
  const guard = useStore((s) => s.guard);
  const mutated = useStore((s) => s.mutated);
  const toast = useStore((s) => s.toast);

  const [result, setResult] = useState<InspectResult | null>(null);
  const [baseText, setBaseText] = useState("0x0");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [fieldSel, setFieldSel] = useState<FieldSel | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);
  // True while the user edits the base field, so live syncs don't clobber typing.
  const editingBase = useRef(false);

  const base = result?.baseAddr ?? 0;
  const expandedArr = useMemo(
    () => [...expanded].map((k) => k.split(".").filter(Boolean).map(Number)),
    [expanded],
  );
  const vecMatKinds = useMemo(
    () => kinds.filter((k) => k.kind.startsWith("Vec") || k.kind.startsWith("Mat")),
    [kinds],
  );

  // Live polling of structure + values (also re-runs on classRev after any edit).
  useEffect(() => {
    if (!selectedClass) {
      setResult(null);
      return;
    }
    let alive = true;
    const tick = async () => {
      const r = await api.inspectClass(selectedClass, expandedArr).catch(() => null);
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
  }, [selectedClass, expandedArr, classRev]);

  // Reset per-field state when the class changes.
  useEffect(() => {
    setFieldSel(null);
    setMenu(null);
  }, [selectedClass]);

  // Dismiss the context menu on any outside click / Escape.
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const onEsc = (e: KeyboardEvent) => e.key === "Escape" && setMenu(null);
    window.addEventListener("click", close);
    window.addEventListener("keydown", onEsc);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("keydown", onEsc);
    };
  }, [menu]);

  // Keyboard copy on the selected field (egui: Ctrl+C address, Ctrl+Shift+C 8 bytes).
  useEffect(() => {
    const onKey = async (e: KeyboardEvent) => {
      if (!fieldSel || !e.ctrlKey || e.key.toLowerCase() !== "c") return;
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA") return;
      e.preventDefault();
      if (e.shiftKey) {
        const b = await api.readBytes(fieldSel.address, 8).catch(() => null);
        if (b) {
          let v = 0n;
          for (let i = 7; i >= 0; i--) v = (v << 8n) | BigInt(b[i] ?? 0);
          if (await clipWrite(v.toString(16).toUpperCase())) toast("Copied 8 bytes");
        }
      } else {
        if (await clipWrite(fieldSel.address.toString(16).toUpperCase()))
          toast("Copied address");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [fieldSel, toast]);

  const commitBase = async () => {
    editingBase.current = false;
    const v = parseAddr(baseText);
    if (v !== null && selectedClass) await api.setClassAddress(selectedClass, v);
  };

  const toggle = (key: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      next.has(key) ? next.delete(key) : next.add(key);
      return next;
    });

  // ---- schema toolbar (acts on the selected field / class) ----
  const doAdd = async (n: number) => {
    const cls = fieldSel?.ownerClass ?? selectedClass;
    if (!cls) return;
    await guard(() => api.addBytes(cls, n));
    await mutated();
  };
  const doInsert = async (n: number) => {
    if (!fieldSel) return;
    await guard(() => api.insertBytes(fieldSel.ownerClass, fieldSel.index, n));
    setFieldSel(null);
    await mutated();
  };
  const doRemove = async (n: number) => {
    if (!fieldSel) return;
    await guard(() => api.removeFields(fieldSel.ownerClass, fieldSel.index, n));
    setFieldSel(null);
    await mutated();
  };
  const doRetype = async (kind: string) => {
    if (!fieldSel) return;
    await guard(() => api.retypeField(fieldSel.ownerClass, fieldSel.index, kind, null));
    await mutated();
  };
  const doGuess = async (sel: FieldSel) => {
    const k = await api.guessType(sel.ownerClass, sel.index, sel.address).catch(() => null);
    if (k) {
      await mutated();
      toast(`Guessed ${k}`, "success");
    } else {
      toast("Couldn't infer a more specific type");
    }
  };

  // ---- context menu (copy/paste/guess) ----
  const openMenu = (e: ReactMouseEvent, sel: FieldSel) => {
    e.preventDefault();
    e.stopPropagation();
    setFieldSel(sel);
    setMenu({ x: e.clientX, y: e.clientY, sel });
  };
  const copyField = async (sel: FieldSel) => {
    const payload = {
      t: "fields",
      fields: [
        { name: sel.name || "field", offset: 0, kind: sel.kind, metadata: sel.metadata },
      ],
    };
    if (await clipWrite(JSON.stringify(payload))) toast("Copied field");
  };
  const copyValue = async (sel: FieldSel) => {
    const bytes = await api.readBytes(sel.address, sel.size).catch(() => null);
    if (bytes && (await clipWrite(JSON.stringify({ t: "value", bytes })))) toast("Copied value");
  };
  const copyAddress = async (sel: FieldSel) => {
    if (await clipWrite(JSON.stringify({ t: "address", address: sel.address })))
      toast("Copied address");
  };
  const doPaste = async (sel: FieldSel) => {
    const text = await clipRead();
    if (text === null) {
      toast("Clipboard unavailable", "error");
      return;
    }
    const p = parsePayload(text);
    if (!p) {
      toast("Clipboard has no pasteable content", "error");
      return;
    }
    if (p.t === "fields") {
      await guard(() => api.insertFields(sel.ownerClass, sel.index + 1, p.fields));
      await mutated();
    } else if (p.t === "value") {
      await guard(() => api.writeBytes(sel.address, p.bytes));
    } else if (selectedClass) {
      await guard(() => api.setClassAddress(selectedClass, p.address));
    }
  };

  if (!selectedClass) {
    return (
      <div className="panel-body flex items-center justify-center text-faint">
        Select or create a class to inspect.
      </div>
    );
  }

  const rows = result?.rows ?? [];
  const hasField = fieldSel != null;

  return (
    <div className="panel-body flex flex-col">
      {/* base address bar */}
      <div className="flex items-center gap-2 border-b border-border bg-panel-2 px-2 py-1.5">
        <span className="mono text-sm font-semibold text-text">{selectedClass}</span>
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

      {/* schema toolbar: byte add/insert/remove + type change on the selected field */}
      <div className="flex flex-wrap items-center gap-1 border-b border-border bg-panel px-2 py-1 text-xs">
        <StepSelect label="Add" steps={ADD_STEPS} onPick={doAdd} disabled={false} />
        <StepSelect label="Insert" steps={INS_STEPS} onPick={doInsert} disabled={!hasField} />
        <StepSelect label="Remove" steps={REM_STEPS} onPick={doRemove} disabled={!hasField} />
        <span className="mx-1 h-4 w-px bg-border" />
        {TYPE_GROUPS.map((group, gi) => (
          <span key={gi} className="flex items-center gap-0.5">
            {gi > 0 && <span className="mx-0.5 h-4 w-px bg-border-soft" />}
            {group.map((k) => (
              <button
                key={k}
                className={`rounded border border-border bg-elevated px-1.5 py-0.5 hover:border-accent-soft disabled:opacity-40 ${kindColor(k)}`}
                disabled={!hasField}
                onClick={() => doRetype(k)}
                title={`Change type to ${k}`}
              >
                {k}
              </button>
            ))}
          </span>
        ))}
        {vecMatKinds.length > 0 && (
          <>
            <span className="mx-0.5 h-4 w-px bg-border-soft" />
            <select
              className="rounded border border-border bg-elevated px-1 py-0.5 text-danger disabled:opacity-40"
              value=""
              disabled={!hasField}
              onChange={(e) => {
                if (e.target.value) doRetype(e.target.value);
                e.currentTarget.value = "";
              }}
              title="Vector / matrix types"
            >
              <option value="">Vec/Mat…</option>
              {vecMatKinds.map((k) => (
                <option key={k.kind} value={k.kind}>
                  {k.kind}
                </option>
              ))}
            </select>
          </>
        )}
      </div>

      {/* grid header */}
      <div
        className={`mono ${GRID} border-b border-border-soft bg-panel px-2 py-1 text-[10px] uppercase tracking-wide text-faint`}
      >
        <span />
        <span>offset</span>
        <span>address</span>
        <span>type</span>
        <span>name</span>
        <span>value</span>
      </div>

      <div className="flex-1 overflow-auto">
        {rows.map((row) => (
          <InspectorRow
            key={row.fieldIndex}
            row={row}
            ownerClass={selectedClass}
            path={[row.fieldIndex]}
            depth={0}
            expanded={expanded}
            onToggle={toggle}
            selectedKey={fieldSel?.key ?? null}
            onSelect={setFieldSel}
            onContextMenu={openMenu}
            onGuess={doGuess}
          />
        ))}
        {!rows.length && (
          <div className="px-3 py-4 text-center text-xs text-faint">
            No fields. Use “Add” to append bytes.
          </div>
        )}
      </div>

      <div className="mono border-t border-border-soft bg-panel px-2 py-0.5 text-[10px] text-faint">
        base {hexAddr(base)} · {rows.length} fields
        {result && ` · ${result.ptrSize * 8}-bit`}
        {fieldSel && ` · sel @ ${fieldSel.address.toString(16).toUpperCase()}`}
      </div>

      {/* field context menu */}
      {menu && (
        <div
          className="fixed z-50 min-w-[9rem] rounded-md border border-border bg-panel-2 py-1 text-xs shadow-lg"
          style={{ left: menu.x, top: menu.y }}
          onClick={(e) => e.stopPropagation()}
          onContextMenu={(e) => e.preventDefault()}
        >
          <MenuItem label="Copy field" run={() => copyField(menu.sel)} close={() => setMenu(null)} />
          <MenuItem label="Copy value" run={() => copyValue(menu.sel)} close={() => setMenu(null)} />
          <MenuItem
            label="Copy address"
            run={() => copyAddress(menu.sel)}
            close={() => setMenu(null)}
          />
          <div className="my-1 border-t border-border-soft" />
          <MenuItem label="Paste" run={() => doPaste(menu.sel)} close={() => setMenu(null)} />
          <div className="my-1 border-t border-border-soft" />
          <MenuItem label="Guess type" run={() => doGuess(menu.sel)} close={() => setMenu(null)} />
        </div>
      )}
    </div>
  );
}

/** A small "label ▾" dropdown of byte/field counts. */
function StepSelect({
  label,
  steps,
  onPick,
  disabled,
}: {
  label: string;
  steps: number[];
  onPick: (n: number) => void;
  disabled: boolean;
}) {
  return (
    <select
      className="rounded border border-border bg-elevated px-1 py-0.5 text-text disabled:opacity-40"
      value=""
      disabled={disabled}
      onChange={(e) => {
        if (e.target.value) onPick(Number(e.target.value));
        e.currentTarget.value = "";
      }}
      title={`${label} bytes`}
    >
      <option value="">{label}…</option>
      {steps.map((n) => (
        <option key={n} value={n}>
          {n}
        </option>
      ))}
    </select>
  );
}

function MenuItem({
  label,
  run,
  close,
}: {
  label: string;
  run: () => void;
  close: () => void;
}) {
  return (
    <button
      className="block w-full px-3 py-1 text-left text-text hover:bg-elevated"
      onClick={() => {
        run();
        close();
      }}
    >
      {label}
    </button>
  );
}
