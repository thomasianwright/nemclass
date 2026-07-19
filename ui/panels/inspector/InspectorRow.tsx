import { ChevronDown, ChevronRight, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { api, hex, hexAddr, parseAddr, type FieldRow, type KindOption } from "../../lib/api";
import { useStore } from "../../store";

export function InspectorRow({
  row,
  ownerClass,
  path,
  depth,
  base,
  kinds,
  expanded,
  onToggle,
}: {
  row: FieldRow;
  ownerClass: string;
  path: number[];
  depth: number;
  base: number;
  kinds: KindOption[];
  expanded: Set<string>;
  onToggle: (key: string) => void;
}) {
  const classes = useStore((s) => s.classes);
  const guard = useStore((s) => s.guard);
  const mutated = useStore((s) => s.mutated);

  const [name, setName] = useState(row.name);
  const [offsetText, setOffsetText] = useState(hex(row.offset));
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");

  useEffect(() => setName(row.name), [row.name]);
  useEffect(() => setOffsetText(hex(row.offset)), [row.offset]);

  const key = path.join(".");
  const isOpen = expanded.has(key);
  const editable = row.kind !== "StrPtr";

  const commitName = async () => {
    if (name === row.name) return;
    const ok = await guard(() => api.setFieldName(ownerClass, row.fieldIndex, name));
    if (ok !== undefined) await mutated();
  };
  const commitOffset = async () => {
    const v = parseAddr(offsetText);
    if (v === null || v === row.offset) {
      setOffsetText(hex(row.offset));
      return;
    }
    const ok = await guard(() => api.setFieldOffset(ownerClass, row.fieldIndex, v));
    if (ok !== undefined) await mutated();
  };
  const changeKind = async (kind: string) => {
    const ok = await guard(() =>
      api.setFieldKind(ownerClass, row.fieldIndex, kind, row.kindMeta),
    );
    if (ok !== undefined) await mutated();
  };
  const setTarget = async (target: string) => {
    const ok = await guard(() =>
      api.setFieldKind(ownerClass, row.fieldIndex, "Ptr", target || null),
    );
    if (ok !== undefined) await mutated();
  };
  const del = async () => {
    const ok = await guard(() => api.deleteField(ownerClass, row.fieldIndex));
    if (ok !== undefined) await mutated();
  };
  const commitValue = async () => {
    setEditing(false);
    const t = draft.trim();
    if (!t) return;
    await guard(() => api.writeValue(row.address, row.kind, t));
  };

  return (
    <>
      <div className="mono group row-hover grid grid-cols-[1.2rem_4.5rem_7.5rem_6.5rem_1fr_10rem_1.5rem] items-center gap-2 border-b border-border-soft py-0.5 pr-2 text-xs">
        {/* expand chevron */}
        <div style={{ paddingLeft: depth * 12 }} className="flex justify-end">
          {row.expandable ? (
            <button
              className="text-faint hover:text-accent"
              onClick={() => onToggle(key)}
              title={isOpen ? "Collapse" : "Expand"}
            >
              {isOpen ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
            </button>
          ) : null}
        </div>

        {/* offset */}
        <input
          className="w-full bg-transparent text-faint outline-none focus:text-accent"
          value={offsetText}
          spellCheck={false}
          onChange={(e) => setOffsetText(e.target.value)}
          onBlur={commitOffset}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        />

        {/* absolute address */}
        <span className="truncate text-faint" title={hexAddr(row.address)}>
          {hexAddr(row.address)}
        </span>

        {/* type */}
        <select
          className="cursor-pointer rounded bg-transparent text-accent outline-none hover:bg-elevated"
          value={row.kind}
          onChange={(e) => changeKind(e.target.value)}
        >
          {kinds.map((k) => (
            <option key={k.kind} value={k.kind}>
              {k.kind}
            </option>
          ))}
          {!kinds.some((k) => k.kind === row.kind) && (
            <option value={row.kind}>{row.kind}</option>
          )}
        </select>

        {/* name */}
        <input
          className="w-full bg-transparent text-text outline-none focus:text-accent"
          value={name}
          spellCheck={false}
          onChange={(e) => setName(e.target.value)}
          onBlur={commitName}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        />

        {/* value / pointer-target */}
        {row.kind === "Ptr" ? (
          <div className="flex items-center gap-1 overflow-hidden">
            <select
              className="max-w-[7rem] cursor-pointer rounded bg-transparent text-success outline-none hover:bg-elevated"
              value={row.kindMeta ?? ""}
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
            onChange={(e) => setDraft(e.target.value)}
            onBlur={commitValue}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitValue();
              if (e.key === "Escape") setEditing(false);
            }}
          />
        ) : (
          <span
            className={`truncate ${editable ? "cursor-text text-success" : "text-muted"}`}
            title={row.value ?? ""}
            onDoubleClick={() => {
              if (!editable) return;
              setDraft((row.value ?? "").replace(/^"|"$/g, ""));
              setEditing(true);
            }}
          >
            {row.value ?? "—"}
          </span>
        )}

        {/* delete */}
        <button
          className="btn-ghost btn-icon hidden text-faint hover:text-danger group-hover:block"
          title="Delete field"
          onClick={del}
        >
          <Trash2 size={12} />
        </button>
      </div>

      {isOpen &&
        row.children.map((child) => (
          <InspectorRow
            key={child.fieldIndex}
            row={child}
            ownerClass={row.kindMeta ?? ownerClass}
            path={[...path, child.fieldIndex]}
            depth={depth + 1}
            base={base}
            kinds={kinds}
            expanded={expanded}
            onToggle={onToggle}
          />
        ))}
    </>
  );
}
