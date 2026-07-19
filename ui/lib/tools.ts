import type { LucideIcon } from "lucide-react";
import {
  Bug,
  Binary,
  Code2,
  Radar,
  Search,
  Table2,
  Terminal,
} from "lucide-react";

export type DockDirection = "right" | "below" | "left" | "above" | "within";

export interface ToolDef {
  id: string;
  title: string;
  icon: LucideIcon;
  /** Where to place the panel when first opened. */
  dir: DockDirection;
  /** The panel to place it relative to (falls back to active group if absent). */
  ref: string;
}

/** The dockable tool panels reachable from the toolbar. */
export const TOOLS: ToolDef[] = [
  { id: "scanner", title: "Scanner", icon: Search, dir: "right", ref: "inspector" },
  { id: "cheatTable", title: "Cheat Table", icon: Table2, dir: "right", ref: "inspector" },
  { id: "spider", title: "Spider", icon: Radar, dir: "right", ref: "inspector" },
  { id: "generator", title: "Code Gen", icon: Code2, dir: "right", ref: "inspector" },
  { id: "disasm", title: "Disassembly", icon: Binary, dir: "below", ref: "console" },
  { id: "debugger", title: "Debugger", icon: Bug, dir: "below", ref: "console" },
  { id: "script", title: "Script", icon: Terminal, dir: "within", ref: "console" },
];
