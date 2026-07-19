import { AlertCircle, CheckCircle2, Info, X } from "lucide-react";
import { useStore } from "../store";

const ICON = {
  info: Info,
  error: AlertCircle,
  success: CheckCircle2,
} as const;

const TONE = {
  info: "border-accent-soft text-text",
  error: "border-danger/40 text-danger",
  success: "border-success/40 text-success",
} as const;

export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  const dismiss = useStore((s) => s.dismissToast);

  return (
    <div className="pointer-events-none fixed bottom-3 right-3 z-[60] flex w-80 flex-col gap-2">
      {toasts.map((t) => {
        const Icon = ICON[t.kind];
        return (
          <div
            key={t.id}
            className={`pointer-events-auto flex items-start gap-2 rounded-md border bg-elevated px-3 py-2 text-xs shadow-lg ${TONE[t.kind]}`}
          >
            <Icon size={15} className="mt-0.5 shrink-0" />
            <span className="mono flex-1 break-words text-text">{t.message}</span>
            <button className="btn-ghost btn-icon -mr-1 -mt-1" onClick={() => dismiss(t.id)}>
              <X size={13} />
            </button>
          </div>
        );
      })}
    </div>
  );
}
