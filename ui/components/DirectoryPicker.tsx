import { ArrowUp, Folder, FolderGit2, Home } from "lucide-react";
import { useEffect, useState } from "react";
import { api, type DirListing } from "../lib/api";
import { Modal } from "./Modal";

type Mode = "open" | "new" | "save";

const CONFIG: Record<Mode, { title: string; action: string; needsName: boolean }> = {
  open: { title: "Open project", action: "Open folder", needsName: false },
  new: { title: "New project", action: "Create here", needsName: true },
  save: { title: "Save project as", action: "Save here", needsName: true },
};

export function DirectoryPicker({
  mode,
  onClose,
  onPick,
}: {
  mode: Mode;
  onClose: () => void;
  onPick: (dir: string, name?: string) => void;
}) {
  const cfg = CONFIG[mode];
  const [listing, setListing] = useState<DirListing | null>(null);
  const [name, setName] = useState("");

  const load = async (path: string | null) => {
    const l = await api.listDir(path).catch(() => null);
    if (l) setListing(l);
  };

  useEffect(() => {
    load(null);
  }, []);

  const goHome = async () => load(await api.homeDir().catch(() => null));

  const pickCurrent = () => {
    if (!listing) return;
    onPick(listing.path, cfg.needsName ? name : undefined);
  };

  return (
    <Modal title={cfg.title} width="max-w-2xl" onClose={onClose}>
      {/* path bar */}
      <div className="flex items-center gap-1.5 pb-2">
        <button className="btn btn-ghost btn-icon" title="Home" onClick={goHome}>
          <Home size={14} />
        </button>
        <button
          className="btn btn-ghost btn-icon"
          title="Up"
          disabled={!listing?.parent}
          onClick={() => listing?.parent && load(listing.parent)}
        >
          <ArrowUp size={14} />
        </button>
        <div className="mono flex-1 truncate rounded border border-border bg-panel px-2 py-1 text-xs text-muted">
          {listing?.path ?? "…"}
        </div>
      </div>

      {/* directory list */}
      <div className="max-h-[46vh] overflow-auto rounded-md border border-border">
        {listing?.dirs.map((d) => (
          <button
            key={d.path}
            className="mono row-hover flex w-full items-center gap-2 px-3 py-1 text-left text-xs"
            onClick={() => load(d.path)}
            onDoubleClick={() => {
              if (mode === "open" && d.isProject) onPick(d.path);
              else load(d.path);
            }}
          >
            {d.isProject ? (
              <FolderGit2 size={14} className="text-accent" />
            ) : (
              <Folder size={14} className="text-faint" />
            )}
            <span className="flex-1 truncate text-text">{d.name}</span>
            {d.isProject && <span className="chip bg-accent-soft text-accent">project</span>}
          </button>
        ))}
        {listing && !listing.dirs.length && (
          <div className="px-3 py-4 text-center text-xs text-faint">No sub-folders here.</div>
        )}
      </div>

      {/* footer */}
      <div className="flex items-center gap-2 pt-3">
        {cfg.needsName && (
          <input
            className="input mono flex-1"
            placeholder="project name (optional)"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && pickCurrent()}
          />
        )}
        <div className="flex-1" />
        <button className="btn" onClick={onClose}>
          Cancel
        </button>
        <button className="btn btn-primary" onClick={pickCurrent}>
          {cfg.action}
        </button>
      </div>
      <p className="pt-1 text-[11px] text-faint">
        {mode === "open"
          ? "Double-click a project folder, or navigate in and press Open."
          : "Navigate to the target folder, then press the button."}
      </p>
    </Modal>
  );
}
