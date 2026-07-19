// Maps dockview component names to React panel components. As each tool panel is
// built, its Placeholder entry is swapped for the real component.
import { ClassListPanel } from "./ClassListPanel";
import { ConsolePanel } from "./ConsolePanel";
import { InspectorPanel } from "./InspectorPanel";
import { Placeholder } from "./Placeholder";

export const panelComponents = {
  classList: () => <ClassListPanel />,
  inspector: () => <InspectorPanel />,
  console: () => <ConsolePanel />,
  scanner: () => <Placeholder title="Memory Scanner" />,
  cheatTable: () => <Placeholder title="Cheat Table" />,
  spider: () => <Placeholder title="Spider" />,
  generator: () => <Placeholder title="Code Generator" />,
  disasm: () => <Placeholder title="Disassembly" />,
  debugger: () => <Placeholder title="Debugger" />,
  script: () => <Placeholder title="Script Console" />,
};
