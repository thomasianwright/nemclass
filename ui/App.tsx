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
import { DirectoryPicker } from "./components/DirectoryPicker";
import { panelComponents } from "./panels/registry";
import { TOOLS } from "./lib/tools";
import { api, type Config } from "./lib/api";
import {
  applyProjectNew,
  applyProjectOpen,
  applyProjectSaveAs,
  saveExisting,
} from "./lib/project";
import { useStore } from "./store";

type PickerMode = "open" | "new" | "save";

// The always-present panels. Everything else is an on-demand tool (see lib/tools).
const CORE_PANELS = [
  { id: "classList", component: "classList", title: "Classes" },
  { id: "inspector", component: "inspector", title: "Inspector", ref: "classList", dir: "right" },
  { id: "console", component: "console", title: "Console", ref: "inspector", dir: "below" },
] as const;

function addCorePanel(dv: DockviewApi, p: (typeof CORE_PANELS)[number]) {
  dv.addPanel({
    id: p.id,
    component: p.component,
    title: p.title,
    position:
      "ref" in p && dv.getPanel(p.ref) ? { referencePanel: p.ref, direction: p.dir } : undefined,
  });
}

function buildDefaultLayout(dv: DockviewApi) {
  CORE_PANELS.forEach((p) => addCorePanel(dv, p));
  try {
    dv.getPanel("classList")?.group.api.setSize({ width: 240 });
  } catch {
    /* older layout api */
  }
}

// Re-add any core panel missing from a restored layout. Older/edited layouts could
// be persisted without the Inspector, which otherwise left it permanently hidden.
function ensureCorePanels(dv: DockviewApi) {
  CORE_PANELS.forEach((p) => {
    if (!dv.getPanel(p.id)) addCorePanel(dv, p);
  });
}

export default function App() {
  const dockApi = useRef<DockviewApi | null>(null);
  const saveTimer = useRef<number | null>(null);
  const [attachOpen, setAttachOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [picker, setPicker] = useState<PickerMode | null>(null);
  const [config, setConfig] = useState<Config | null | undefined>(undefined);

  const init = useStore((s) => s.init);
  const mutated = useStore((s) => s.mutated);

  const onProject = useCallback(async (mode: PickerMode) => {
    // Save writes to the existing dir directly; only fall back to the picker when
    // there is no project directory yet.
    if (mode === "save" && (await saveExisting())) return;
    setPicker(mode);
  }, []);

  const handlePick = useCallback(
    async (dir: string, name?: string) => {
      const mode = picker;
      setPicker(null);
      if (mode === "new") await applyProjectNew(dir, name);
      else if (mode === "open") await applyProjectOpen(dir);
      else if (mode === "save") await applyProjectSaveAs(dir);
    },
    [picker],
  );

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
    else ensureCorePanels(event.api);

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

  // Wipe the (possibly broken) layout and rebuild the default one.
  const resetLayout = useCallback(() => {
    const dv = dockApi.current;
    if (!dv) return;
    dv.clear();
    buildDefaultLayout(dv);
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
        onProject("save");
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
  }, [mutated, onProject]);

  return (
    <div className="flex h-full flex-col bg-bg">
      <Toolbar
        onAttach={() => setAttachOpen(true)}
        onSettings={() => setSettingsOpen(true)}
        onProject={onProject}
        openTool={openTool}
        onResetLayout={resetLayout}
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
      {picker && (
        <DirectoryPicker
          mode={picker}
          onClose={() => setPicker(null)}
          onPick={handlePick}
        />
      )}
      <Toasts />
    </div>
  );
}
