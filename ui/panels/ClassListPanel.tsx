import { Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { api } from "../lib/api";
import { useStore } from "../store";

export function ClassListPanel() {
  const classes = useStore((s) => s.classes);
  const selected = useStore((s) => s.selectedClass);
  const select = useStore((s) => s.selectClass);
  const mutated = useStore((s) => s.mutated);
  const guard = useStore((s) => s.guard);
  const [newName, setNewName] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameVal, setRenameVal] = useState("");

  const add = async () => {
    const n = newName.trim();
    if (!n) return;
    const ok = await guard(() => api.addClass(n));
    if (ok !== undefined) {
      setNewName("");
      await mutated();
      select(n);
    }
  };

  const del = async (name: string) => {
    const ok = await guard(() => api.deleteClass(name));
    if (ok !== undefined) await mutated();
  };

  const commitRename = async (oldName: string) => {
    const n = renameVal.trim();
    setRenaming(null);
    if (!n || n === oldName) return;
    const ok = await guard(() => api.renameClass(oldName, n));
    if (ok !== undefined) {
      await mutated();
      select(n);
    }
  };

  return (
    <div className="panel-body flex flex-col">
      <div className="flex items-center gap-1 border-b border-border bg-panel-2 p-1.5">
        <input
          className="input"
          placeholder="New class…"
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
        />
        <button className="btn btn-primary btn-icon" title="Add class" onClick={add}>
          <Plus size={14} />
        </button>
      </div>

      <ul className="flex-1 overflow-auto py-1">
        {classes.map((c) => {
          const active = c.name === selected;
          return (
            <li
              key={c.name}
              className={`group mx-1 flex cursor-pointer items-center gap-2 rounded px-2 py-1 text-xs ${
                active ? "bg-accent-soft text-text" : "row-hover text-muted"
              }`}
              onClick={() => select(c.name)}
              onDoubleClick={() => {
                setRenaming(c.name);
                setRenameVal(c.name);
              }}
            >
              {renaming === c.name ? (
                <input
                  autoFocus
                  className="input mono py-0"
                  value={renameVal}
                  onChange={(e) => setRenameVal(e.target.value)}
                  onBlur={() => commitRename(c.name)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") commitRename(c.name);
                    if (e.key === "Escape") setRenaming(null);
                  }}
                  onClick={(e) => e.stopPropagation()}
                />
              ) : (
                <>
                  <span className="mono flex-1 truncate text-text">{c.name}</span>
                  <span className="mono text-[10px] text-faint">
                    {c.fieldCount}f · {c.size}b
                  </span>
                  <button
                    className="btn-ghost btn-icon hidden text-faint hover:text-danger group-hover:block"
                    title="Delete class"
                    onClick={(e) => {
                      e.stopPropagation();
                      del(c.name);
                    }}
                  >
                    <Trash2 size={13} />
                  </button>
                </>
              )}
            </li>
          );
        })}
        {!classes.length && (
          <li className="px-3 py-4 text-center text-xs text-faint">
            No classes yet. Add one above.
          </li>
        )}
      </ul>
    </div>
  );
}
