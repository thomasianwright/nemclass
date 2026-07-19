// Project new / open / save flows, using the Tauri dialog plugin for folder
// picking and surfacing outcomes through the store's toasts.
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import { useStore } from "../store";

function basename(dir: string): string {
  const parts = dir.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || "project";
}

export async function doProjectNew() {
  const st = useStore.getState();
  const dir = await open({
    directory: true,
    multiple: false,
    title: "Choose an empty folder for the new project",
  });
  if (!dir || typeof dir !== "string") return;
  const status = await st.guard(
    () => api.projectNew(dir, basename(dir)),
    `Created project in ${dir}`,
  );
  if (status) {
    st.setProject(status);
    await st.mutated();
  }
}

export async function doProjectOpen() {
  const st = useStore.getState();
  const dir = await open({
    directory: true,
    multiple: false,
    title: "Open project folder",
  });
  if (!dir || typeof dir !== "string") return;
  const isProj = await api.isProjectDir(dir).catch(() => false);
  if (!isProj) {
    st.toast("That folder has no project.nemproj manifest", "error");
    return;
  }
  const status = await st.guard(() => api.projectOpen(dir), `Opened ${dir}`);
  if (status) {
    st.setProject(status);
    await st.mutated();
  }
}

export async function doProjectSave() {
  const st = useStore.getState();
  if (st.project?.dir) {
    const status = await st.guard(() => api.projectSave(), "Project saved");
    if (status) st.setProject(status);
  } else {
    await doProjectSaveAs();
  }
}

export async function doProjectSaveAs() {
  const st = useStore.getState();
  const dir = await open({
    directory: true,
    multiple: false,
    title: "Choose a folder to save the project",
  });
  if (!dir || typeof dir !== "string") return;
  const status = await st.guard(
    () => api.projectSaveAs(dir),
    `Saved to ${dir}`,
  );
  if (status) st.setProject(status);
}
