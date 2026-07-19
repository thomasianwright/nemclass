// Maps dockview component names to React panel components. As each tool panel is
// built, its Placeholder entry is swapped for the real component.
import { CheatTablePanel } from "./CheatTablePanel";
import { ClassListPanel } from "./ClassListPanel";
import { ConsolePanel } from "./ConsolePanel";
import { DisasmPanel } from "./DisasmPanel";
import { GeneratorPanel } from "./GeneratorPanel";
import { InspectorPanel } from "./InspectorPanel";
import { Placeholder } from "./Placeholder";
import { ScannerPanel } from "./ScannerPanel";

export const panelComponents = {
  classList: () => <ClassListPanel />,
  inspector: () => <InspectorPanel />,
  console: () => <ConsolePanel />,
  scanner: () => <ScannerPanel />,
  cheatTable: () => <CheatTablePanel />,
  spider: () => <Placeholder title="Spider" />,
  generator: () => <GeneratorPanel />,
  disasm: () => <DisasmPanel />,
  debugger: () => <Placeholder title="Debugger" />,
  script: () => <Placeholder title="Script Console" />,
};
