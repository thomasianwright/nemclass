import { ChevronDown, ChevronRight } from "lucide-react";
import { type MouseEvent as ReactMouseEvent, useEffect, useState } from "react";
import { api, type FieldRow } from "../../lib/api";
import {
  asFloat,
  asInt,
  hexPairs,
  kindColor,
  type FieldSel,
} from "../../lib/inspector";
import { useStore } from "../../store";

const GRID = "grid grid-cols-[1.1rem_3.5rem_6.5rem_5rem_9rem_1fr] items-center gap-2";

export function InspectorRow({
  row,
  ownerClass,
  path,
  depth,
  expanded,
  onToggle,
  selectedKey,
  onSelect,
  onContextMenu,
  onGuess,
}: {
  row: FieldRow;
  ownerClass: string;
  path: number[];
  depth: number;
  expanded: Set<string>;
  onToggle: (key: string) => void;
  selectedKey: string | null;
  onSelect: (sel: FieldSel) => void;
  onContextMenu: (e: ReactMouseEvent, sel: FieldSel) => void;
  onGuess: (sel: FieldSel) => void;
}) {
  const classes = useStore((s) => s.classes);
  const guard = useStore((s) => s.guard);
  const mutated = useStore((s) => s.mutated);

  const [name, setName] = useState(row.name);
  const [renaming, setRenaming] = useState(false);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");

  useEffect(() => setName(row.name), [row.name]);

  const key = path.join(".");
  const isOpen = expanded.has(key);
  const selected = selectedKey === key;
  const unaligned = row.offset % 8 !== 0;
  const isUnk = row.raw != null;
  const editable = row.kind !== "StrPtr" && row.kind !== "Ptr" && !isUnk;

  const sel: FieldSel = {
    key,
    ownerClass,
    index: row.fieldIndex,
    address: row.address,
    size: row.size,
    kind: row.kind,
    name: row.name,
    metadata: row.kindMeta,
  };

  const commitName = async () => {
    setRenaming(false);
    if (name === row.name) return;
    await guard(() => api.setFieldName(ownerClass, row.fieldIndex, name));
    await mutated();
  };
  const commitValue = async () => {
    setEditing(false);
    const t = draft.trim();
    if (!t) return;
    await guard(() => api.writeValue(row.address, row.kind, t));
  };
  const setTarget = async (target: string) => {
    await guard(() => api.retypeField(ownerClass, row.fieldIndex, "Ptr", target || null));
    await mutated();
  };

  const startRename = (e: ReactMouseEvent) => {
    e.preventDefault();
    setName(row.name);
    setRenaming(true);
  };
  const startEdit = (e: ReactMouseEvent) => {
    e.preventDefault();
    if (!editable) return;
    setDraft((row.value ?? "").replace(/^"|"$/g, ""));
    setEditing(true);
  };

  return (
    <>
      <div
        className={`mono ${GRID} border-b border-border-soft py-0.5 pr-2 text-xs ${
          selected ? "bg-accent-soft/40" : "row-hover"
        }`}
        onClick={() => onSelect(sel)}
      >
        {/* expand chevron */}
        <div style={{ paddingLeft: depth * 12 }} className="flex justify-end">
          {row.expandable ? (
            <button
              className="text-faint hover:text-accent"
              onClick={(e) => {
                e.stopPropagation();
                onToggle(key);
              }}
              title={isOpen ? "Collapse" : "Expand"}
            >
              {isOpen ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
            </button>
          ) : null}
        </div>

        {/* offset + address: right-click opens the field context menu */}
        <span
          className={`text-faint ${unaligned ? "underline decoration-danger decoration-2 underline-offset-2" : ""}`}
          title={unaligned ? "Unaligned offset" : undefined}
          onContextMenu={(e) => onContextMenu(e, sel)}
        >
          {row.offset.toString(16).toUpperCase().padStart(4, "0")}
        </span>
        <span
          className="truncate text-success"
          title={"0x" + row.address.toString(16).toUpperCase()}
          onContextMenu={(e) => onContextMenu(e, sel)}
        >
          {row.address.toString(16).toUpperCase().padStart(12, "0")}
        </span>

        {/* type (recolored like egui; changed via the toolbar) */}
        <span className={`truncate ${kindColor(row.kind)}`} title={row.kind}>
          {row.kind}
        </span>

        {/* name: right-click to rename */}
        {renaming ? (
          <input
            autoFocus
            className="w-full rounded bg-elevated px-1 text-text outline-none"
            value={name}
            spellCheck={false}
            onClick={(e) => e.stopPropagation()}
            onChange={(e) => setName(e.target.value)}
            onBlur={commitName}
            onKeyDown={(e) => {
              if (e.key === "Enter") e.currentTarget.blur();
              if (e.key === "Escape") {
                setName(row.name);
                setRenaming(false);
              }
            }}
          />
        ) : (
          <span
            className="w-full cursor-default truncate text-text"
            title={row.name || "(unnamed)"}
            onContextMenu={startRename}
          >
            {row.name || <span className="text-faint">—</span>}
          </span>
        )}

        {/* value */}
        {isUnk ? (
          <HexView row={row} onGuess={() => onGuess(sel)} />
        ) : row.kind === "Ptr" ? (
          <div className="flex items-center gap-1 overflow-hidden">
            <select
              className="max-w-[7rem] cursor-pointer rounded bg-transparent text-[#c9955f] outline-none hover:bg-elevated"
              value={row.kindMeta ?? ""}
              onClick={(e) => e.stopPropagation()}
              onChange={(e) => setTarget(e.target.value)}
              title="Pointer target class"
            >
              <option value="">(void*)</option>
              {classes.map((c) => (
                <option key={c.name} value={c.name}>
                  →{c.name}
                </option>
              ))}
            </select>
            <span className="truncate text-faint" title={row.value ?? ""}>
              {row.value}
            </span>
          </div>
        ) : editing ? (
          <input
            autoFocus
            className="w-full rounded bg-elevated px-1 text-text outline-none"
            value={draft}
            spellCheck={false}
            onClick={(e) => e.stopPropagation()}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={commitValue}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitValue();
              if (e.key === "Escape") setEditing(false);
            }}
          />
        ) : (
          <span
            className={`truncate ${editable ? kindColor(row.kind) : "text-muted"}`}
            title={row.value ?? ""}
            onContextMenu={startEdit}
          >
            {row.value ?? "—"}
          </span>
        )}
      </div>

      {isOpen &&
        row.children.map((child) => (
          <InspectorRow
            key={child.fieldIndex}
            row={child}
            ownerClass={row.kindMeta ?? ownerClass}
            path={[...path, child.fieldIndex]}
            depth={depth + 1}
            expanded={expanded}
            onToggle={onToggle}
            selectedKey={selectedKey}
            onSelect={onSelect}
            onContextMenu={onContextMenu}
            onGuess={onGuess}
          />
        ))}
    </>
  );
}

/** ReClass-style hex cell: raw bytes, an int/float reading, and a clickable "guess" hint. */
function HexView({ row, onGuess }: { row: FieldRow; onGuess: () => void }) {
  const bytes = row.raw ?? [];
  const flt = asFloat(bytes);
  return (
    <div className="flex items-center gap-2 overflow-hidden">
      <span className="truncate text-faint">{hexPairs(bytes)}</span>
      <span className="shrink-0 text-muted">{asInt(bytes)}</span>
      {flt && <span className="shrink-0 text-danger/70">≈{flt}</span>}
      {row.hint && (
        <button
          className="shrink-0 rounded bg-warn/15 px-1 text-warn hover:bg-warn/25"
          title={`Convert to ${row.hint}`}
          onClick={(e) => {
            e.stopPropagation();
            onGuess();
          }}
        >
          {row.hint}?
        </button>
      )}
    </div>
  );
}
