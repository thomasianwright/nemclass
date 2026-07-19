// Maps dockview component names to React panel components. As each tool panel is
// built, its Placeholder entry is swapped for the real component.
import { CheatTablePanel } from "./CheatTablePanel";
import { ClassListPanel } from "./ClassListPanel";
import { ConsolePanel } from "./ConsolePanel";
import { DebuggerPanel } from "./DebuggerPanel";
import { DisasmPanel } from "./DisasmPanel";
import { GeneratorPanel } from "./GeneratorPanel";
import { InspectorPanel } from "./InspectorPanel";
import { Placeholder } from "./Placeholder";
import { ScannerPanel } from "./ScannerPanel";
import { SpiderPanel } from "./SpiderPanel";

export const panelComponents = {
  classList: () => <ClassListPanel />,
  inspector: () => <InspectorPanel />,
  console: () => <ConsolePanel />,
  scanner: () => <ScannerPanel />,
  cheatTable: () => <CheatTablePanel />,
  spider: () => <SpiderPanel />,
  generator: () => <GeneratorPanel />,
  disasm: () => <DisasmPanel />,
  debugger: () => <DebuggerPanel />,
  script: () => <Placeholder title="Script Console" />,
};
