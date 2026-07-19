// Project new / open / save flows. Directory selection is done by the in-app
// DirectoryPicker (App wires it up), NOT the GTK native dialog — the native
// folder chooser crashes on systems with a broken SVG icon loader.
import { api } from "./api";
import { useStore } from "../store";

function basename(dir: string): string {
  const parts = dir.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || "project";
}

/** Creates + opens a new project in `dir`. */
export async function applyProjectNew(dir: string, name?: string) {
  const st = useStore.getState();
  const status = await st.guard(
    () => api.projectNew(dir, name?.trim() || basename(dir)),
    `Created project in ${dir}`,
  );
  if (status) {
    st.setProject(status);
    await st.mutated();
  }
}

/** Opens an existing project folder; returns whether it succeeded. */
export async function applyProjectOpen(dir: string): Promise<boolean> {
  const st = useStore.getState();
  const isProj = await api.isProjectDir(dir).catch(() => false);
  if (!isProj) {
    st.toast("That folder has no project.nemproj manifest", "error");
    return false;
  }
  const status = await st.guard(() => api.projectOpen(dir), `Opened ${dir}`);
  if (status) {
    st.setProject(status);
    await st.mutated();
    return true;
  }
  return false;
}

/** Saves the current project to `dir` and adopts it. */
export async function applyProjectSaveAs(dir: string) {
  const st = useStore.getState();
  const status = await st.guard(() => api.projectSaveAs(dir), `Saved to ${dir}`);
  if (status) st.setProject(status);
}

/** Saves to the existing project dir; returns false if there is none (needs Save As). */
export async function saveExisting(): Promise<boolean> {
  const st = useStore.getState();
  if (st.project?.dir) {
    const status = await st.guard(() => api.projectSave(), "Project saved");
    if (status) st.setProject(status);
    return true;
  }
  return false;
}
