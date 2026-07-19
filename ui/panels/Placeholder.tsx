import { Hammer } from "lucide-react";

export function Placeholder({ title }: { title: string }) {
  return (
    <div className="panel-body flex flex-col items-center justify-center gap-2 text-muted">
      <Hammer size={22} className="text-faint" />
      <div className="text-sm text-text">{title}</div>
      <div className="text-xs text-faint">Wired up in a later build phase.</div>
    </div>
  );
}
