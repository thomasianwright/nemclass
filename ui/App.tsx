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
import { useStore } from "./store";

export default function App() {
  const dockApi = useRef<DockviewApi | null>(null);
  const [attachOpen, setAttachOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const init = useStore((s) => s.init);

  useEffect(() => {
    init();
  }, [init]);

  const onReady = (event: DockviewReadyEvent) => {
    dockApi.current = event.api;

    const classList = event.api.addPanel({
      id: "classList",
      component: "classList",
      title: "Classes",
    });
    event.api.addPanel({
      id: "inspector",
      component: "inspector",
      title: "Inspector",
      position: { referencePanel: "classList", direction: "right" },
    });
    event.api.addPanel({
      id: "console",
      component: "console",
      title: "Console",
      position: { referencePanel: "inspector", direction: "below" },
    });

    // Give the class list a narrow default width.
    try {
      classList.group.api.setSize({ width: 240 });
    } catch {
      /* older layout api */
    }
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

  return (
    <div className="flex h-full flex-col bg-bg">
      <Toolbar
        onAttach={() => setAttachOpen(true)}
        onSettings={() => setSettingsOpen(true)}
        openTool={openTool}
      />
      <div className="min-h-0 flex-1">
        <DockviewReact
          className="dockview-theme-abyss"
          components={panelComponents}
          onReady={onReady}
        />
      </div>
      {attachOpen && <ProcessAttachModal onClose={() => setAttachOpen(false)} />}
      {settingsOpen && (
        <ProjectSettingsModal onClose={() => setSettingsOpen(false)} />
      )}
      <Toasts />
    </div>
  );
}
