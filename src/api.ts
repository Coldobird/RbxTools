import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export interface Status {
  steamPath: string | null;
  steamRunning: boolean;
  targetPath: string | null;
  blocked: boolean;
  elevated: boolean;
}

export interface LocalUpdate {
  version: string;
  notes: string;
}

export const desktop = isTauri();
const previewStatus: Status = {
  steamPath: null,
  steamRunning: false,
  targetPath: null,
  blocked: false,
  elevated: false,
};

export async function getStatus(): Promise<Status> {
  return desktop ? invoke("get_status") : previewStatus;
}

export async function pickExecutable(
  kind: "steam" | "target",
): Promise<Status | null> {
  if (!desktop)
    throw new Error(
      "Open the desktop app to choose an executable. This browser preview cannot change your computer.",
    );
  const path = await open({
    multiple: false,
    directory: false,
    title:
      kind === "steam"
        ? "Locate steam.exe"
        : "Locate steam.exe (the NetLimiter target)",
    filters: [{ name: "Windows application", extensions: ["exe"] }],
  });
  if (!path) return null;
  return invoke("set_path", { kind, path });
}

export async function restartSteam(force: boolean): Promise<string> {
  if (!desktop)
    throw new Error("Restart is available in the installed desktop app.");
  return invoke("restart_steam", { force });
}

export async function setBlocked(blocked: boolean): Promise<Status> {
  if (!desktop)
    throw new Error(
      "Network controls are available in the installed desktop app.",
    );
  return invoke("set_blocked", { blocked });
}

export async function getLocalUpdate(): Promise<LocalUpdate | null> {
  return desktop ? invoke("get_local_update") : null;
}

export async function installLocalUpdate(): Promise<void> {
  if (!desktop) throw new Error("Updates are available in the installed desktop app.");
  return invoke("install_local_update");
}

export function fileName(path: string | null): string {
  return path?.split(/[\\/]/).pop() || "No executable selected";
}
