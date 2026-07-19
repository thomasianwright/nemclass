import { Plus, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, type CheatEntry } from "../lib/api";
import { useStore } from "../store";

export function CheatTablePanel() {
  const guard = useStore((s) => s.guard);
  const kinds = useStore((s) => s.kinds);
  const attached = useStore((s) => s.attached);
  const [entries, setEntries] = useState<CheatEntry[]>([]);
  const [desc, setDesc] = useState("");
  const [addr, setAddr] = useState("");
  const [kind, setKind] = useState("I32");

  const load = async () => setEntries(await api.tableList().catch(() => []));

  useEffect(() => {
    load();
    const id = setInterval(load, 300);
    return () => clearInterval(id);
  }, []);

  const add = async () => {
    if (!addr.trim()) return;
    const ok = await guard(() => api.tableAdd(desc.trim() || "entry", addr.trim(), kind));
    if (ok !== undefined) {
      setDesc("");
      setAddr("");
      await load();
    }
  };

  return (
    <div className="panel-body flex flex-col">
      <div className="mono grid grid-cols-[1.5rem_1fr_1.4fr_5rem_7rem_1.5rem] items-center gap-2 border-b border-border-soft bg-panel px-2 py-1 text-[10px] uppercase tracking-wide text-faint">
        <span title="freeze">❄</span>
        <span>description</span>
        <span>address</span>
        <span>type</span>
        <span>value</span>
        <span />
      </div>

      <div className="flex-1 overflow-auto">
        {entries.map((e) => (
          <CheatRow key={e.index} entry={e} kinds={kinds} reload={load} />
        ))}
        {!entries.length && (
          <div className="px-3 py-4 text-center text-xs text-faint">
            No entries. Add an address below or from the scanner.
          </div>
        )}
      </div>

      <div className="mono grid grid-cols-[1fr_1.4fr_5rem_1.8rem] items-center gap-1.5 border-t border-border bg-panel-2 px-2 py-1.5">
        <input
          className="input"
          placeholder="description"
          value={desc}
          onChange={(e) => setDesc(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
        />
        <input
          className="input"
          placeholder="0x… or [<mod>+off]+off"
          value={addr}
          onChange={(e) => setAddr(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
        />
        <select className="input" value={kind} onChange={(e) => setKind(e.target.value)}>
          {kinds.map((k) => (
            <option key={k.kind} value={k.kind}>
              {k.kind}
            </option>
          ))}
        </select>
        <button className="btn btn-primary btn-icon" title="Add entry" onClick={add}>
          <Plus size={14} />
        </button>
      </div>
      {!attached && (
        <div className="mono border-t border-border-soft bg-panel px-2 py-0.5 text-[10px] text-faint">
          attach to a process to resolve addresses
        </div>
      )}
    </div>
  );
}

function CheatRow({
  entry,
  kinds,
  reload,
}: {
  entry: CheatEntry;
  kinds: { kind: string; size: number }[];
  reload: () => Promise<void>;
}) {
  const guard = useStore((s) => s.guard);
  const [editingVal, setEditingVal] = useState(false);
  const [draft, setDraft] = useState("");
  const descRef = useRef<HTMLInputElement>(null);
  const addrRef = useRef<HTMLInputElement>(null);

  const update = async (
    description = entry.description,
    address = entry.address,
    kind = entry.kind,
  ) => {
    const ok = await guard(() =>
      api.tableUpdate(entry.index, description, address, kind),
    );
    if (ok !== undefined) await reload();
  };

  const remove = async () => {
    const ok = await guard(() => api.tableRemove(entry.index));
    if (ok !== undefined) await reload();
  };

  const freeze = async (on: boolean) => {
    const ok = await guard(() => api.tableFreeze(entry.index, on));
    if (ok !== undefined) await reload();
  };

  const commitVal = async () => {
    setEditingVal(false);
    if (draft.trim()) await guard(() => api.tableWrite(entry.index, draft.trim()));
  };

  return (
    <div className="mono group row-hover grid grid-cols-[1.5rem_1fr_1.4fr_5rem_7rem_1.5rem] items-center gap-2 border-b border-border-soft px-2 py-0.5 text-xs">
      <input
        type="checkbox"
        checked={entry.frozen}
        onChange={(e) => freeze(e.target.checked)}
        title="Freeze value"
        className="accent-accent"
      />
      <input
        ref={descRef}
        className="w-full truncate bg-transparent text-text outline-none focus:text-accent"
        defaultValue={entry.description}
        onBlur={(e) => e.target.value !== entry.description && update(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      />
      <input
        ref={addrRef}
        className="w-full truncate bg-transparent text-faint outline-none focus:text-accent"
        defaultValue={entry.address}
        onBlur={(e) =>
          e.target.value !== entry.address &&
          update(entry.description, e.target.value)
        }
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      />
      <select
        className="cursor-pointer rounded bg-transparent text-accent outline-none hover:bg-elevated"
        value={entry.kind}
        onChange={(e) => update(entry.description, entry.address, e.target.value)}
      >
        {kinds.map((k) => (
          <option key={k.kind} value={k.kind}>
            {k.kind}
          </option>
        ))}
      </select>
      {editingVal ? (
        <input
          autoFocus
          className="w-full rounded bg-elevated px-1 text-text outline-none"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commitVal}
          onKeyDown={(e) => {
            if (e.key === "Enter") commitVal();
            if (e.key === "Escape") setEditingVal(false);
          }}
        />
      ) : (
        <span
          className="cursor-text truncate text-success"
          title="Double-click to edit"
          onDoubleClick={() => {
            setDraft(entry.value ?? "");
            setEditingVal(true);
          }}
        >
          {entry.value ?? "—"}
        </span>
      )}
      <button
        className="btn-ghost btn-icon hidden text-faint hover:text-danger group-hover:block"
        title="Remove"
        onClick={remove}
      >
        <Trash2 size={12} />
      </button>
    </div>
  );
}
