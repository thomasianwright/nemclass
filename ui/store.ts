import { create } from "zustand";
import {
  api,
  type Attached,
  type ClassSummary,
  type KindOption,
  type ProjectStatus,
} from "./lib/api";

export type ToastKind = "info" | "error" | "success";
export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

interface AppStore {
  attached: Attached | null;
  project: ProjectStatus | null;
  classes: ClassSummary[];
  selectedClass: string | null;
  kinds: KindOption[];
  toasts: Toast[];
  /** Bumped after any class/field mutation so panels re-fetch. */
  classRev: number;

  init: () => Promise<void>;
  refreshStatus: () => Promise<void>;
  refreshClasses: () => Promise<void>;
  /** Refreshes the class list and bumps `classRev`. Call after any edit. */
  mutated: () => Promise<void>;
  selectClass: (name: string | null) => void;
  setAttached: (a: Attached | null) => void;
  setProject: (p: ProjectStatus | null) => void;
  toast: (message: string, kind?: ToastKind) => void;
  dismissToast: (id: number) => void;
  /** Runs an async action, surfacing any error as a toast. */
  guard: <T>(fn: () => Promise<T>, okMsg?: string) => Promise<T | undefined>;
}

let toastSeq = 1;

export const useStore = create<AppStore>((set, get) => ({
  attached: null,
  project: null,
  classes: [],
  selectedClass: null,
  kinds: [],
  toasts: [],
  classRev: 0,

  init: async () => {
    const kinds = await api.fieldKinds().catch(() => []);
    set({ kinds });
    await get().refreshStatus();
    await get().refreshClasses();
  },

  refreshStatus: async () => {
    const project = await api.projectStatus().catch(() => null);
    set({ project, attached: project?.attached ?? null });
  },

  refreshClasses: async () => {
    const classes = await api.listClasses().catch(() => []);
    set((s) => {
      const stillThere =
        s.selectedClass && classes.some((c) => c.name === s.selectedClass);
      return {
        classes,
        selectedClass: stillThere
          ? s.selectedClass
          : classes.length
            ? classes[0].name
            : null,
      };
    });
  },

  mutated: async () => {
    await get().refreshClasses();
    set((s) => ({ classRev: s.classRev + 1 }));
  },

  selectClass: (name) => set({ selectedClass: name }),
  setAttached: (attached) => set({ attached }),
  setProject: (project) => set({ project, attached: project?.attached ?? null }),

  toast: (message, kind = "info") => {
    const id = toastSeq++;
    set((s) => ({ toasts: [...s.toasts, { id, kind, message }] }));
    const ttl = kind === "error" ? 6000 : 3000;
    setTimeout(() => get().dismissToast(id), ttl);
  },

  dismissToast: (id) =>
    set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

  guard: async (fn, okMsg) => {
    try {
      const r = await fn();
      if (okMsg) get().toast(okMsg, "success");
      return r;
    } catch (e) {
      get().toast(String(e), "error");
      return undefined;
    }
  },
}));
