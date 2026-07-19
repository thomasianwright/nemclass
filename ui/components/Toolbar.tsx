import {
  FilePlus2,
  FolderOpen,
  Plug,
  Redo2,
  Save,
  Settings,
  Undo2,
  Unplug,
} from "lucide-react";
import { api } from "../lib/api";
import { doProjectNew, doProjectOpen, doProjectSave } from "../lib/project";
import { TOOLS } from "../lib/tools";
import { useStore } from "../store";

export function Toolbar({
  onAttach,
  onSettings,
  openTool,
}: {
  onAttach: () => void;
  onSettings: () => void;
  openTool: (id: string) => void;
}) {
  const project = useStore((s) => s.project);
  const attached = useStore((s) => s.attached);
  const guard = useStore((s) => s.guard);
  const mutated = useStore((s) => s.mutated);
  const refreshStatus = useStore((s) => s.refreshStatus);
  const toast = useStore((s) => s.toast);

  const detach = async () => {
    await api.detach();
    await refreshStatus();
    toast("Detached", "info");
  };

  const undo = async () => {
    const changed = await api.undo().catch(() => false);
    if (changed) await mutated();
  };
  const redo = async () => {
    const changed = await api.redo().catch(() => false);
    if (changed) await mutated();
  };

  return (
    <header className="flex h-10 shrink-0 items-center gap-1 border-b border-border bg-panel-2 px-2">
      <div className="mono flex items-center gap-1.5 pr-2 text-[13px] font-bold tracking-tight">
        <span className="text-accent">nem</span>
        <span className="text-text">class</span>
      </div>

      <Divider />

      <button className="btn btn-ghost" title="New project" onClick={doProjectNew}>
        <FilePlus2 size={14} /> New
      </button>
      <button className="btn btn-ghost" title="Open project" onClick={doProjectOpen}>
        <FolderOpen size={14} /> Open
      </button>
      <button className="btn btn-ghost" title="Save project (Ctrl+S)" onClick={doProjectSave}>
        <Save size={14} /> Save
        {project?.dirty && <span className="ml-0.5 text-warn">●</span>}
      </button>

      <Divider />

      <button className="btn btn-ghost btn-icon" title="Undo (Ctrl+Z)" onClick={undo}>
        <Undo2 size={14} />
      </button>
      <button className="btn btn-ghost btn-icon" title="Redo (Ctrl+Y)" onClick={redo}>
        <Redo2 size={14} />
      </button>

      <div className="mx-1 flex-1" />

      {/* attach status */}
      {attached ? (
        <div className="flex items-center gap-1.5">
          <span className="chip mono bg-success/15 text-success">
            <span className="h-1.5 w-1.5 rounded-full bg-success" />
            {attached.name || "process"}({attached.pid})
          </span>
          <span className="chip mono bg-elevated text-muted">
            {attached.pointerSize * 8}-bit{attached.isWine ? " · wine" : ""}
          </span>
          <button className="btn btn-ghost btn-icon" title="Detach" onClick={detach}>
            <Unplug size={14} />
          </button>
        </div>
      ) : (
        <button className="btn btn-primary" title="Attach to a process (Alt+A)" onClick={onAttach}>
          <Plug size={14} /> Attach
        </button>
      )}

      <Divider />

      {/* tool panels */}
      {TOOLS.map((t) => (
        <button
          key={t.id}
          className="btn btn-ghost btn-icon"
          title={t.title}
          onClick={() => openTool(t.id)}
        >
          <t.icon size={15} />
        </button>
      ))}

      <Divider />

      <button className="btn btn-ghost btn-icon" title="Project settings" onClick={onSettings}>
        <Settings size={15} />
      </button>
    </header>
  );
}

function Divider() {
  return <div className="mx-1 h-5 w-px bg-border" />;
}
