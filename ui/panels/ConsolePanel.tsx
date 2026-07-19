import { Terminal } from "lucide-react";

export function ConsolePanel() {
  return (
    <div className="panel-body flex flex-col items-center justify-center gap-2 text-muted">
      <Terminal size={20} className="text-faint" />
      <div className="text-xs text-faint">
        Output &amp; Lua console — wired up in the scripting phase.
      </div>
    </div>
  );
}
