import { useCallback, useEffect, useRef, useState } from "react";
import {
  DockviewReact,
  type DockviewApi,
  type DockviewReadyEvent,
} from "dockview-react";
import { Toolbar } from "./components/Toolbar";
import { Toasts } from "./components/Toasts";
import { ProcessAttachModal } from "./components/ProcessAttachModal";
import { ProjectSettingsModal } from "./components/ProjectSettingsModal";
import { panelComponents } from "./panels/registry";
import { TOOLS } from "./lib/tools";
import { api, type Config } from "./lib/api";
import { doProjectSave } from "./lib/project";
import { useStore } from "./store";

function buildDefaultLayout(dv: DockviewApi) {
  const classList = dv.addPanel({ id: "classList", component: "classList", title: "Classes" });
  dv.addPanel({
    id: "inspector",
    component: "inspector",
    title: "Inspector",
    position: { referencePanel: "classList", direction: "right" },
  });
  dv.addPanel({
    id: "console",
    component: "console",
    title: "Console",
    position: { referencePanel: "inspector", direction: "below" },
  });
  try {
    classList.group.api.setSize({ width: 240 });
  } catch {
    /* older layout api */
  }
}

export default function App() {
  const dockApi = useRef<DockviewApi | null>(null);
  const saveTimer = useRef<number | null>(null);
  const [attachOpen, setAttachOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [config, setConfig] = useState<Config | null | undefined>(undefined);

  const init = useStore((s) => s.init);
  const mutated = useStore((s) => s.mutated);

  useEffect(() => {
    init();
    api.getConfig().then(setConfig).catch(() => setConfig(null));
  }, [init]);

  const onReady = (event: DockviewReadyEvent) => {
    dockApi.current = event.api;
    let restored = false;
    if (config?.layout) {
      try {
        event.api.fromJSON(JSON.parse(config.layout));
        restored = true;
      } catch {
        /* fall back to default */
      }
    }
    if (!restored) buildDefaultLayout(event.api);

    event.api.onDidLayoutChange(() => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(() => {
        try {
          api.setLayout(JSON.stringify(event.api.toJSON()));
        } catch {
          /* ignore */
        }
      }, 800);
    });
  };

  const openTool = useCallback((id: string) => {
    const dv = dockApi.current;
    if (!dv) return;
    const existing = dv.getPanel(id);
    if (existing) {
      existing.api.setActive();
      return;
    }
    const tool = TOOLS.find((t) => t.id === id);
    const ref = tool && dv.getPanel(tool.ref) ? tool.ref : undefined;
    dv.addPanel({
      id,
      component: id,
      title: tool?.title ?? id,
      position: ref ? { referencePanel: ref, direction: tool!.dir } : undefined,
    });
  }, []);

  // Global keyboard shortcuts.
  useEffect(() => {
    const editable = (el: EventTarget | null) => {
      const n = el as HTMLElement | null;
      return (
        !!n &&
        (n.tagName === "INPUT" ||
          n.tagName === "TEXTAREA" ||
          n.isContentEditable ||
          n.closest(".monaco-editor") != null)
      );
    };
    const onKey = async (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === "s") {
        e.preventDefault();
        doProjectSave();
      } else if (e.altKey && e.key.toLowerCase() === "a") {
        e.preventDefault();
        setAttachOpen(true);
      } else if (mod && e.key.toLowerCase() === "z" && !e.shiftKey && !editable(e.target)) {
        e.preventDefault();
        if (await api.undo().catch(() => false)) await mutated();
      } else if (
        (mod && e.key.toLowerCase() === "y") ||
        (mod && e.shiftKey && e.key.toLowerCase() === "z")
      ) {
        if (editable(e.target)) return;
        e.preventDefault();
        if (await api.redo().catch(() => false)) await mutated();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [mutated]);

  return (
    <div className="flex h-full flex-col bg-bg">
      <Toolbar
        onAttach={() => setAttachOpen(true)}
        onSettings={() => setSettingsOpen(true)}
        openTool={openTool}
      />
      <div className="min-h-0 flex-1">
        {config !== undefined && (
          <DockviewReact
            className="dockview-theme-abyss"
            components={panelComponents}
            onReady={onReady}
          />
        )}
      </div>
      {attachOpen && <ProcessAttachModal onClose={() => setAttachOpen(false)} />}
      {settingsOpen && (
        <ProjectSettingsModal onClose={() => setSettingsOpen(false)} />
      )}
      <Toasts />
    </div>
  );
}
