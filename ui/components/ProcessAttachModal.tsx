import { RefreshCw, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api, type ProcessInfo } from "../lib/api";
import { useStore } from "../store";
import { Modal } from "./Modal";

export function ProcessAttachModal({ onClose }: { onClose: () => void }) {
  const [procs, setProcs] = useState<ProcessInfo[]>([]);
  const [filter, setFilter] = useState("");
  const [loading, setLoading] = useState(false);
  const guard = useStore((s) => s.guard);
  const refreshStatus = useStore((s) => s.refreshStatus);

  const load = async () => {
    setLoading(true);
    const p = await api.listProcesses().catch(() => []);
    setProcs(p);
    setLoading(false);
  };

  useEffect(() => {
    load();
  }, []);

  const filtered = useMemo(() => {
    const f = filter.trim().toLowerCase();
    if (!f) return procs;
    return procs.filter(
      (p) => p.name.toLowerCase().includes(f) || String(p.id).includes(f),
    );
  }, [procs, filter]);

  const attach = async (pid: number) => {
    const a = await guard(() => api.attachPid(pid), `Attached to PID ${pid}`);
    if (a) {
      await refreshStatus();
      onClose();
    }
  };

  return (
    <Modal title="Attach to process" width="max-w-2xl" onClose={onClose}>
      <div className="flex items-center gap-2 pb-3">
        <div className="relative flex-1">
          <Search
            size={14}
            className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-faint"
          />
          <input
            autoFocus
            className="input pl-7"
            placeholder="Filter by name or pid…"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        </div>
        <button className="btn" onClick={load} disabled={loading}>
          <RefreshCw size={14} className={loading ? "animate-spin" : ""} /> Refresh
        </button>
      </div>

      <div className="max-h-[52vh] overflow-auto rounded-md border border-border">
        <table className="mono w-full text-xs">
          <thead className="sticky top-0 bg-panel-2 text-left text-muted">
            <tr>
              <th className="px-3 py-1.5 font-medium">PID</th>
              <th className="px-3 py-1.5 font-medium">Name</th>
              <th className="px-3 py-1.5 font-medium">Parent</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((p) => (
              <tr
                key={p.id}
                className="row-hover cursor-pointer border-t border-border-soft"
                onDoubleClick={() => attach(p.id)}
              >
                <td className="px-3 py-1 text-accent">{p.id}</td>
                <td className="px-3 py-1">{p.name}</td>
                <td className="px-3 py-1 text-muted">{p.parentId}</td>
                <td className="px-2 py-1 text-right">
                  <button className="btn btn-primary py-0.5" onClick={() => attach(p.id)}>
                    Attach
                  </button>
                </td>
              </tr>
            ))}
            {!filtered.length && (
              <tr>
                <td colSpan={4} className="px-3 py-6 text-center text-muted">
                  {loading ? "Loading…" : "No matching processes"}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      <p className="pt-2 text-[11px] text-faint">
        Double-click a row or press Attach. {filtered.length} of {procs.length} processes.
      </p>
    </Modal>
  );
}
