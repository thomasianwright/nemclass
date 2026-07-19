import { Zap } from "lucide-react";
import { useEffect, useState } from "react";
import { api, type Manifest } from "../lib/api";
import { useStore } from "../store";
import { Modal } from "./Modal";

export function ProjectSettingsModal({ onClose }: { onClose: () => void }) {
  const [name, setName] = useState("");
  const [autoAttach, setAutoAttach] = useState(false);
  const [processName, setProcessName] = useState("");
  const [moduleName, setModuleName] = useState("");
  const guard = useStore((s) => s.guard);
  const refreshStatus = useStore((s) => s.refreshStatus);

  useEffect(() => {
    api.getManifest().then((m) => {
      setName(m.name);
      if (m.auto_attach) {
        setAutoAttach(true);
        setProcessName(m.auto_attach.process_name);
        setModuleName(m.auto_attach.module_name ?? "");
      }
    });
  }, []);

  const save = async (thenAttach: boolean) => {
    const manifest: Manifest = {
      name: name.trim() || "Untitled",
      auto_attach:
        autoAttach && processName.trim()
          ? {
              process_name: processName.trim(),
              module_name: moduleName.trim() || null,
            }
          : null,
    };
    const ok = await guard(() => api.setManifest(manifest), "Settings saved");
    if (ok === undefined) return;
    await refreshStatus();
    if (thenAttach && manifest.auto_attach) {
      const a = await guard(() => api.autoAttach(), "Auto-attached");
      if (a) await refreshStatus();
    }
    onClose();
  };

  return (
    <Modal title="Project settings" onClose={onClose}>
      <div className="flex flex-col gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs text-muted">Project name</span>
          <input
            autoFocus
            className="input"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </label>

        <label className="flex items-center gap-2 pt-1">
          <input
            type="checkbox"
            checked={autoAttach}
            onChange={(e) => setAutoAttach(e.target.checked)}
          />
          <span className="text-xs">Enable auto-attach</span>
        </label>

        {autoAttach && (
          <div className="flex flex-col gap-3 rounded-md border border-border bg-panel-2 p-3">
            <label className="flex flex-col gap-1">
              <span className="text-xs text-muted">Process name</span>
              <input
                className="input mono"
                placeholder="game.exe / wine64-preloader"
                value={processName}
                onChange={(e) => setProcessName(e.target.value)}
              />
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-xs text-muted">Module filter (optional)</span>
              <input
                className="input mono"
                placeholder="test.dll"
                value={moduleName}
                onChange={(e) => setModuleName(e.target.value)}
              />
            </label>
          </div>
        )}

        <div className="flex justify-end gap-2 pt-2">
          <button className="btn" onClick={() => save(false)}>
            Save
          </button>
          <button
            className="btn btn-primary"
            disabled={!autoAttach || !processName.trim()}
            onClick={() => save(true)}
          >
            <Zap size={14} /> Save &amp; Attach
          </button>
        </div>
      </div>
    </Modal>
  );
}
