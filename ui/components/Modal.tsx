import { X } from "lucide-react";
import { type ReactNode, useEffect } from "react";

export function Modal({
  title,
  onClose,
  children,
  width = "max-w-lg",
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  width?: string;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-[1px]"
      onMouseDown={onClose}
    >
      <div
        className={`w-full ${width} overflow-hidden rounded-lg border border-border bg-panel shadow-2xl`}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-border bg-panel-2 px-4 py-2.5">
          <h2 className="text-sm font-semibold">{title}</h2>
          <button className="btn-ghost btn-icon" onClick={onClose} title="Close (Esc)">
            <X size={16} />
          </button>
        </div>
        <div className="p-4">{children}</div>
      </div>
    </div>
  );
}
